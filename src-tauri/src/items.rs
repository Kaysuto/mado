//! Items: the single shape every source is translated into (a mail, a ticket, an event…).

use crate::error::Result;
use rusqlite::{Connection, Row, params, params_from_iter, types::Value};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Source name of what the user creates in Madow itself. Never removed by a sync.
pub const LOCAL_SOURCE: &str = "local";

/// What an item is, whichever service it comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Mail,
    Ticket,
    MergeRequest,
    Notification,
    Event,
    Task,
    Note,
    Message,
    Alert,
    Article,
}

/// Where the user stands with an item. Local only: a sync never changes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Triage {
    Inbox,
    Done,
    Later,
}

// Both enums are stored as the same lowercase text the interface receives.
macro_rules! stored_as_text {
    ($type:ty) => {
        impl rusqlite::ToSql for $type {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                let text = serde_json::to_value(self)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
                Ok(text.as_str().unwrap_or_default().to_owned().into())
            }
        }
        impl rusqlite::types::FromSql for $type {
            fn column_result(
                value: rusqlite::types::ValueRef<'_>,
            ) -> rusqlite::types::FromSqlResult<Self> {
                serde_json::from_value(value.as_str()?.into())
                    .map_err(|e| rusqlite::types::FromSqlError::Other(Box::new(e)))
            }
        }
    };
}
stored_as_text!(Kind);
stored_as_text!(Triage);

/// An item as a connector produces it.
#[derive(Debug, Clone, PartialEq)]
pub struct NewItem {
    /// Identifier of the item inside its source; stable across syncs.
    pub external_id: String,
    pub kind: Kind,
    pub title: String,
    pub body: String,
    pub author: Option<String>,
    pub url: Option<String>,
    /// State in the source's own words ("unread", "in progress", "merged"…).
    pub state: String,
    /// Unix seconds: when it was received, created or starts.
    pub occurred_at: i64,
    /// Unix seconds: end of an event, due date of a task.
    pub ends_at: Option<i64>,
    /// Source-specific extras the interface may show (labels, CI status…).
    pub meta: serde_json::Value,
}

/// An item as stored and sent to the interface.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: i64,
    pub source: String,
    pub external_id: String,
    pub kind: Kind,
    pub title: String,
    pub body: String,
    pub author: Option<String>,
    pub url: Option<String>,
    pub state: String,
    pub triage: Triage,
    pub occurred_at: i64,
    pub ends_at: Option<i64>,
    pub meta: serde_json::Value,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Query {
    /// Empty means every kind.
    pub kinds: Vec<Kind>,
    pub triage: Option<Triage>,
    /// Words to find in title, body or author; the last one may be a prefix.
    pub search: String,
    pub limit: Option<u32>,
}

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

/// Makes the stored items of `source` match `items`: adds, updates, and removes what is gone.
/// The user's triage of an item that is still there is kept.
pub fn replace_source(conn: &mut Connection, source: &str, items: &[NewItem]) -> Result<()> {
    let tx = conn.transaction()?;
    let stamp: i64 = tx.query_row(
        "SELECT COALESCE(MAX(synced_at), 0) + 1 FROM items WHERE source = ?1",
        [source],
        |row| row.get(0),
    )?;
    {
        let mut upsert = tx.prepare(
            "INSERT INTO items (source, external_id, kind, title, body, author, url, state,
                                occurred_at, ends_at, meta, synced_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             ON CONFLICT (source, external_id) DO UPDATE SET
                kind = excluded.kind, title = excluded.title, body = excluded.body,
                author = excluded.author, url = excluded.url, state = excluded.state,
                occurred_at = excluded.occurred_at, ends_at = excluded.ends_at,
                meta = excluded.meta, synced_at = excluded.synced_at",
        )?;
        for item in items {
            upsert.execute(params![
                source,
                item.external_id,
                item.kind,
                item.title,
                item.body,
                item.author,
                item.url,
                item.state,
                item.occurred_at,
                item.ends_at,
                item.meta.to_string(),
                stamp,
            ])?;
        }
    }
    tx.execute(
        "DELETE FROM items WHERE source = ?1 AND synced_at <> ?2",
        params![source, stamp],
    )?;
    tx.commit()?;
    Ok(())
}

