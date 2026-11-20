mod assistant;
mod connectors;
mod db;
mod error;
mod http;
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
    /// Why each source failed at the last sync. A source that is not listed is fine.
    sync_errors: Mutex<HashMap<String, String>>,
}

impl State {
    fn db(&self) -> MutexGuard<'_, Connection> {
        // A poisoned lock means a command panicked mid-query; the connection itself is still valid.
        self.db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn sync_errors(&self) -> MutexGuard<'_, HashMap<String, String>> {
        self.sync_errors
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

#[tauri::command]
fn create_task(app: AppHandle, title: &str) -> Result<()> {
    let title = title.trim();
    if title.is_empty() {
        return Err(Error::Message("A task needs a title.".into()));
    }
    items::create_local(&app.state::<State>().db(), items::Kind::Task, title)?;
    app.emit(sync::ITEMS_CHANGED, ())?;
    Ok(())
}

#[tauri::command]
fn set_task_done(app: AppHandle, id: i64, done: bool) -> Result<()> {
    let state = if done { "done" } else { "todo" };
    items::set_local_state(&app.state::<State>().db(), id, state)?;
    app.emit(sync::ITEMS_CHANGED, ())?;
    Ok(())
}

#[tauri::command]
fn delete_task(app: AppHandle, id: i64) -> Result<()> {
    items::delete_local(&app.state::<State>().db(), id)?;
    app.emit(sync::ITEMS_CHANGED, ())?;
    Ok(())
}

/// An item and the connector of the source it comes from.
fn item_with_connector(
    state: &State,
    id: i64,
) -> Result<(items::Item, Box<dyn connectors::Connector>)> {
    let gone = || Error::Message("This item no longer exists.".into());
    let item = items::get(&state.db(), id)?.ok_or_else(gone)?;
    let connector = connectors::configured(&state.db())?
        .into_iter()
        .find(|connector| connector.source() == item.source)
        .ok_or_else(gone)?;
    Ok((item, connector))
}

/// Sends the user's answer to a mail, through the mailbox it arrived in. Nothing calls this
/// but the user's own click on "Send".
#[tauri::command]
async fn send_reply(app: AppHandle, id: i64, text: String) -> Result<()> {
    if text.trim().is_empty() {
        return Err(Error::Message("The reply is empty.".into()));
    }
    let (mail, connector) = item_with_connector(&app.state::<State>(), id)?;
    connector.reply(&mail, &text).await?;
    Ok(())
}

/// Does something to an item where it lives (mark a mail as read, archive it…), then
/// records the result locally.
#[tauri::command]
async fn act_on_item(app: AppHandle, id: i64, action: connectors::Action) -> Result<()> {
    let state = app.state::<State>();
    let (item, connector) = item_with_connector(&state, id)?;
    connector.act(&item.external_id, action).await?;
    match action {
        connectors::Action::MarkRead => items::set_state(&state.db(), id, "read")?,
        connectors::Action::Archive => items::set_triage(&state.db(), id, items::Triage::Done)?,
    }
    app.emit(sync::ITEMS_CHANGED, ())?;
    Ok(())
}

// ---------- sources ----------

/// A source as the interface sees it: never its token, only whether it has one.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceView {
    #[serde(flatten)]
    source: connectors::Source,
    has_token: bool,
    /// Why the last sync of this source failed, if it did.
    error: Option<String>,
}

#[tauri::command]
fn list_sources(state: tauri::State<'_, State>) -> Result<Vec<SourceView>> {
    let errors = state.sync_errors().clone();
    connectors::sources(&state.db())?
        .into_iter()
        .map(|source| {
            Ok(SourceView {
                has_token: secrets::get(&connectors::secret_name(&source.id))?.is_some(),
                error: errors.get(&source.id).cloned(),
                source,
            })
        })
        .collect()
}

/// Saves a source. `token` replaces its token when given; leaving it out keeps the stored one.
#[tauri::command]
fn save_source(
    state: tauri::State<'_, State>,
    source: connectors::Source,
    token: Option<String>,
) -> Result<()> {
    if source.id.trim().is_empty() || source.name.trim().is_empty() {
        return Err(Error::Message("A source needs a name.".into()));
    }
    if let Some(token) = token.filter(|token| !token.trim().is_empty()) {
        secrets::set(&connectors::secret_name(&source.id), token.trim())?;
    }
    connectors::save_source(&state.db(), source)
}

