//! Coding-agent sessions on this machine, read from the files the agents keep themselves:
//! one JSON line per event, under `~/.claude/projects` and `~/.codex/sessions`. Only the start
//! and the end of each file are read, so long sessions stay cheap. Nothing is written.

use crate::assistant::line;
use serde::Serialize;
use serde_json::Value;
use std::{
    fs::File,
    io::{BufRead, BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

/// Sessions untouched for longer than this are left out.
const RECENT: i64 = 12 * 3600;
const SHOWN: usize = 15;
/// How much of a session file is read at each end.
const HEAD: u64 = 256 * 1024;
const TAIL: u64 = 128 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    ClaudeCode,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Progress {
    /// The agent is in the middle of a turn.
    Working,
    /// The agent just stopped: it finished, or it waits for an answer.
    Idle,
    Done,
}

#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub engine: Engine,
    pub id: String,
    /// Name of the folder the agent works in.
    pub project: String,
    /// What the user asked first, or the name the session was given.
    pub title: String,
    /// The last thing the agent wrote.
    pub last: String,
    /// The tool the agent called last, when that came after its last words.
    pub tool: Option<String>,
    /// Unix seconds.
    pub updated_at: i64,
    pub state: Progress,
}

/// What a session file says, before its age is taken into account.
#[derive(Debug, Default, PartialEq)]
struct Record {
    id: Option<String>,
    cwd: String,
    title: String,
    last: String,
    tool: Option<String>,
    /// Whether the last event leaves the agent in the middle of a turn.
    working: bool,
}

/// The sessions touched in the last hours under `home`, the latest first.
pub fn list(home: &Path, now: i64) -> Vec<Session> {
    let mut files = Vec::new();
    let projects = home.join(".claude").join("projects");
    for project in entries(&projects).filter(|path| path.is_dir()) {
        files.extend(entries(&project).map(|file| (Engine::ClaudeCode, file)));
    }
    let mut folders = vec![home.join(".codex").join("sessions")];
    // Codex files its sessions by year, month and day.
    while let Some(folder) = folders.pop() {
        for path in entries(&folder) {
            if path.is_dir() {
                folders.push(path);
            } else {
                files.push((Engine::Codex, path));
            }
        }
    }

    let mut sessions: Vec<Session> = files
        .into_iter()
        .filter(|(_, file)| {
            file.extension()
                .is_some_and(|extension| extension == "jsonl")
        })
        .filter_map(|(engine, file)| {
            let updated_at = modified(&file).filter(|updated| now - updated < RECENT)?;
            let read = match engine {
                Engine::ClaudeCode => claude(&head(&file), &tail(&file)),
                Engine::Codex => codex(&head(&file), &tail(&file)),
            }?;
            let age = now - updated_at;
            Some(Session {
                engine,
                id: read.id.or_else(|| name(&file)).unwrap_or_default(),
                // Split by hand: the folder is written the way of the system the agent ran on.
                project: read
                    .cwd
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or_default()
                    .into(),
                // A session started by a command alone has no first words: show its last ones.
                title: line(
                    if read.title.is_empty() {
                        &read.last
                    } else {
                        &read.title
                    },
                    90,
                ),
                last: line(&read.last, 110),
                tool: read.tool,
                updated_at,
                // A file that stopped growing means the agent stopped, whatever its last line.
                state: match age {
                    ..180 if read.working => Progress::Working,
                    ..120 => Progress::Idle,
                    _ => Progress::Done,
                },
            })
        })
        .collect();
    sessions.sort_by_key(|session| std::cmp::Reverse(session.updated_at));
    sessions.truncate(SHOWN);
    sessions
}

fn entries(folder: &Path) -> impl Iterator<Item = PathBuf> {
    let entries = std::fs::read_dir(folder).into_iter().flatten().flatten();
    entries.map(|entry| entry.path())
}

fn name(path: &Path) -> Option<String> {
    let name = path.file_stem()?.to_string_lossy().into_owned();
    Some(name)
}

fn modified(file: &Path) -> Option<i64> {
    let modified = file.metadata().ok()?.modified().ok()?;
    Some(modified.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64)
}

/// The first events of a session file.
fn head(file: &Path) -> Vec<Value> {
    let Ok(file) = File::open(file) else {
        return Vec::new();
    };
    let lines = BufReader::new(file.take(HEAD))
        .lines()
        .map_while(Result::ok);
    events(lines.take(60))
}

/// The last events of a session file.
fn tail(file: &Path) -> Vec<Value> {
    let mut text = Vec::new();
    let read = File::open(file).and_then(|mut file| {
        let start = file.metadata()?.len().saturating_sub(TAIL);
        file.seek(SeekFrom::Start(start))?;
        file.read_to_end(&mut text)?;
        Ok(start)
    });
    let Ok(start) = read else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&text);
    // Starting in the middle of the file means starting in the middle of a line: drop it.
    events(text.lines().skip(usize::from(start > 0)))
}

/// The lines that are JSON; a line cut short by the reading limits is left out.
fn events(lines: impl Iterator<Item = impl AsRef<str>>) -> Vec<Value> {
    lines
        .filter_map(|line| serde_json::from_str(line.as_ref()).ok())
        .collect()
}