/// Removes the items of every source that is no longer configured.
pub fn retain_sources(conn: &Connection, sources: &[&str]) -> Result<()> {
    let keep = sources.iter().copied().chain([LOCAL_SOURCE]);
    let marks = vec!["?"; sources.len() + 1].join(", ");
    conn.execute(
        &format!("DELETE FROM items WHERE source NOT IN ({marks})"),
        params_from_iter(keep),
    )?;
    Ok(())
}

pub fn set_triage(conn: &Connection, id: i64, triage: Triage) -> Result<()> {
    conn.execute(
        "UPDATE items SET triage = ?2 WHERE id = ?1",
        params![id, triage],
    )?;
    Ok(())
}

/// The lowercase text an enum is stored as.
fn stored_text(value: impl Serialize) -> Result<Value> {
    let text = serde_json::to_value(value)?;
    Ok(text.as_str().unwrap_or_default().to_owned().into())
}

pub fn list(conn: &Connection, query: &Query) -> Result<Vec<Item>> {
    let mut sql = String::from(
        "SELECT items.id, source, external_id, kind, items.title, items.body, items.author, url,
                state, triage, occurred_at, ends_at, meta
         FROM items",
    );
    let mut values: Vec<Value> = Vec::new();
    let mut filters: Vec<String> = Vec::new();

    let search = fts_query(&query.search);
    if let Some(search) = &search {
        sql.push_str(" JOIN items_fts ON items_fts.rowid = items.id");
        filters.push("items_fts MATCH ?".into());
        values.push(search.clone().into());
    }
    if !query.kinds.is_empty() {
        filters.push(format!(
            "kind IN ({})",
            vec!["?"; query.kinds.len()].join(", ")
        ));
        for kind in &query.kinds {
            values.push(stored_text(kind)?);
        }
    }
    if let Some(triage) = query.triage {
        filters.push("triage = ?".into());
        values.push(stored_text(triage)?);
    }
    if !filters.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&filters.join(" AND "));
    }
    // Best match first when searching, most recent first otherwise.
    sql.push_str(if search.is_some() {
        " ORDER BY rank, occurred_at DESC"
    } else {
        " ORDER BY occurred_at DESC"
    });
    sql.push_str(" LIMIT ?");
    values.push(i64::from(query.limit.unwrap_or(200).min(1000)).into());

    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(values), read_item)?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

fn read_item(row: &Row<'_>) -> rusqlite::Result<Item> {
    let meta: String = row.get(12)?;
    Ok(Item {
        id: row.get(0)?,
        source: row.get(1)?,
        external_id: row.get(2)?,
        kind: row.get(3)?,
        title: row.get(4)?,
        body: row.get(5)?,
        author: row.get(6)?,
        url: row.get(7)?,
        state: row.get(8)?,
        triage: row.get(9)?,
        occurred_at: row.get(10)?,
        ends_at: row.get(11)?,
        meta: serde_json::from_str(&meta).unwrap_or_default(),
    })
}

