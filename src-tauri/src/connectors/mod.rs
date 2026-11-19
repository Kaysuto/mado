//! Connectors: each one translates a service (a mailbox, a tracker, a forge…) into items.
//! The rest of the app only ever sees items, so it never depends on a particular service.

mod demo;

use crate::{error::Result, items::NewItem, settings};
use async_trait::async_trait;
use rusqlite::Connection;

#[async_trait]
pub trait Connector: Send + Sync {
    /// Name of this configured source, stored on every item it produces.
    /// Unique among the configured connectors, and stable across restarts.
    fn source(&self) -> &str;

    /// Fetches everything the source currently holds. What is no longer returned is removed
    /// locally; the user's triage of what remains is kept.
    async fn sync(&self) -> std::result::Result<Vec<NewItem>, String>;
}

/// The connectors the user has turned on.
pub fn configured(conn: &Connection) -> Result<Vec<Box<dyn Connector>>> {
    let mut connectors: Vec<Box<dyn Connector>> = Vec::new();
    if settings::flag(conn, settings::DEMO)? {
        connectors.push(Box::new(demo::Demo));
    }
    Ok(connectors)
}
