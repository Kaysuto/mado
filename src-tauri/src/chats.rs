//! Conversations with the assistant, kept so they can be found and taken up again.

use crate::{
    assistant::line,
    error::{Error, Result},
    items::now,
    llm::{Message, Role},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

const TITLE_CHARS: usize = 80;
/// How many conversations the history lists.
const LISTED: u32 = 200;

/// A conversation as the history lists it.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: i64,
    /// The start of the user's first message.
    pub title: String,
    /// Unix seconds: when the last message was added.
    pub updated_at: i64,
}

/// The conversations, the one touched last first.
pub fn list(conn: &Connection) -> Result<Vec<Summary>> {
    let mut statement = conn.prepare(
        "SELECT id, title, updated_at FROM conversations
         ORDER BY updated_at DESC, id DESC LIMIT ?1",
    )?;
    let rows = statement.query_map([LISTED], |row| {
        Ok(Summary {
            id: row.get(0)?,
            title: row.get(1)?,
            updated_at: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// The messages of a conversation. `None` when it no longer exists.
pub fn messages(conn: &Connection, id: i64) -> Result<Option<Vec<Message>>> {
    let text: Option<String> = conn
        .query_row(
            "SELECT messages FROM conversations WHERE id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(text.map(|text| serde_json::from_str(&text)).transpose()?)
}

/// Stores a conversation as it stands and returns its id. Without an id, or with the id of a
/// conversation deleted meanwhile, it is stored as a new one.
pub fn save(conn: &Connection, id: Option<i64>, messages: &[Message]) -> Result<i64> {
    let first = messages
        .iter()
        .find(|message| message.role == Role::User)
        .ok_or_else(|| Error::Message("An empty conversation is not kept.".into()))?;
    let title = line(&first.content, TITLE_CHARS);
    let text = serde_json::to_string(messages)?;
    if let Some(id) = id {
        let updated = conn.execute(
            "UPDATE conversations SET title = ?2, messages = ?3, updated_at = ?4 WHERE id = ?1",
            params![id, title, text, now()],
        )?;
        if updated == 1 {
            return Ok(id);
        }
    }
    conn.execute(
        "INSERT INTO conversations (title, messages, updated_at) VALUES (?1, ?2, ?3)",
        params![title, text, now()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM conversations WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn message(role: Role, content: &str) -> Message {
        Message {
            role,
            content: content.into(),
        }
    }

    #[test]
    fn a_conversation_is_kept_extended_listed_and_deleted() {
        let conn = open_in_memory();
        let mut talk = vec![
            message(Role::User, "What should I\nstart with today?"),
            message(Role::Assistant, "The invoice."),
        ];
        let id = save(&conn, None, &talk).unwrap();
        talk.push(message(Role::User, "And then?"));
        assert_eq!(save(&conn, Some(id), &talk).unwrap(), id);
        let other = save(&conn, None, &[message(Role::User, "When am I free?")]).unwrap();

        let listed = list(&conn).unwrap();
        assert_eq!(listed.len(), 2);
        let titles: Vec<_> = listed
            .iter()
            .map(|summary| summary.title.as_str())
            .collect();
        assert!(titles.contains(&"What should I start with today?"));
        assert_eq!(messages(&conn, id).unwrap(), Some(talk.clone()));

        delete(&conn, id).unwrap();
        assert_eq!(messages(&conn, id).unwrap(), None);
        // Saved again after it was deleted elsewhere: it comes back as a new one.
        let again = save(&conn, Some(id), &talk).unwrap();
        assert!(again != other && messages(&conn, again).unwrap().is_some());
        assert_eq!(list(&conn).unwrap().len(), 2);
    }

    #[test]
    fn a_conversation_without_a_question_is_not_kept() {
        let conn = open_in_memory();
        assert!(save(&conn, None, &[]).is_err());
        assert!(list(&conn).unwrap().is_empty());
    }
}
