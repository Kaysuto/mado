//! The Code space: the git repositories found in the folders the user chose, what state they
//! are in, and how to open one in the user's editor or terminal.

use crate::{
    error::{Error, Result},
    items::now,
    settings,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

/// Setting: the folders the user chose, as an array of paths.
pub const FOLDERS: &str = "code.folders";

/// A git repository on this machine.
#[derive(Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    /// Empty for a sample repository, which is nowhere on disk.
    pub path: String,
    pub name: String,
    /// The folder it sits in, when it is not itself one of the chosen folders.
    pub group: Option<String>,
    /// Empty when no branch is checked out.
    pub branch: String,
    /// Files changed or new since the last commit.
    pub changes: usize,
    /// Commits not pushed yet, and commits not pulled yet.
    pub ahead: u32,
    pub behind: u32,
    /// Unix seconds.
    pub last_commit: Option<i64>,
    /// `owner/name` of the remote it is pushed to, as the forges write it.
    pub remote: Option<String>,
}

/// The folders the user chose.
pub fn folders(conn: &Connection) -> Result<Vec<PathBuf>> {
    let chosen = settings::get(conn, FOLDERS)?;
    Ok(chosen
        .and_then(|value| serde_json::from_value::<Vec<PathBuf>>(value).ok())
        .unwrap_or_default())
}

/// The repositories in `folders`: a folder is one itself, or holds some up to two levels down.
pub fn find(folders: &[PathBuf]) -> Vec<PathBuf> {
    fn search(folder: &Path, depth: u8, found: &mut Vec<PathBuf>) {
        if folder.join(".git").exists() {
            found.push(folder.to_owned());
        } else if depth > 0 {
            let inside = std::fs::read_dir(folder).into_iter().flatten().flatten();
            for entry in inside.filter(|entry| entry.path().is_dir()) {
                search(&entry.path(), depth - 1, found);
            }
        }
    }
    let mut found = Vec::new();
    for folder in folders {
        search(folder, 2, &mut found);
    }
    found.sort();
    found.dedup();
    found
}

/// Every repository of `folders` with its state, the one committed to last first.
pub async fn list(folders: &[PathBuf]) -> Result<Vec<Repository>> {
    let paths = find(folders);
    if paths.is_empty() {
        return Ok(Vec::new());
    }
    let git = which::which("git").map_err(|_| Error::Message("Git is not installed.".into()))?;
    let described = paths.iter().map(|path| describe(&git, path, folders));
    let mut repositories = futures::future::join_all(described).await;
    repositories.sort_by_key(|repository| std::cmp::Reverse(repository.last_commit));
    Ok(repositories)
}

async fn describe(git: &Path, path: &Path, folders: &[PathBuf]) -> Repository {
    let (status, last, remote) = tokio::join!(
        run(git, path, &["status", "--porcelain=v2", "--branch"]),
        run(git, path, &["log", "-1", "--format=%ct"]),
        run(git, path, &["remote", "get-url", "origin"]),
    );
    let name = |path: &Path| {
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    };
    Repository {
        path: path.to_string_lossy().into_owned(),
        name: name(path).unwrap_or_default(),
        group: path
            .parent()
            .filter(|_| !folders.iter().any(|folder| folder == path))
            .and_then(name),
        last_commit: last.trim().parse().ok(),
        remote: slug(&remote),
        ..read_status(&status)
    }
}

