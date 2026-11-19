mod connectors;
mod db;
mod error;
mod items;
mod llm;
mod secrets;
mod settings;
mod sync;

use error::{Error, Result};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    collections::HashMap,
    future::Future,
    sync::{Mutex, MutexGuard},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::oneshot;

/// How long a model may take to answer before the call is given up.
const ANSWER_LIMIT: Duration = Duration::from_secs(180);
const MODEL_LIST_LIMIT: Duration = Duration::from_secs(20);

/// What every command can reach.
pub struct State {
    db: Mutex<Connection>,
    /// Model calls in progress, by the id the interface gave them, so they can be cancelled.
    running: Mutex<HashMap<String, oneshot::Sender<()>>>,
}

impl State {
    fn db(&self) -> MutexGuard<'_, Connection> {
        // A poisoned lock means a command panicked mid-query; the connection itself is still valid.
        self.db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn running(&self) -> MutexGuard<'_, HashMap<String, oneshot::Sender<()>>> {
        self.running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

// ---------- items ----------

#[tauri::command]
fn list_items(state: tauri::State<'_, State>, query: items::Query) -> Result<Vec<items::Item>> {
    items::list(&state.db(), &query)
}

#[tauri::command]
fn set_triage(app: AppHandle, id: i64, triage: items::Triage) -> Result<()> {
    items::set_triage(&app.state::<State>().db(), id, triage)?;
    app.emit(sync::ITEMS_CHANGED, ())?;
    Ok(())
}

#[tauri::command]
async fn sync_now(app: AppHandle) -> Result<Vec<sync::Failure>> {
    sync::run(&app).await
}

// ---------- settings ----------

#[tauri::command]
fn get_setting(state: tauri::State<'_, State>, key: &str) -> Result<Option<serde_json::Value>> {
    settings::get(&state.db(), key)
}

#[tauri::command]
fn set_setting(state: tauri::State<'_, State>, key: &str, value: serde_json::Value) -> Result<()> {
    settings::set(&state.db(), key, &value)
}

// ---------- models ----------

/// A provider as the interface sees it: never its key, only whether it has one.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProviderView {
    #[serde(flatten)]
    provider: llm::Provider,
    has_key: bool,
    /// For subscription providers: whether their command-line tool is installed.
    installed: Option<bool>,
}

#[tauri::command]
fn list_providers(state: tauri::State<'_, State>) -> Result<Vec<ProviderView>> {
    llm::providers(&state.db())?
        .into_iter()
        .map(|provider| {
            Ok(ProviderView {
                has_key: secrets::get(&provider.id)?.is_some(),
                installed: llm::installed(provider.kind),
                provider,
            })
        })
        .collect()
}

/// Saves a provider. `key` replaces its API key when given; leaving it out keeps the stored one.
#[tauri::command]
fn save_provider(
    state: tauri::State<'_, State>,
    provider: llm::Provider,
    key: Option<String>,
) -> Result<()> {
    if provider.id.trim().is_empty() || provider.name.trim().is_empty() {
        return Err(Error::Message("A provider needs a name.".into()));
    }
    if let Some(key) = key.filter(|key| !key.trim().is_empty()) {
        secrets::set(&provider.id, key.trim())?;
    }
    llm::save_provider(&state.db(), provider)
}

#[tauri::command]
fn delete_provider(state: tauri::State<'_, State>, id: &str) -> Result<()> {
    secrets::delete(id)?;
    llm::delete_provider(&state.db(), id)
}

fn provider_with_key(app: &AppHandle, id: &str) -> Result<(llm::Provider, Option<String>)> {
    let provider = llm::providers(&app.state::<State>().db())?
        .into_iter()
        .find(|provider| provider.id == id)
        .ok_or_else(|| Error::Message("This provider no longer exists.".into()))?;
    let key = secrets::get(&provider.id)?;
    Ok((provider, key))
}

#[tauri::command]
async fn list_models(app: AppHandle, provider_id: String) -> Result<Vec<String>> {
    let (provider, key) = provider_with_key(&app, &provider_id)?;
    let models = tokio::time::timeout(MODEL_LIST_LIMIT, llm::models(&provider, key.as_deref()))
        .await
        .map_err(|_| Error::Message("The provider took too long to answer.".into()))??;
    Ok(models)
}

/// Asks a model and waits for its whole answer. `request_id` is chosen by the interface so it
/// can cancel the call with `cancel`.
#[tauri::command]
async fn ask(
    app: AppHandle,
    request_id: String,
    provider_id: String,
    request: llm::Request,
) -> Result<llm::Reply> {
    let (provider, key) = provider_with_key(&app, &provider_id)?;
    let workdir = app.path().app_cache_dir()?.join("cli");
    let call = llm::complete(&provider, key.as_deref(), &request, &workdir);
    Ok(cancellable(&app.state::<State>(), request_id, ANSWER_LIMIT, call).await?)
}

/// Stops a call started by `ask`. Nothing happens if it has already ended.
#[tauri::command]
fn cancel(state: tauri::State<'_, State>, request_id: &str) {
    stop(&state, request_id);
}

fn stop(state: &State, request_id: &str) {
    if let Some(stop) = state.running().remove(request_id) {
        let _ = stop.send(());
    }
}

/// Runs `call` until it answers, is cancelled or runs out of time. Dropping it in the last two
/// cases closes the connection, or stops the command-line tool it started.
async fn cancellable<T>(
    state: &State,
    request_id: String,
    limit: Duration,
    call: impl Future<Output = std::result::Result<T, String>>,
) -> std::result::Result<T, String> {
    let (stop, stopped) = oneshot::channel();
    state.running().insert(request_id.clone(), stop);
    let outcome = tokio::select! {
        answer = tokio::time::timeout(limit, call) => {
            answer.unwrap_or_else(|_| Err("The model took too long to answer.".into()))
        }
        _ = stopped => Err("Cancelled.".into()),
    };
    state.running().remove(&request_id);
    outcome
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
            app.manage(State {
                db: Mutex::new(db),
                running: Mutex::default(),
            });
            sync::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_items,
            set_triage,
            sync_now,
            get_setting,
            set_setting,
            list_providers,
            save_provider,
            delete_provider,
            list_models,
            ask,
            cancel,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Madow");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::{pending, ready};

    fn state() -> State {
        State {
            db: Mutex::new(db::open_in_memory()),
            running: Mutex::default(),
        }
    }

    const LONG: Duration = Duration::from_secs(60);

    #[tokio::test]
    async fn a_call_gives_its_answer_and_is_then_forgotten() {
        let state = state();
        let answer = cancellable(&state, "a".into(), LONG, ready(Ok::<_, String>(42))).await;
        assert_eq!(answer, Ok(42));
        assert!(state.running().is_empty());
    }

    #[tokio::test]
    async fn cancelling_ends_a_call_that_would_never_answer() {
        let state = state();
        let call = cancellable(&state, "a".into(), LONG, pending::<Outcome>());
        let (answer, ()) = tokio::join!(call, async { stop(&state, "a") });
        assert_eq!(answer, Err("Cancelled.".into()));
        assert!(state.running().is_empty());
    }

    #[tokio::test]
    async fn a_call_that_takes_too_long_is_given_up() {
        let state = state();
        let limit = Duration::from_millis(10);
        let answer = cancellable(&state, "a".into(), limit, pending::<Outcome>()).await;
        assert_eq!(answer, Err("The model took too long to answer.".into()));
    }

    #[tokio::test]
    async fn cancelling_an_unknown_call_does_nothing() {
        stop(&state(), "nobody");
    }

    type Outcome = std::result::Result<(), String>;
}
