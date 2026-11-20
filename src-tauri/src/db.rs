//! The local database: one SQLite file holding everything Madow knows.

use rusqlite::Connection;
use std::path::Path;

/// Schema steps, applied in order. Never edit a step that has shipped: append a new one.
const MIGRATIONS: &[&str] = &[
    // 1: items (the one shape every source is translated into), full-text search, settings.
    "
    CREATE TABLE items (
        id INTEGER PRIMARY KEY,
        source TEXT NOT NULL,
        external_id TEXT NOT NULL,
        kind TEXT NOT NULL,
        title TEXT NOT NULL,
        body TEXT NOT NULL DEFAULT '',
        author TEXT,
        url TEXT,
        state TEXT NOT NULL DEFAULT '',
        triage TEXT NOT NULL DEFAULT 'inbox',
        occurred_at INTEGER NOT NULL,
        ends_at INTEGER,
        meta TEXT NOT NULL DEFAULT '{}',
        synced_at INTEGER NOT NULL,
        UNIQUE (source, external_id)
    );
    CREATE INDEX items_by_time ON items (occurred_at DESC);

    CREATE VIRTUAL TABLE items_fts USING fts5(
        title, body, author,
        content = 'items', content_rowid = 'id',
        tokenize = 'unicode61 remove_diacritics 2'
    );
    CREATE TRIGGER items_fts_insert AFTER INSERT ON items BEGIN
        INSERT INTO items_fts (rowid, title, body, author)
        VALUES (new.id, new.title, new.body, new.author);
    END;
    CREATE TRIGGER items_fts_delete AFTER DELETE ON items BEGIN
        INSERT INTO items_fts (items_fts, rowid, title, body, author)
        VALUES ('delete', old.id, old.title, old.body, old.author);
    END;
    CREATE TRIGGER items_fts_update AFTER UPDATE ON items BEGIN
        INSERT INTO items_fts (items_fts, rowid, title, body, author)
        VALUES ('delete', old.id, old.title, old.body, old.author);
        INSERT INTO items_fts (rowid, title, body, author)
        VALUES (new.id, new.title, new.body, new.author);
    END;

    CREATE TABLE settings (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;
    ",
    // 2: the label a model gave an item when sorting it. Local, like the triage.
    "ALTER TABLE items ADD COLUMN label TEXT;",
    // 3: finished focus sessions. `item_id` is not a foreign key: a sync may remove the item,
    // the time spent on it stays.
    "
    CREATE TABLE focus (
        id INTEGER PRIMARY KEY,
        item_id INTEGER,
        title TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        ended_at INTEGER NOT NULL,
        seconds INTEGER NOT NULL
    );
    CREATE INDEX focus_by_end ON focus (ended_at);
    ",
];

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    migrate(&conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory database");
    migrate(&conn).expect("migrations");
    conn
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let done: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (index, step) in MIGRATIONS.iter().enumerate().skip(done as usize) {
        conn.execute_batch(&format!(
            "BEGIN; {step} PRAGMA user_version = {}; COMMIT;",
            index + 1
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_run_once() {
        let conn = open_in_memory();
        migrate(&conn).expect("second run is a no-op");
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
    }
}
