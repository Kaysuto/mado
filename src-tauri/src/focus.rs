//! Focus sessions: the time the user spent concentrating, on an item or on nothing in particular.
//! Only finished sessions are stored; the one in progress belongs to the interface.

use crate::error::{Error, Result};
use rusqlite::{Connection, params};
use serde::Deserialize;

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
    fn a_session_without_a_possible_duration_is_refused() {
        let conn = open_in_memory();
        assert!(record(&conn, &session(10_000, 0)).is_err());
        // Longer than the time between its start and its end.
        assert!(record(&conn, &session(10_000, 3_601)).is_err());
        assert_eq!(total_since(&conn, 0).unwrap(), 0);
    }
}