/// What a git command prints; nothing when it fails (no commit yet, no remote…).
async fn run(git: &Path, folder: &Path, arguments: &[&str]) -> String {
    let mut command = tokio::process::Command::new(git);
    command
        .arg("-C")
        .arg(folder)
        .args(arguments)
        .stdin(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    match command.output().await {
        Ok(output) if output.status.success() => String::from_utf8_lossy(&output.stdout).into(),
        _ => String::new(),
    }
}

/// Reads `git status --porcelain=v2 --branch`: header lines start with `#`, then one line per
/// changed or new file.
fn read_status(text: &str) -> Repository {
    let mut repository = Repository::default();
    for line in text.lines() {
        if let Some(branch) = line.strip_prefix("# branch.head ") {
            if branch != "(detached)" {
                repository.branch = branch.into();
            }
        } else if let Some(counts) = line.strip_prefix("# branch.ab ") {
            // "+2 -1": the sign is dropped.
            let mut counts = counts
                .split(' ')
                .map(|count| count.get(1..).and_then(|n| n.parse().ok()).unwrap_or(0));
            repository.ahead = counts.next().unwrap_or(0);
            repository.behind = counts.next().unwrap_or(0);
        } else if !line.starts_with('#') && !line.is_empty() {
            repository.changes += 1;
        }
    }
    repository
}

/// `owner/name` out of a remote address, whichever way it is written:
/// `https://host/owner/name.git`, `git@host:owner/name.git`, `ssh://git@host:22/owner/name`.
fn slug(remote: &str) -> Option<String> {
    let remote = remote.trim().trim_end_matches('/');
    let remote = remote.strip_suffix(".git").unwrap_or(remote);
    let path = match remote.split_once("://") {
        Some((_, rest)) => rest.split_once('/')?.1,
        None => remote.split_once(':')?.1,
    };
    (!path.is_empty()).then(|| path.to_owned())
}

/// What a repository can be opened with.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Editor,
    Terminal,
}

impl Tool {
    /// The command the user set for this tool, or the usual one. It runs in the repository.
    pub fn command(self, conn: &Connection) -> Result<String> {
        let (setting, usual) = match self {
            Tool::Editor => ("code.editor", "code ."),
            Tool::Terminal if cfg!(windows) => ("code.terminal", "wt -d ."),
            Tool::Terminal if cfg!(target_os = "macos") => ("code.terminal", "open -a Terminal ."),
            Tool::Terminal => ("code.terminal", "x-terminal-emulator"),
        };
        let set = settings::get(conn, setting)?;
        let set = set.as_ref().and_then(|value| value.as_str()).map(str::trim);
        Ok(set
            .filter(|set| !set.is_empty())
            .unwrap_or(usual)
            .to_owned())
    }
}

/// `path` as a repository of the chosen folders: nothing else can be opened or worked in
/// from the interface.
pub fn repository(conn: &Connection, path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    if !find(&folders(conn)?).iter().any(|found| found == path) {
        return Err(Error::Message("This repository is no longer there.".into()));
    }
    Ok(path.to_owned())
}

