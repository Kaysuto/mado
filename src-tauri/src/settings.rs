//! User settings: small JSON values by key, in the local database. Never secrets.

use crate::error::Result;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

/// Whether the built-in sample data is shown.
pub const DEMO: &str = "demo";

pub fn get(conn: &Connection, key: &str) -> Result<Option<Value>> {
    let text: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| {
            row.get(0)
        })
        .optional()?;
    Ok(text.map(|text| serde_json::from_str(&text)).transpose()?)
}

pub fn set(conn: &Connection, key: &str, value: &Value) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET value = excluded.value",
        params![key, value.to_string()],
    )?;
    Ok(())
}

pub fn flag(conn: &Connection, key: &str) -> Result<bool> {
    Ok(get(conn, key)?.and_then(|value| value.as_bool()) == Some(true))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use serde_json::json;

    #[test]
    fn round_trip_and_overwrite() {
        let conn = open_in_memory();
        assert_eq!(get(&conn, "locale").unwrap(), None);
        set(&conn, "locale", &json!("fr")).unwrap();
        set(&conn, "locale", &json!("en")).unwrap();
        assert_eq!(get(&conn, "locale").unwrap(), Some(json!("en")));
    }

    #[test]
    fn flag_is_false_unless_true() {
        let conn = open_in_memory();
        assert!(!flag(&conn, DEMO).unwrap());
        set(&conn, DEMO, &json!("yes")).unwrap();
        assert!(!flag(&conn, DEMO).unwrap());
        set(&conn, DEMO, &json!(true)).unwrap();
        assert!(flag(&conn, DEMO).unwrap());
    }
}