/// The words of a message, which is a text or a list of blocks. Blocks that open with `<`
/// are not the user's or the agent's words but what the tool wrapped around them.
fn words(content: &Value) -> Option<&str> {
    let said = |text: &&str| !text.is_empty() && !text.starts_with('<');
    if let Some(text) = content.as_str() {
        return Some(text.trim()).filter(said);
    }
    content.as_array()?.iter().find_map(|block| {
        let kind = block["type"].as_str()?;
        let text = block["text"].as_str()?.trim();
        Some(text).filter(|text| kind.ends_with("text") && said(text))
    })
}

/// A session of Claude Code. `None` when the agent never answered in it.
fn claude(head: &[Value], tail: &[Value]) -> Option<Record> {
    let mut read = Record::default();
    let mut named = false;
    let mut answered = false;
    for event in head.iter().chain(tail) {
        if read.cwd.is_empty() {
            read.cwd = event["cwd"].as_str().unwrap_or_default().into();
        }
        let given = match event["type"].as_str() {
            Some("custom-title") => event["customTitle"].as_str(),
            Some("summary") => event["summary"].as_str(),
            _ => None,
        };
        // A given name wins over the first words, and the latest one over the others.
        if let Some(given) = given.filter(|given| !given.is_empty()) {
            read.title = given.into();
            named = true;
        }
        match event["type"].as_str() {
            Some("user") => {
                let content = &event["message"]["content"];
                if !named && read.title.is_empty() && event["isMeta"] != true {
                    read.title = words(content).unwrap_or_default().into();
                }
                read.working = true;
            }
            Some("assistant") => {
                answered = true;
                let content = &event["message"]["content"];
                let called = content
                    .as_array()
                    .and_then(|blocks| blocks.iter().rfind(|block| block["type"] == "tool_use"));
                if let Some(tool) = called.and_then(|block| block["name"].as_str()) {
                    read.tool = Some(tool.into());
                } else if let Some(text) = words(content) {
                    read.last = text.into();
                    read.tool = None;
                }
                read.working = event["message"]["stop_reason"] != "end_turn";
            }
            _ => {}
        }
    }
    answered.then_some(read)
}