/// Forgets a source and its token. Its items go at the next sync.
#[tauri::command]
fn delete_source(state: tauri::State<'_, State>, id: &str) -> Result<()> {
    secrets::delete(&connectors::secret_name(id))?;
    connectors::delete_source(&state.db(), id)
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

/// Writes the morning briefing from what the app knows, with the user's strong model.
/// `done` is what the user already ticked off today. Cancellable like `ask`.
#[tauri::command]
async fn briefing(
    app: AppHandle,
    request_id: String,
    lang: assistant::Lang,
    done: Vec<String>,
) -> Result<llm::Reply> {
    let state = app.state::<State>();
    // The database is read and released before the model is waited on.
    let (provider, request) = {
        let db = state.db();
        let (provider, model) = llm::chosen(&db, llm::Tier::Strong)?;
        let request = assistant::briefing_request(&db, lang, &done, chrono::Local::now(), model)?;
        (provider, request)
    };
    let key = secrets::get(&provider.id)?;
    let workdir = app.path().app_cache_dir()?.join("cli");
    let call = llm::complete(&provider, key.as_deref(), &request, &workdir);
    Ok(cancellable(&state, request_id, ANSWER_LIMIT, call).await?)
}

/// One turn of the chat: sends the conversation and hands the answer over piece by piece on
/// `on_delta` while it is being written. Returns the whole answer. Cancellable like `ask`.
#[tauri::command]
async fn chat(
    app: AppHandle,
    request_id: String,
    lang: assistant::Lang,
    messages: Vec<llm::Message>,
    on_delta: tauri::ipc::Channel<String>,
) -> Result<llm::Reply> {
    let state = app.state::<State>();
    let (provider, request) = {
        let db = state.db();
        let (provider, model) = llm::chosen(&db, llm::Tier::Strong)?;
        let request = assistant::chat_request(&db, lang, messages, chrono::Local::now(), model)?;
        (provider, request)
    };
    let key = secrets::get(&provider.id)?;
    let workdir = app.path().app_cache_dir()?.join("cli");
    // A piece that cannot be delivered means the window is gone; the answer is still returned.
    let deliver = |piece: &str| {
        let _ = on_delta.send(piece.to_owned());
    };
    let call = llm::stream(&provider, key.as_deref(), &request, &workdir, &deliver);
    Ok(cancellable(&state, request_id, ANSWER_LIMIT, call).await?)
}

/// Drafts the user's answer to a mail with the strong model, handing it over piece by piece
/// on `on_delta`. `intent` is what the user wants to say; it may be empty. The draft is only
/// text for the user to edit: nothing is sent. Cancellable like `ask`.
#[tauri::command]
async fn draft_reply(
    app: AppHandle,
    request_id: String,
    id: i64,
    intent: String,
    on_delta: tauri::ipc::Channel<String>,
) -> Result<llm::Reply> {
    let state = app.state::<State>();
    let (mail, connector) = item_with_connector(&state, id)?;
    if mail.kind != items::Kind::Mail {
        return Err(Error::Message("Only a mail can be answered.".into()));
    }
    let (provider, model) = llm::chosen(&state.db(), llm::Tier::Strong)?;
    let key = secrets::get(&provider.id)?;
    let workdir = app.path().app_cache_dir()?.join("cli");
    let deliver = |piece: &str| {
        let _ = on_delta.send(piece.to_owned());
    };
    let call = async {
        // Only the start of a mail is stored: its whole text is read from where it lives.
        let text = connector.full_text(&mail.external_id).await?;
        let text = text.as_deref().unwrap_or(&mail.body);
        let request = assistant::reply_request(&mail, text, &intent, model);
        llm::stream(&provider, key.as_deref(), &request, &workdir, &deliver).await
    };
    Ok(cancellable(&state, request_id, ANSWER_LIMIT, call).await?)
}

/// Sorts the mail still waiting without a label, with the user's fast model, and returns how
/// many mails got one. Cancellable like `ask`.
#[tauri::command]
async fn sort_mail(app: AppHandle, request_id: String) -> Result<usize> {
    let state = app.state::<State>();
    let (provider, asked, request) = {
        let db = state.db();
        let (provider, model) = llm::chosen(&db, llm::Tier::Fast)?;
        match assistant::sort_request(&db, model)? {
            Some((asked, request)) => (provider, asked, request),
            None => return Ok(0),
        }
    };
    let key = secrets::get(&provider.id)?;
    let workdir = app.path().app_cache_dir()?.join("cli");
    let attempts = async {
        // Asked a second time when the first answer cannot be read: small models slip now and then.
        for _ in 0..2 {
            let reply = llm::complete(&provider, key.as_deref(), &request, &workdir).await?;
            let labels = assistant::read_labels(&reply.text, &asked);
            if !labels.is_empty() {
                return Ok(labels);
            }
        }
        Err(
            "The model's answer could not be read. Try again, or choose another fast model."
                .to_owned(),
        )
    };
    let labels = cancellable(&state, request_id, ANSWER_LIMIT, attempts).await?;
    for (id, label) in &labels {
        items::set_label(&state.db(), *id, *label)?;
    }
    app.emit(sync::ITEMS_CHANGED, ())?;
    Ok(labels.len())
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
        .plugin(tauri_plugin_opener::init())
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
                sync_errors: Mutex::default(),
            });
            sync::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_items,
            set_triage,
            sync_now,
            create_task,
            set_task_done,
            delete_task,
            act_on_item,
            list_sources,
            save_source,
            delete_source,
            get_setting,
            set_setting,
            list_providers,
            save_provider,
            delete_provider,
            list_models,
            ask,
            briefing,
            chat,
            sort_mail,
            draft_reply,
            send_reply,
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
            sync_errors: Mutex::default(),
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
