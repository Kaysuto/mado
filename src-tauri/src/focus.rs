//! Focus sessions: the time the user spent concentrating, on an item or on nothing in particular.
//! Only finished sessions are stored; the one in progress belongs to the interface.

use crate::error::{Error, Result};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

/// A finished session. Times are Unix seconds; `seconds` leaves the pauses out.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    /// The item worked on, if any. Its title is kept with it: a sync may remove the item later.
    pub item_id: Option<i64>,
    pub title: String,
    pub started_at: i64,
    pub ended_at: i64,
    pub seconds: i64,
}

pub fn record(conn: &Connection, session: &Session) -> Result<()> {
    if session.seconds <= 0 || session.seconds > session.ended_at - session.started_at {
        return Err(Error::Message(
            "This focus session has no valid duration.".into(),
        ));
    }
    conn.execute(
        "INSERT INTO focus (item_id, title, started_at, ended_at, seconds)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            session.item_id,
            session.title.trim(),
            session.started_at,
            session.ended_at,
            session.seconds
        ],
    )?;
    Ok(())
}

/// Seconds of focus in the sessions that ended at `since` or later.
pub fn total_since(conn: &Connection, since: i64) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(SUM(seconds), 0) FROM focus WHERE ended_at >= ?1",
        [since],
        |row| row.get(0),
    )?)
}

/// The focus time one item got over a period.
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Time {
    pub item_id: Option<i64>,
    pub title: String,
    /// The project or repository of the item, when it still exists and has one.
    pub project: Option<String>,
    pub seconds: i64,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Day {
    /// Unix seconds: when the day starts.
    pub start: i64,
    pub seconds: i64,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Report {
    /// Most time first.
    pub items: Vec<Time>,
    /// One entry per day asked for, in the same order.
    pub days: Vec<Day>,
}

/// Focus time over the days that start at `starts` (Unix seconds, oldest first); the last one
/// runs until now. A session counts on the day it ended.
pub fn report(conn: &Connection, starts: &[i64]) -> Result<Report> {
    // An item's id may be given to another item once it is gone: the title has to match too.
    let mut by_item = conn.prepare(
        "SELECT focus.item_id, focus.title,
                COALESCE(json_extract(items.meta, '$.project'),
                         json_extract(items.meta, '$.repository')),
                SUM(focus.seconds) AS seconds
         FROM focus
         LEFT JOIN items ON items.id = focus.item_id AND items.title = focus.title
         WHERE focus.ended_at >= ?1
         GROUP BY focus.item_id, focus.title
         ORDER BY seconds DESC, focus.title",
    )?;
    let items = by_item
        .query_map([starts.first().copied().unwrap_or(i64::MAX)], |row| {
            Ok(Time {
                item_id: row.get(0)?,
                title: row.get(1)?,
                project: row.get(2)?,
                seconds: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let mut by_day = conn.prepare(
        "SELECT COALESCE(SUM(seconds), 0) FROM focus WHERE ended_at >= ?1 AND ended_at < ?2",
    )?;
    let days = starts
        .iter()
        .enumerate()
        .map(|(index, &start)| {
            let end = starts.get(index + 1).copied().unwrap_or(i64::MAX);
            Ok(Day {
                start,
                seconds: by_day.query_row([start, end], |row| row.get(0))?,
            })
        })
        .collect::<Result<_>>()?;
    Ok(Report { items, days })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn session(ended_at: i64, seconds: i64) -> Session {
        Session {
            item_id: Some(7),
            title: " Fix the login ".into(),
            started_at: ended_at - 3_600,
            ended_at,
            seconds,
        }
    }

    #[test]
    fn finished_sessions_add_up_from_a_given_time() {
        let conn = open_in_memory();
        record(&conn, &session(10_000, 1_500)).unwrap();
        record(&conn, &session(20_000, 600)).unwrap();
        assert_eq!(total_since(&conn, 0).unwrap(), 2_100);
        assert_eq!(total_since(&conn, 20_000).unwrap(), 600);
        assert_eq!(total_since(&conn, 20_001).unwrap(), 0);
        let title: String = conn
            .query_row("SELECT title FROM focus LIMIT 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(title, "Fix the login");
    }

    #[test]
    fn the_report_adds_up_by_item_and_by_day() {
        let conn = open_in_memory();
        conn.execute(
            "INSERT INTO items (id, source, external_id, kind, title, occurred_at, meta, synced_at)
             VALUES (7, 'tracker', 'a', 'ticket', 'Fix the login', 0, '{\"project\":\"Mobile\"}', 0),
                    (8, 'tracker', 'b', 'ticket', 'Someone else''s id', 0, '{\"project\":\"Web\"}', 0)",
            [],
        )
        .unwrap();
        let free = |ended_at, seconds| Session {
            item_id: None,
            title: String::new(),
            ..session(ended_at, seconds)
        };
        record(&conn, &session(10_000, 600)).unwrap();
        record(&conn, &session(30_000, 900)).unwrap();
        record(&conn, &free(30_500, 300)).unwrap();
        // The item this one was about is gone, and its id now belongs to another item.
        record(
            &conn,
            &Session {
                item_id: Some(8),
                title: "Old ticket".into(),
                ..session(31_000, 2_000)
            },
        )
        .unwrap();
        // Before the period.
        record(&conn, &session(5_000, 50)).unwrap();

        let report = report(&conn, &[10_000, 20_000, 30_000]).unwrap();
        let rows: Vec<_> = report
            .items
            .iter()
            .map(|time| (time.title.as_str(), time.project.as_deref(), time.seconds))
            .collect();
        assert_eq!(
            rows,
            [
                ("Old ticket", None, 2_000),
                ("Fix the login", Some("Mobile"), 1_500),
                ("", None, 300),
            ]
        );
        let days: Vec<_> = report.days.iter().map(|day| day.seconds).collect();
        assert_eq!(days, [600, 0, 3_200]);
        assert_eq!(report.days[2].start, 30_000);
    }

    #[test]
    fn a_session_without_a_possible_duration_is_refused() {
        let conn = open_in_memory();
        assert!(record(&conn, &session(10_000, 0)).is_err());
        // Longer than the time between its start and its end.
        assert!(record(&conn, &session(10_000, 3_601)).is_err());
        assert_eq!(total_since(&conn, 0).unwrap(), 0);
    }
}