/// Turns what the user typed into a full-text query: every word must match, the last one as a
/// prefix so results follow the typing. Words are quoted, so no character is an operator.
fn fts_query(input: &str) -> Option<String> {
    let words: Vec<String> = input
        .split_whitespace()
        .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
        .collect();
    let (last, rest) = words.split_last()?;
    Some(
        rest.iter()
            .cloned()
            .chain([format!("{last}*")])
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn item(external_id: &str, kind: Kind, title: &str, occurred_at: i64) -> NewItem {
        NewItem {
            external_id: external_id.into(),
            kind,
            title: title.into(),
            body: String::new(),
            author: None,
            url: None,
            state: String::new(),
            occurred_at,
            ends_at: None,
            meta: serde_json::json!({}),
        }
    }

    fn titles(conn: &Connection, query: Query) -> Vec<String> {
        list(conn, &query)
            .unwrap()
            .into_iter()
            .map(|i| i.title)
            .collect()
    }

    #[test]
    fn replace_adds_updates_and_removes() {
        let mut conn = open_in_memory();
        replace_source(
            &mut conn,
            "a",
            &[
                item("1", Kind::Mail, "First", 10),
                item("2", Kind::Mail, "Second", 20),
            ],
        )
        .unwrap();
        replace_source(
            &mut conn,
            "a",
            &[
                item("2", Kind::Mail, "Second, edited", 20),
                item("3", Kind::Ticket, "Third", 30),
            ],
        )
        .unwrap();
        assert_eq!(titles(&conn, Query::default()), ["Third", "Second, edited"]);
    }

    #[test]
    fn replace_keeps_triage_and_other_sources() {
        let mut conn = open_in_memory();
        replace_source(&mut conn, "a", &[item("1", Kind::Mail, "Mail", 10)]).unwrap();
        replace_source(&mut conn, "b", &[item("1", Kind::Ticket, "Ticket", 20)]).unwrap();
        let mail = list(&conn, &Query::default()).unwrap().pop().unwrap();
        set_triage(&conn, mail.id, Triage::Done).unwrap();

        replace_source(&mut conn, "a", &[item("1", Kind::Mail, "Mail", 10)]).unwrap();

        let done = Query {
            triage: Some(Triage::Done),
            ..Query::default()
        };
        assert_eq!(titles(&conn, done), ["Mail"]);
        assert_eq!(titles(&conn, Query::default()), ["Ticket", "Mail"]);
    }

    #[test]
    fn retain_drops_unconfigured_sources_but_never_local() {
        let mut conn = open_in_memory();
        replace_source(&mut conn, "gone", &[item("1", Kind::Mail, "Gone", 1)]).unwrap();
        replace_source(&mut conn, "kept", &[item("1", Kind::Mail, "Kept", 2)]).unwrap();
        replace_source(&mut conn, LOCAL_SOURCE, &[item("1", Kind::Task, "Mine", 3)]).unwrap();
        retain_sources(&conn, &["kept"]).unwrap();
        assert_eq!(titles(&conn, Query::default()), ["Mine", "Kept"]);
    }

    #[test]
    fn filters_by_kind() {
        let mut conn = open_in_memory();
        replace_source(
            &mut conn,
            "a",
            &[
                item("1", Kind::Mail, "Mail", 1),
                item("2", Kind::Ticket, "Ticket", 2),
                item("3", Kind::MergeRequest, "Merge request", 3),
            ],
        )
        .unwrap();
        let query = Query {
            kinds: vec![Kind::Ticket, Kind::MergeRequest],
            ..Query::default()
        };
        assert_eq!(titles(&conn, query), ["Merge request", "Ticket"]);
    }

    #[test]
    fn search_ignores_accents_and_completes_the_last_word() {
        let mut conn = open_in_memory();
        replace_source(
            &mut conn,
            "a",
            &[
                item("1", Kind::Mail, "Réunion d'équipe jeudi", 1),
                item("2", Kind::Mail, "Facture de novembre", 2),
            ],
        )
        .unwrap();
        let search = |text: &str| {
            titles(
                &conn,
                Query {
                    search: text.into(),
                    ..Query::default()
                },
            )
        };
        assert_eq!(search("reunion equ"), ["Réunion d'équipe jeudi"]);
        assert_eq!(search("fact"), ["Facture de novembre"]);
        assert!(search("absent").is_empty());
        // Characters that are operators in the search syntax are plain text here.
        assert!(search("\"novembre\" OR (").is_empty());
    }

    #[test]
    fn search_follows_edits_and_removals() {
        let mut conn = open_in_memory();
        replace_source(&mut conn, "a", &[item("1", Kind::Mail, "Alpha", 1)]).unwrap();
        replace_source(&mut conn, "a", &[item("1", Kind::Mail, "Beta", 1)]).unwrap();
        let search = |text: &str| {
            titles(
                &conn,
                Query {
                    search: text.into(),
                    ..Query::default()
                },
            )
        };
        assert!(search("alpha").is_empty());
        assert_eq!(search("beta"), ["Beta"]);
    }
}