/// Opens one of the user's repositories with their editor or terminal.
pub fn open(conn: &Connection, path: &str, tool: Tool) -> Result<()> {
    let path = repository(conn, path)?;
    let line = tool.command(conn)?;
    // shortcut: words are split on spaces, so a program path with a space cannot be set;
    // read quotes if someone needs one.
    let mut words = line.split_whitespace();
    let program = words.next().unwrap_or_default();
    // The full path, extension included: Windows does not find `.cmd` launchers by bare name.
    let program = which::which(program)
        .map_err(|_| Error::Message(format!("`{program}` is not installed.")))?;
    let mut command = std::process::Command::new(program);
    command.args(words).current_dir(path).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // An editor launcher is a script whose console must not flash on screen; a terminal
        // needs a console of its own. CREATE_NO_WINDOW, CREATE_NEW_CONSOLE.
        command.creation_flags(match tool {
            Tool::Editor => 0x0800_0000,
            Tool::Terminal => 0x0000_0010,
        });
    }
    let mut child = command.spawn()?;
    // Waited for so that it leaves no dead process behind; the app never reads its output.
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Sample repositories, to look around without choosing a folder. They match the sample items.
pub fn samples() -> Vec<Repository> {
    const HOUR: i64 = 3600;
    let sample = |name: &str, branch: &str, changes, ahead, behind, age| Repository {
        name: name.into(),
        branch: branch.into(),
        changes,
        ahead,
        behind,
        last_commit: Some(now() - age),
        remote: Some(format!("acme/{name}")),
        ..Repository::default()
    };
    vec![
        sample("sync", "retry-backoff", 3, 2, 0, HOUR / 2),
        sample("desktop", "main", 0, 0, 0, 5 * HOUR),
        sample("docs", "main", 1, 0, 4, 70 * HOUR),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use serde_json::json;

    #[test]
    fn reads_the_branch_the_distance_to_the_remote_and_the_changes() {
        let status = "# branch.oid 1f2e3d\n# branch.head retry-backoff\n# branch.upstream origin/retry-backoff\n# branch.ab +2 -1\n1 .M N... 100644 100644 100644 abc def src/a.rs\n? notes.txt\n";
        let read = read_status(status);
        assert_eq!(read.branch, "retry-backoff");
        assert_eq!((read.ahead, read.behind, read.changes), (2, 1, 2));
    }

    #[test]
    fn a_detached_or_unreadable_repository_has_no_branch() {
        assert_eq!(
            read_status("# branch.oid 1f2e3d\n# branch.head (detached)\n").branch,
            ""
        );
        assert_eq!(read_status(""), Repository::default());
    }

    #[test]
    fn a_remote_is_named_the_same_however_it_is_written() {
        for remote in [
            "https://github.com/acme/app.git\n",
            "https://github.com/acme/app/",
            "git@github.com:acme/app.git",
            "ssh://git@git.example.com:2222/acme/app.git",
        ] {
            assert_eq!(slug(remote).as_deref(), Some("acme/app"), "{remote}");
        }
        assert_eq!(
            slug("git@gitlab.com:group/team/app.git").as_deref(),
            Some("group/team/app")
        );
        assert_eq!(slug(""), None);
        assert_eq!(slug("/srv/git/app"), None);
    }

    /// A folder of its own under the system's temporary folder, emptied first.
    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("madow-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn finds_repositories_up_to_two_levels_down_and_not_inside_one() {
        let root = scratch("find");
        for folder in [
            "app/.git",
            "app/vendor/lib/.git",
            "work/api/.git",
            "work/a/b/deep/.git",
            "notes",
        ] {
            std::fs::create_dir_all(root.join(folder)).unwrap();
        }
        let found = find(&[root.clone(), root.join("app")]);
        assert_eq!(found, [root.join("app"), root.join("work").join("api")]);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn only_a_repository_of_the_chosen_folders_can_be_opened() {
        let conn = open_in_memory();
        let root = scratch("open");
        std::fs::create_dir_all(root.join("app/.git")).unwrap();
        std::fs::create_dir_all(root.join("other")).unwrap();
        settings::set(&conn, FOLDERS, &json!([root.join("app")])).unwrap();
        settings::set(&conn, "code.editor", &json!("madow-no-such-editor .")).unwrap();

        let outside = open(&conn, &root.join("other").to_string_lossy(), Tool::Editor);
        assert_eq!(
            outside.unwrap_err().to_string(),
            "This repository is no longer there."
        );
        // The repository passes the check, and the user's own command is the one looked for.
        let inside = open(&conn, &root.join("app").to_string_lossy(), Tool::Editor);
        assert_eq!(
            inside.unwrap_err().to_string(),
            "`madow-no-such-editor` is not installed."
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_usual_tool_is_used_until_the_user_sets_one() {
        let conn = open_in_memory();
        assert_eq!(Tool::Editor.command(&conn).unwrap(), "code .");
        settings::set(&conn, "code.editor", &json!("  ")).unwrap();
        assert_eq!(Tool::Editor.command(&conn).unwrap(), "code .");
        settings::set(&conn, "code.editor", &json!("zed .")).unwrap();
        assert_eq!(Tool::Editor.command(&conn).unwrap(), "zed .");
    }

    /// Runs the real `git` on the checkout this code is built from, when it is one.
    #[tokio::test]
    async fn describes_the_repository_this_code_lives_in() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_owned();
        if !root.join(".git").exists() {
            return;
        }
        let repositories = list(std::slice::from_ref(&root)).await.unwrap();
        assert_eq!(repositories.len(), 1);
        assert_eq!(repositories[0].path, root.to_string_lossy());
        assert_eq!(repositories[0].group, None);
        assert!(repositories[0].last_commit.is_some());
    }
}
