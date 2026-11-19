//! Keeps the local items in step with the configured connectors.

use crate::{State, connectors, error::Result, items};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Sent to the interface whenever stored items may have changed.
pub const ITEMS_CHANGED: &str = "items-changed";

const PERIOD: Duration = Duration::from_secs(5 * 60);

/// A source that could not be synced, and why. The others still are.
#[derive(Debug, Serialize)]
pub struct Failure {
    pub source: String,
    pub message: String,
}

/// Syncs every configured connector once, and drops the items of sources that are gone.
pub async fn run(app: &AppHandle) -> Result<Vec<Failure>> {
    let state = app.state::<State>();
    let connectors = connectors::configured(&state.db())?;
    let sources: Vec<&str> = connectors.iter().map(|c| c.source()).collect();
    items::retain_sources(&state.db(), &sources)?;

    let mut failures = Vec::new();
    for connector in &connectors {
        // The database is never locked while a connector waits on the network.
        match connector.sync().await {
            Ok(fetched) => items::replace_source(&mut state.db(), connector.source(), &fetched)?,
            Err(message) => failures.push(Failure {
                source: connector.source().to_owned(),
                message,
            }),
        }
    }
    app.emit(ITEMS_CHANGED, ())?;
    Ok(failures)
}

/// Syncs now, then every few minutes for as long as the app runs.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut ticks = tokio::time::interval(PERIOD);
        loop {
            ticks.tick().await;
            match run(&app).await {
                Ok(failures) => {
                    for failure in failures {
                        eprintln!("sync of {} failed: {}", failure.source, failure.message);
                    }
                }
                Err(error) => eprintln!("sync failed: {error}"),
            }
        }
    });
}
