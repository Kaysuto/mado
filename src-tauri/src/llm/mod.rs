//! Language models. The user picks the provider: a subscription driven through its official
//! command-line tool, an API key, a free tier or a model running on this machine. The rest of
//! the app only calls `complete` and never knows which one answered.

mod anthropic;
mod cli;
mod openai_compat;

use crate::{error::Result, http::client, settings};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Anthropic's Messages API, with an API key.
    Anthropic,
    /// Any service speaking the OpenAI Chat Completions protocol: OpenAI, OpenRouter, Groq,
    /// Mistral, Gemini, Ollama, LM Studio… The key is optional (local servers need none).
    OpenaiCompat,
    /// The user's Claude subscription, through the Claude Code tool installed on this machine.
    ClaudeCode,
    /// The user's ChatGPT subscription, through the Codex tool installed on this machine.
    Codex,
}

/// A provider the user has set up. Its API key, if any, is in the keychain under `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub kind: ProviderKind,
    pub name: String,
    /// Required for `OpenaiCompat`; overrides the official address for `Anthropic`.
    #[serde(default)]
    pub base_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// Empty means the provider's own default, where it has one.
    pub model: String,
    #[serde(default)]
    pub system: Option<String>,
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reply {
    pub text: String,
    /// Token counts, when the provider reports them.
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// Failures here are already worded for the user.
type Outcome<T> = std::result::Result<T, String>;

/// Sends the conversation and waits for the whole answer. The caller sets the time limit.
/// `workdir` is an empty folder for the command-line tools to run in.
pub async fn complete(
    provider: &Provider,
    key: Option<&str>,
    request: &Request,
    workdir: &Path,
) -> Outcome<Reply> {
    match provider.kind {
        ProviderKind::Anthropic => {
            let key = key.ok_or("This provider has no API key yet.")?;
            anthropic::complete(client(), &provider.base_url, key, request).await
        }
        ProviderKind::OpenaiCompat => {
            openai_compat::complete(client(), &provider.base_url, key, request).await
        }
        ProviderKind::ClaudeCode | ProviderKind::Codex => {
            cli::complete(provider.kind, request, workdir).await
        }
    }
}

/// Called with each piece of text as the model writes it.
pub type OnDelta<'a> = &'a (dyn Fn(&str) + Send + Sync);

/// Like `complete`, handing the answer over piece by piece while it is being written.
pub async fn stream(
    provider: &Provider,
    key: Option<&str>,
    request: &Request,
    workdir: &Path,
    on_delta: OnDelta<'_>,
) -> Outcome<Reply> {
    match provider.kind {
        ProviderKind::Anthropic => {
            let key = key.ok_or("This provider has no API key yet.")?;
            anthropic::stream(client(), &provider.base_url, key, request, on_delta).await
        }
        ProviderKind::OpenaiCompat => {
            openai_compat::stream(client(), &provider.base_url, key, request, on_delta).await
        }
        ProviderKind::ClaudeCode | ProviderKind::Codex => {
            cli::stream(provider.kind, request, workdir, on_delta).await
        }
    }
}

/// Reads an answer sent as a stream of events, calling `on_data` with the payload of each.
async fn events(response: &mut reqwest::Response, mut on_data: impl FnMut(&str)) -> Outcome<()> {
    let mut buffer: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(unreachable)? {
        buffer.extend_from_slice(&chunk);
        // Whole lines only: a character cut in two by the network waits for its other half.
        while let Some(end) = buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = buffer.drain(..=end).collect();
            if let Some(data) = String::from_utf8_lossy(&line)
                .trim_end()
                .strip_prefix("data:")
            {
                on_data(data.trim_start());
            }
        }
    }
    Ok(())
}

/// The models the provider offers, to choose from. The user can always type another name.
pub async fn models(provider: &Provider, key: Option<&str>) -> Outcome<Vec<String>> {
    let mut models = match provider.kind {
        ProviderKind::Anthropic => {
            let key = key.ok_or("This provider has no API key yet.")?;
            anthropic::models(client(), &provider.base_url, key).await?
        }
        ProviderKind::OpenaiCompat => {
            openai_compat::models(client(), &provider.base_url, key).await?
        }
        ProviderKind::ClaudeCode => vec!["haiku".into(), "sonnet".into(), "opus".into()],
        // Codex uses the model of the user's own configuration unless told otherwise.
        ProviderKind::Codex => return Ok(Vec::new()),
    };
    if provider.kind != ProviderKind::ClaudeCode {
        models.sort();
    }
    Ok(models)
}

/// Whether the command-line tool of a subscription provider is installed. `None` for the others.
pub fn installed(kind: ProviderKind) -> Option<bool> {
    cli::program(kind).map(|program| which::which(program).is_ok())
}