/// A session of Codex. `None` when the file is not one.
fn codex(head: &[Value], tail: &[Value]) -> Option<Record> {
    let mut read = Record {
        working: true,
        ..Record::default()
    };
    for event in head.iter().chain(tail) {
        let payload = &event["payload"];
        match (event["type"].as_str(), payload["type"].as_str()) {
            (Some("session_meta"), _) => {
                read.cwd = payload["cwd"].as_str().unwrap_or_default().into();
                read.id = payload["id"].as_str().map(str::to_owned);
            }
            (Some("event_msg"), Some("task_complete")) => read.working = false,
            (Some("event_msg"), Some("user_message" | "task_started")) => read.working = true,
            (Some("response_item"), Some("function_call" | "custom_tool_call")) => {
                read.tool = payload["name"].as_str().map(str::to_owned);
            }
            (Some("response_item"), _) => match payload["role"].as_str() {
                Some("user") if read.title.is_empty() => {
                    read.title = words(&payload["content"]).unwrap_or_default().into();
                }
                Some("assistant") => {
                    if let Some(text) = words(&payload["content"]) {
                        read.last = text.into();
                        read.tool = None;
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
    read.id.is_some().then_some(read)
}

/// Sample sessions, shown with the sample data.
pub fn samples(now: i64) -> Vec<Session> {
    let sample =
        |engine, project: &str, title: &str, last: &str, tool: Option<&str>, age, state| Session {
            engine,
            id: format!("sample-{project}"),
            project: project.into(),
            title: title.into(),
            last: last.into(),
            tool: tool.map(str::to_owned),
            updated_at: now - age,
            state,
        };
    vec![
        sample(
            Engine::ClaudeCode,
            "sync",
            "Add retry with backoff to the sync worker",
            "",
            Some("Edit"),
            20,
            Progress::Working,
        ),
        sample(
            Engine::Codex,
            "docs",
            "Rewrite the install page for the new installer",
            "The page is rewritten and the links are checked.",
            None,
            3 * 3600,
            Progress::Done,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user(text: &str) -> Value {
        json!({ "type": "user", "cwd": "C:\\work\\sync", "message": { "content": text } })
    }

    fn assistant(content: Value, stop: &str) -> Value {
        json!({ "type": "assistant", "message": { "content": content, "stop_reason": stop } })
    }

    #[test]
    fn a_claude_session_is_titled_by_its_first_words_and_ends_on_its_last() {
        let head = [
            json!({ "type": "user", "isMeta": true, "message": { "content": "Caveat: ignore" } }),
            user("<command-name>/mcp</command-name>"),
            user("Add a retry\nto the sync worker"),
            assistant(json!([{ "type": "text", "text": "On it." }]), "tool_use"),
        ];
        let tail = [assistant(
            json!([{ "type": "text", "text": "Done, tests pass." }]),
            "end_turn",
        )];
        let read = claude(&head, &tail).unwrap();
        assert_eq!(read.title, "Add a retry\nto the sync worker");
        assert_eq!(read.cwd, "C:\\work\\sync");
        assert_eq!(
            (read.last.as_str(), read.tool, read.working),
            ("Done, tests pass.", None, false)
        );
    }

    #[test]
    fn a_claude_session_in_the_middle_of_a_turn_shows_its_tool_and_its_given_name() {
        let head = [user("Fix the build")];
        let tail = [
            assistant(json!([{ "type": "text", "text": "Looking." }]), "tool_use"),
            assistant(json!([{ "type": "tool_use", "name": "Bash" }]), "tool_use"),
            json!({ "type": "user", "message": { "content": [{ "type": "tool_result" }] } }),
            json!({ "type": "custom-title", "customTitle": "Build fix" }),
        ];
        let read = claude(&head, &tail).unwrap();
        assert_eq!(read.title, "Build fix");
        assert_eq!(
            (read.last.as_str(), read.tool.as_deref(), read.working),
            ("Looking.", Some("Bash"), true)
        );
    }

    #[test]
    fn a_claude_session_without_an_answer_is_not_one() {
        assert_eq!(claude(&[user("hello")], &[]), None);
    }

    #[test]
    fn a_codex_session_is_read_the_same_way() {
        let message = |role: &str, kind: &str, text: &str| {
            json!({ "type": "response_item", "payload": {
                "type": "message", "role": role, "content": [{ "type": kind, "text": text }],
            } })
        };
        let head = [
            json!({ "type": "session_meta", "payload": { "id": "abc", "cwd": "/work/docs" } }),
            message(
                "user",
                "input_text",
                "<environment_context>…</environment_context>",
            ),
            message("user", "input_text", "Rewrite the install page"),
        ];
        let tail = [
            json!({ "type": "response_item", "payload": { "type": "function_call", "name": "shell" } }),
            message("assistant", "output_text", "The page is rewritten."),
            json!({ "type": "event_msg", "payload": { "type": "task_complete" } }),
        ];
        let read = codex(&head, &tail).unwrap();
        assert_eq!(
            (read.id.as_deref(), read.cwd.as_str()),
            (Some("abc"), "/work/docs")
        );
        assert_eq!(read.title, "Rewrite the install page");
        assert_eq!(
            (read.last.as_str(), read.tool, read.working),
            ("The page is rewritten.", None, false)
        );
        assert_eq!(codex(&[], &tail), None);
    }

    #[test]
    fn lists_the_recent_session_files_of_both_agents() {
        let home = std::env::temp_dir().join(format!("madow-agents-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        let write = |folder: &str, file: &str, events: &[Value]| {
            let folder = home.join(folder);
            std::fs::create_dir_all(&folder).unwrap();
            let lines: Vec<String> = events.iter().map(Value::to_string).collect();
            std::fs::write(folder.join(file), lines.join("\n")).unwrap();
        };
        let answer = assistant(json!([{ "type": "text", "text": "Done." }]), "end_turn");
        write(
            ".claude/projects/C--work-sync",
            "1f2e.jsonl",
            &[user("Add a retry"), answer],
        );
        write(
            ".claude/projects/C--work-sync",
            "notes.txt",
            &[user("not a session")],
        );
        let meta =
            json!({ "type": "session_meta", "payload": { "id": "abc", "cwd": "/work/docs" } });
        write(".codex/sessions/2026/11/20", "rollout.jsonl", &[meta]);

        let now = modified(&home.join(".claude/projects/C--work-sync/1f2e.jsonl")).unwrap();
        let mut sessions = list(&home, now + 60);
        sessions.sort_by_key(|session| session.id.clone());
        assert_eq!(sessions.len(), 2);
        let (first, second) = (&sessions[0], &sessions[1]);
        assert_eq!(
            (first.engine, first.id.as_str(), first.project.as_str()),
            (Engine::ClaudeCode, "1f2e", "sync")
        );
        assert_eq!(
            (first.title.as_str(), first.state),
            ("Add a retry", Progress::Idle)
        );
        // Nothing says the Codex turn ended, and the file was touched a minute ago.
        assert_eq!(
            (second.engine, second.project.as_str(), second.state),
            (Engine::Codex, "docs", Progress::Working)
        );

        assert_eq!(list(&home, now + 300)[0].state, Progress::Done);
        assert_eq!(list(&home, now + RECENT + 60), []);
        std::fs::remove_dir_all(home).unwrap();
    }

    /// Reads the real sessions of this machine and prints how many there are, not what they say.
    #[test]
    #[ignore = "reads the user's own agent sessions"]
    fn real_sessions_are_listed() {
        let home = std::env::home_dir().unwrap();
        let sessions = list(&home, crate::items::now());
        for session in &sessions {
            println!(
                "{:?} {:?} title: {} chars, project: {} chars",
                session.engine,
                session.state,
                session.title.chars().count(),
                session.project.chars().count()
            );
        }
        assert!(sessions.iter().all(|session| !session.id.is_empty()));
    }
}
