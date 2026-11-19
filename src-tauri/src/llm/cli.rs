//! Subscriptions, through the vendor's own command-line tool installed on this machine.
//! Madow only launches the unmodified tool: it never reads the login or the token behind it.
//! The tool runs in an empty folder with its tools switched off, so it can only write text.

use super::{Outcome, ProviderKind, Reply, Request, Role};
use serde_json::Value;
use std::{
    path::Path,
    process::Stdio,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{io::AsyncWriteExt, process::Command};

/// Name of the tool behind a subscription provider.
pub fn program(kind: ProviderKind) -> Option<&'static str> {
    match kind {
        ProviderKind::ClaudeCode => Some("claude"),
        ProviderKind::Codex => Some("codex"),
        ProviderKind::Anthropic | ProviderKind::OpenaiCompat => None,
    }
}

pub async fn complete(kind: ProviderKind, request: &Request, workdir: &Path) -> Outcome<Reply> {
    let name = program(kind).ok_or("This provider has no command-line tool.")?;
    // The full path, extension included: Windows does not find `.cmd` launchers by bare name.
    let path = which::which(name).map_err(|_| format!("`{name}` is not installed."))?;
    std::fs::create_dir_all(workdir).map_err(|error| error.to_string())?;

    let mut command = Command::new(path);
    command
        .current_dir(workdir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Cancelling or timing out drops the call, which must also stop the tool.
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console flashing on screen

    match kind {
        ProviderKind::ClaudeCode => claude(command, request).await,
        _ => codex(command, request, workdir).await,
    }
}

async fn claude(mut command: Command, request: &Request) -> Outcome<Reply> {
    command.args([
        "-p",
        "--output-format",
        "json",
        // No user hooks, plugins or MCP servers; no built-in tools; nothing saved to disk.
        "--setting-sources",
        "project,local",
        "--strict-mcp-config",
        "--tools",
        "",
        "--no-session-persistence",
    ]);
    if !request.model.is_empty() {
        command.args(["--model", &request.model]);
    }
    if let Some(system) = &request.system {
        command.args(["--system-prompt", system]);
    }
    let output = run(command, &transcript(request)).await?;
    read_claude(&output.stdout).ok_or_else(|| explain(&output))?
}

/// Reads the single JSON object `claude -p --output-format json` prints.
fn read_claude(stdout: &[u8]) -> Option<Outcome<Reply>> {
    let answer: Value = serde_json::from_slice(stdout).ok()?;
    let text = answer["result"].as_str()?.to_owned();
    Some(if answer["is_error"] == true {
        Err(text)
    } else {
        Ok(Reply {
            text,
            input_tokens: answer["usage"]["input_tokens"].as_u64(),
            output_tokens: answer["usage"]["output_tokens"].as_u64(),
        })
    })
}

async fn codex(mut command: Command, request: &Request, workdir: &Path) -> Outcome<Reply> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let answer = workdir.join(format!("answer-{stamp}.txt"));
    command
        .args(["exec", "--skip-git-repo-check", "--ephemeral"])
        .args(["--sandbox", "read-only"])
        .arg("--output-last-message")
        .arg(&answer);
    if !request.model.is_empty() {
        command.args(["--model", &request.model]);
    }
    command.arg("-"); // the prompt comes on standard input

    // Codex has no separate system prompt in this mode: the instructions go first.
    let prompt = match &request.system {
        Some(system) => format!("{system}\n\n{}", transcript(request)),
        None => transcript(request),
    };
    let output = run(command, &prompt).await?;
    let text = std::fs::read_to_string(&answer);
    let _ = std::fs::remove_file(&answer);
    match text {
        Ok(text) if output.status.success() => Ok(Reply {
            text: text.trim().to_owned(),
            input_tokens: None,
            output_tokens: None,
        }),
        _ => Err(explain(&output)),
    }
}

async fn run(mut command: Command, prompt: &str) -> Outcome<std::process::Output> {
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    // The prompt goes through standard input: no length limit, nothing to escape.
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .await
            .map_err(|error| error.to_string())?;
    }
    child
        .wait_with_output()
        .await
        .map_err(|error| error.to_string())
}

/// What the tool said when it failed, shortened.
fn explain(output: &std::process::Output) -> String {
    let said = if output.stderr.is_empty() {
        &output.stdout
    } else {
        &output.stderr
    };
    let said: String = String::from_utf8_lossy(said)
        .trim()
        .chars()
        .take(300)
        .collect();
    if said.is_empty() {
        format!("The tool stopped without an answer ({}).", output.status)
    } else {
        said
    }
}

/// The tools take one prompt, not a list of messages. A lone question is sent as is;
/// a conversation is written out with who said what.
fn transcript(request: &Request) -> String {
    if let [only] = request.messages.as_slice() {
        return only.content.clone();
    }
    request
        .messages
        .iter()
        .map(|message| {
            let who = match message.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
            };
            format!("{who}: {}", message.content)
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Message;

    fn request(messages: &[(Role, &str)]) -> Request {
        Request {
            model: String::new(),
            system: None,
            messages: messages
                .iter()
                .map(|(role, content)| Message {
                    role: *role,
                    content: (*content).into(),
                })
                .collect(),
        }
    }

    #[test]
    fn a_lone_question_is_sent_as_is() {
        assert_eq!(transcript(&request(&[(Role::User, "Why?")])), "Why?");
    }

    #[test]
    fn a_conversation_says_who_spoke() {
        let conversation = request(&[
            (Role::User, "Hi"),
            (Role::Assistant, "Hello"),
            (Role::User, "Why?"),
        ]);
        assert_eq!(
            transcript(&conversation),
            "User: Hi\n\nAssistant: Hello\n\nUser: Why?"
        );
    }

    #[test]
    fn reads_the_answer_and_the_token_counts_of_claude() {
        let stdout = br#"{"type":"result","is_error":false,"result":"ok","usage":{"input_tokens":3,"output_tokens":1}}"#;
        assert_eq!(
            read_claude(stdout),
            Some(Ok(Reply {
                text: "ok".into(),
                input_tokens: Some(3),
                output_tokens: Some(1),
            }))
        );
    }

    #[test]
    fn an_error_from_claude_is_reported_with_its_message() {
        let stdout = br#"{"type":"result","is_error":true,"result":"Not logged in"}"#;
        assert_eq!(read_claude(stdout), Some(Err("Not logged in".into())));
        assert_eq!(read_claude(b"not json"), None);
    }

    /// Calls the real tool with the user's subscription, so it only runs on demand:
    /// `cargo test -- --ignored real_claude`.
    #[tokio::test]
    #[ignore]
    async fn real_claude_code_answers() {
        let workdir = std::env::temp_dir().join("madow-cli-test");
        let mut ask = request(&[(Role::User, "Reply with the single word: ok")]);
        ask.model = "haiku".into();
        let reply = complete(ProviderKind::ClaudeCode, &ask, &workdir)
            .await
            .unwrap();
        assert!(reply.text.to_lowercase().contains("ok"), "{reply:?}");
    }
}
