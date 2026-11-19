mod connectors;
mod db;
mod error;
mod items;
mod settings;
mod sync;

use error::Result;
use rusqlite::Connection;
use std::sync::{Mutex, MutexGuard};
use tauri::{AppHandle, Manager};

/// What every command can reach.
pub struct State {
    db: Mutex<Connection>,
}

impl State {
    fn db(&self) -> MutexGuard<'_, Connection> {
        // A poisoned lock means a command panicked mid-query; the connection itself is still valid.
        self.db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[tauri::command]
fn list_items(state: tauri::State<'_, State>, query: items::Query) -> Result<Vec<items::Item>> {
    items::list(&state.db(), &query)
}

#[tauri::command]
fn set_triage(app: AppHandle, id: i64, triage: items::Triage) -> Result<()> {
    items::set_triage(&app.state::<State>().db(), id, triage)?;
    tauri::Emitter::emit(&app, sync::ITEMS_CHANGED, ())?;
    Ok(())
}

#[tauri::command]
fn get_setting(state: tauri::State<'_, State>, key: &str) -> Result<Option<serde_json::Value>> {
    settings::get(&state.db(), key)
}

#[tauri::command]
fn set_setting(state: tauri::State<'_, State>, key: &str, value: serde_json::Value) -> Result<()> {
    settings::set(&state.db(), key, &value)
}

#[tauri::command]
async fn sync_now(app: AppHandle) -> Result<Vec<sync::Failure>> {
    sync::run(&app).await
}

pub fn run() {
    tauri::Builder::default()
        // A second launch brings the running window forward instead of opening another app.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let db = db::open(&dir.join("madow.db"))?;
            app.manage(State { db: Mutex::new(db) });
            sync::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_items,
            set_triage,
            get_setting,
            set_setting,
            sync_now,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Madow");
}