/// Reads the error out of a failed HTTP answer. Both protocols put it in `error.message`.
async fn failure(response: reqwest::Response) -> String {
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    let message = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|json| json["error"]["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| body.chars().take(300).collect());
    format!("{status}: {message}")
}

fn unreachable(error: reqwest::Error) -> String {
    format!("Could not reach the provider: {error}")
}

// ---------- the two models the user picked ----------

/// Which of the user's two models a job needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Sorting, classifying, estimating. Its first user is the sorting of mail.
    Fast,
    /// Writing: briefings, reviews, chat, drafts.
    Strong,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Choice {
    provider: String,
    model: String,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Choices {
    fast: Choice,
    strong: Choice,
}

/// The provider and model to use for a job. When only one of the two models is set,
/// it does both jobs.
pub fn chosen(conn: &Connection, tier: Tier) -> Result<(Provider, String)> {
    let choices: Choices = match settings::get(conn, "models")? {
        Some(value) => serde_json::from_value(value).unwrap_or_default(),
        None => Choices::default(),
    };
    let (first, second) = match tier {
        Tier::Fast => (choices.fast, choices.strong),
        Tier::Strong => (choices.strong, choices.fast),
    };
    let all = providers(conn)?;
    [first, second]
        .into_iter()
        .find_map(|choice| {
            let provider = all.iter().find(|provider| provider.id == choice.provider)?;
            Some((provider.clone(), choice.model))
        })
        .ok_or_else(|| crate::error::Error::Message("Choose a model in the settings first.".into()))
}

// ---------- the providers the user has set up ----------

const PROVIDERS: &str = "providers";

pub fn providers(conn: &Connection) -> Result<Vec<Provider>> {
    Ok(match settings::get(conn, PROVIDERS)? {
        Some(value) => serde_json::from_value(value)?,
        None => Vec::new(),
    })
}

/// Adds the provider, or replaces the one with the same id.
pub fn save_provider(conn: &Connection, provider: Provider) -> Result<()> {
    let mut all = providers(conn)?;
    match all.iter_mut().find(|p| p.id == provider.id) {
        Some(existing) => *existing = provider,
        None => all.push(provider),
    }
    settings::set(conn, PROVIDERS, &serde_json::to_value(all)?)
}

pub fn delete_provider(conn: &Connection, id: &str) -> Result<()> {
    let mut all = providers(conn)?;
    all.retain(|p| p.id != id);
    settings::set(conn, PROVIDERS, &serde_json::to_value(all)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    fn provider(id: &str, name: &str) -> Provider {
        Provider {
            id: id.into(),
            kind: ProviderKind::OpenaiCompat,
            name: name.into(),
            base_url: "http://localhost:11434/v1".into(),
        }
    }

    #[test]
    fn providers_are_added_replaced_and_deleted() {
        let conn = open_in_memory();
        assert!(providers(&conn).unwrap().is_empty());
        save_provider(&conn, provider("a", "First")).unwrap();
        save_provider(&conn, provider("b", "Second")).unwrap();
        save_provider(&conn, provider("a", "First, renamed")).unwrap();
        let names: Vec<_> = providers(&conn)
            .unwrap()
            .into_iter()
            .map(|p| p.name)
            .collect();
        assert_eq!(names, ["First, renamed", "Second"]);

        delete_provider(&conn, "a").unwrap();
        assert_eq!(providers(&conn).unwrap(), [provider("b", "Second")]);
    }

    #[test]
    fn a_job_uses_its_model_or_the_other_one_when_only_one_is_set() {
        let conn = open_in_memory();
        assert!(chosen(&conn, Tier::Strong).is_err());

        save_provider(&conn, provider("a", "First")).unwrap();
        save_provider(&conn, provider("b", "Second")).unwrap();
        let set = |value| settings::set(&conn, "models", &value).unwrap();
        set(serde_json::json!({ "fast": { "provider": "a", "model": "small" } }));
        let (used, model) = chosen(&conn, Tier::Strong).unwrap();
        assert_eq!((used.id.as_str(), model.as_str()), ("a", "small"));

        set(serde_json::json!({
            "fast": { "provider": "a", "model": "small" },
            "strong": { "provider": "b", "model": "big" },
        }));
        let (used, model) = chosen(&conn, Tier::Strong).unwrap();
        assert_eq!((used.id.as_str(), model.as_str()), ("b", "big"));
        assert_eq!(chosen(&conn, Tier::Fast).unwrap().1, "small");

        // A model set on a provider that was removed since does not count.
        delete_provider(&conn, "b").unwrap();
        assert_eq!(chosen(&conn, Tier::Strong).unwrap().1, "small");
    }

    #[test]
    fn only_subscription_providers_depend_on_an_installed_tool() {
        assert_eq!(installed(ProviderKind::Anthropic), None);
        assert_eq!(installed(ProviderKind::OpenaiCompat), None);
        assert!(installed(ProviderKind::ClaudeCode).is_some());
    }
}
