//! Hidden `ax turn-hook start|end` — one local `turn` memory per agent turn that changed files
//! or made a commit (docs/specs/per-turn-memory.md).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use ax_memory::TurnRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TurnPhase {
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HookInput {
    pub conversation: String,
    pub generation: Option<String>,
    pub prompt: String,
    pub root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Snapshot {
    turn: String,
    counter: u64,
    prompt: String,
    head: Option<String>,
    dirty: BTreeMap<String, String>,
}

const PROMPT_CHARS: usize = 300;
const TITLE_CHARS: usize = 80;
const MAX_COMMITS: &str = "50";
const MAX_FILES_IN_BODY: usize = 30;
const MAX_FILES: usize = 200;

/// Hook entry point. Always returns `Ok` and prints nothing: memory capture never blocks a turn.
pub async fn run(phase: TurnPhase) -> Result<(), String> {
    use std::io::{IsTerminal, Read};
    if disabled_by_env(std::env::var("AX_NO_STOP_HOOK").ok().as_deref()) {
        return Ok(());
    }
    let mut stdin = std::io::stdin();
    let mut raw = String::new();
    if stdin.is_terminal() || stdin.read_to_string(&mut raw).is_err() {
        return Ok(());
    }
    let value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
    match phase {
        TurnPhase::Start => {
            if let Some(input) = parse_input(&value) {
                let root = input.root.clone().unwrap_or_else(|| super::resolve_path(None));
                let _ = start_turn(&root, &input);
            }
        }
        TurnPhase::End => end_from_input(&value).await,
    }
    Ok(())
}

/// Turn end for an already-parsed hook payload (Cursor `stop`, Claude `Stop` via `ax stop-hook`).
pub(crate) async fn end_from_input(value: &serde_json::Value) {
    if disabled_by_env(std::env::var("AX_NO_STOP_HOOK").ok().as_deref()) {
        return;
    }
    let Some(input) = parse_input(value) else {
        return;
    };
    let root = input.root.unwrap_or_else(|| super::resolve_path(None));
    let _ = end_turn(&root, &input.conversation, now_ms()).await;
}

pub(crate) fn parse_input(v: &serde_json::Value) -> Option<HookInput> {
    let text = |key: &str| {
        v.get(key)
            .and_then(|x| x.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let conversation = text("conversation_id").or_else(|| text("session_id"))?;
    let root = v
        .get("workspace_roots")
        .and_then(|r| r.get(0))
        .and_then(|r| r.as_str())
        .map(PathBuf::from)
        .or_else(|| text("cwd").map(PathBuf::from));
    Some(HookInput {
        conversation,
        generation: text("generation_id"),
        prompt: text("prompt").unwrap_or_default(),
        root,
    })
}

pub(crate) fn disabled_by_env(value: Option<&str>) -> bool {
    value == Some("1")
}

/// `ax.json` `memory.perTurn: false` switches per-turn memories off; anything else keeps them on.
fn per_turn_enabled(root: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(root.join("ax.json")) else {
        return true;
    };
    let Ok(config) = serde_json::from_str::<serde_json::Value>(&text) else {
        return true;
    };
    config.pointer("/memory/perTurn").and_then(|v| v.as_bool()) != Some(false)
}

fn turns_dir(root: &Path) -> PathBuf {
    ax_context::directory::get_ax_dir(root).join("turns")
}

fn snapshot_path(root: &Path, conversation: &str) -> PathBuf {
    let name = &blake3::hash(conversation.as_bytes()).to_hex()[..16];
    turns_dir(root).join(format!("{name}.json"))
}

fn read_snapshot(root: &Path, conversation: &str) -> Option<Snapshot> {
    let text = std::fs::read_to_string(snapshot_path(root, conversation)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Save the turn-start snapshot. `None` when nothing was written (off, not an ax project, no git).
pub(crate) fn start_turn(root: &Path, input: &HookInput) -> Option<()> {
    if !ax_context::directory::get_ax_dir(root).is_dir() || !per_turn_enabled(root) {
        return None;
    }
    let dirty = dirty_files(root)?;
    let counter = read_snapshot(root, &input.conversation).map_or(1, |s| s.counter + 1);
    let snapshot = Snapshot {
        turn: input.generation.clone().unwrap_or_else(|| counter.to_string()),
        counter,
        prompt: clip(&ax_memory::redact_secrets(&input.prompt), PROMPT_CHARS),
        head: head(root),
        dirty,
    };
    let dir = turns_dir(root);
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join(".gitignore"), "*\n").ok()?;
    let path = snapshot_path(root, &input.conversation);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&snapshot).ok()?).ok()?;
    std::fs::rename(&tmp, &path).ok()
}

/// The memory for the turn since the last snapshot, or `None` when it changed nothing.
pub(crate) fn turn_record(root: &Path, conversation: &str) -> Option<TurnRecord> {
    let snapshot = read_snapshot(root, conversation)?;
    let now_dirty = dirty_files(root)?;
    let commits = commits_since(root, snapshot.head.as_deref())?;

    let mut files: Vec<String> = snapshot
        .dirty
        .keys()
        .chain(now_dirty.keys())
        .filter(|path| snapshot.dirty.get(*path) != now_dirty.get(*path))
        .cloned()
        .collect();
    files.extend(commits.iter().flat_map(|c| c.files.iter().cloned()));
    files.sort();
    files.dedup();
    if files.is_empty() && commits.is_empty() {
        return None;
    }
    files.truncate(MAX_FILES);

    let id_source = format!("{conversation}\n{}", snapshot.turn);
    let id = format!("turn-{}", &blake3::hash(id_source.as_bytes()).to_hex()[..16]);
    Some(TurnRecord {
        id,
        title: turn_title(&snapshot.prompt, files.len()),
        body: ax_memory::redact_secrets(&turn_body(&snapshot.prompt, &files, &commits)),
        files,
    })
}

/// Write the turn memory and prune expired ones. Returns the memory id when one exists.
pub(crate) async fn end_turn(root: &Path, conversation: &str, now_ms: i64) -> Option<String> {
    if !per_turn_enabled(root) {
        return None;
    }
    let record = turn_record(root, conversation)?;
    let ax = ax_core::Ax::open(root).await.ok()?;
    ax_memory::save_turn(ax.db_pool(), &record, now_ms).await.ok()?;
    let _ = ax_memory::prune_turns(ax.db_pool(), now_ms, ax_memory::TURN_RETENTION_DAYS).await;
    Some(record.id)
}

fn turn_title(prompt: &str, file_count: usize) -> String {
    let first_line = prompt.lines().map(str::trim).find(|l| !l.is_empty());
    match first_line {
        Some(line) => clip(line, TITLE_CHARS),
        None => format!("Agent turn changed {file_count} file(s)"),
    }
}

fn turn_body(prompt: &str, files: &[String], commits: &[Commit]) -> String {
    let mut body = prompt.to_string();
    if !files.is_empty() {
        let shown: Vec<&str> = files.iter().take(MAX_FILES_IN_BODY).map(String::as_str).collect();
        body.push_str(&format!("\n\nFiles: {}", shown.join(", ")));
        if files.len() > shown.len() {
            body.push_str(&format!(" (+{} more)", files.len() - shown.len()));
        }
    }
    if !commits.is_empty() {
        body.push_str("\n\nCommits:");
        for commit in commits {
            body.push_str(&format!("\n- {} {}", commit.hash, commit.subject));
        }
    }
    body
}

fn clip(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

fn git_output(root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

fn head(root: &Path) -> Option<String> {
    let out = git_output(root, &["rev-parse", "--verify", "-q", "HEAD"])?;
    Some(String::from_utf8_lossy(&out).trim().to_string()).filter(|s| !s.is_empty())
}

/// Dirty paths (tracked changes and untracked files) mapped to a content hash; `-` when deleted.
fn dirty_files(root: &Path) -> Option<BTreeMap<String, String>> {
    let out = git_output(root, &["status", "--porcelain=v1", "-z", "--untracked-files=all"])?;
    let text = String::from_utf8_lossy(&out);
    let mut entries = text.split('\0');
    let mut dirty = BTreeMap::new();
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (status, path) = entry.split_at(3);
        if status.starts_with('R') || status.starts_with('C') {
            if let Some(source) = entries.next() {
                dirty.insert(source.to_string(), "-".to_string());
            }
        }
        let hash = std::fs::read(root.join(path))
            .map_or_else(|_| "-".to_string(), |bytes| blake3::hash(&bytes).to_hex().to_string());
        dirty.insert(path.to_string(), hash);
    }
    Some(dirty)
}

struct Commit {
    hash: String,
    subject: String,
    files: Vec<String>,
}

/// Commits in `since..HEAD` (all of `HEAD` when the turn started without commits).
fn commits_since(root: &Path, since: Option<&str>) -> Option<Vec<Commit>> {
    let Some(now) = head(root) else {
        return Some(Vec::new());
    };
    if since == Some(now.as_str()) {
        return Some(Vec::new());
    }
    let range = since.map_or_else(|| "HEAD".to_string(), |s| format!("{s}..HEAD"));
    let out = git_output(
        root,
        &["log", "-n", MAX_COMMITS, "--format=%x00%h%x09%s", "--name-only", &range],
    )?;
    let text = String::from_utf8_lossy(&out);
    let commits = text
        .split('\0')
        .filter_map(|chunk| {
            let mut lines = chunk.lines();
            let (hash, subject) = lines.next()?.split_once('\t')?;
            Some(Commit {
                hash: hash.to_string(),
                subject: subject.to_string(),
                files: lines.filter(|l| !l.trim().is_empty()).map(str::to_string).collect(),
            })
        })
        .collect();
    Some(commits)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const DAY_MS: i64 = 86_400_000;
    const NOW: i64 = 1_790_000_000_000;

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(root)
            .args([
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=t",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "core.hooksPath=/dev/null",
            ])
            .args(args)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// A git repo with `.ax/` and one committed file `src/a.rs`.
    fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        write(root, "src/a.rs", "fn a() {}\n");
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "initial"]);
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        dir
    }

    fn input(prompt: &str, generation: Option<&str>) -> HookInput {
        HookInput {
            conversation: "conv-1".into(),
            generation: generation.map(str::to_string),
            prompt: prompt.into(),
            root: None,
        }
    }

    #[test]
    fn parses_cursor_hook_input() {
        let v = serde_json::json!({
            "conversation_id": "c1",
            "generation_id": "g1",
            "prompt": "Fix it",
            "workspace_roots": ["/work/repo", "/work/other"]
        });
        assert_eq!(
            parse_input(&v),
            Some(HookInput {
                conversation: "c1".into(),
                generation: Some("g1".into()),
                prompt: "Fix it".into(),
                root: Some(PathBuf::from("/work/repo")),
            })
        );
    }

    #[test]
    fn parses_claude_hook_input() {
        let v = serde_json::json!({ "session_id": "s1", "cwd": "/work/repo", "prompt": "Fix it" });
        assert_eq!(
            parse_input(&v),
            Some(HookInput {
                conversation: "s1".into(),
                generation: None,
                prompt: "Fix it".into(),
                root: Some(PathBuf::from("/work/repo")),
            })
        );
    }

    #[test]
    fn input_without_a_conversation_is_ignored() {
        assert_eq!(parse_input(&serde_json::json!({ "prompt": "x" })), None);
        assert_eq!(parse_input(&serde_json::Value::Null), None);
    }

    #[test]
    fn only_ax_no_stop_hook_1_disables() {
        assert!(disabled_by_env(Some("1")));
        assert!(!disabled_by_env(Some("0")));
        assert!(!disabled_by_env(None));
    }

    #[test]
    fn t1_file_edited_during_the_turn_is_recorded() {
        let dir = repo();
        let root = dir.path();
        start_turn(root, &input("Fix the cache bug in a.rs", Some("g1"))).unwrap();
        write(root, "src/a.rs", "fn a() { fixed() }\n");

        let record = turn_record(root, "conv-1").expect("turn changed a file");
        assert_eq!(record.files, vec!["src/a.rs".to_string()]);
        assert_eq!(record.title, "Fix the cache bug in a.rs");
        assert!(record.body.contains("Fix the cache bug in a.rs"), "{}", record.body);
        assert!(record.body.contains("src/a.rs"), "{}", record.body);
    }

    #[test]
    fn t2_turn_without_changes_is_not_recorded() {
        let dir = repo();
        let root = dir.path();
        start_turn(root, &input("Explain a.rs", Some("g1"))).unwrap();
        assert_eq!(turn_record(root, "conv-1"), None);
    }

    #[test]
    fn t3_file_dirty_before_the_turn_and_untouched_is_not_listed() {
        let dir = repo();
        let root = dir.path();
        write(root, "src/a.rs", "fn a() { wip() }\n");
        start_turn(root, &input("Explain a.rs", Some("g1"))).unwrap();
        assert_eq!(turn_record(root, "conv-1"), None);

        write(root, "src/b.rs", "fn b() {}\n");
        let record = turn_record(root, "conv-1").expect("b.rs is new");
        assert_eq!(record.files, vec!["src/b.rs".to_string()]);
    }

    #[test]
    fn file_dirty_before_the_turn_and_edited_again_is_listed() {
        let dir = repo();
        let root = dir.path();
        write(root, "src/a.rs", "fn a() { wip() }\n");
        start_turn(root, &input("Finish a.rs", Some("g1"))).unwrap();
        write(root, "src/a.rs", "fn a() { done() }\n");
        let record = turn_record(root, "conv-1").expect("a.rs changed again");
        assert_eq!(record.files, vec!["src/a.rs".to_string()]);
    }

    #[test]
    fn t4_commit_during_the_turn_lists_subject_and_files() {
        let dir = repo();
        let root = dir.path();
        start_turn(root, &input("Fix eviction", Some("g1"))).unwrap();
        write(root, "src/a.rs", "fn a() { evict() }\n");
        git(root, &["commit", "-q", "-am", "Fix cache eviction"]);

        let record = turn_record(root, "conv-1").expect("turn made a commit");
        assert_eq!(record.files, vec!["src/a.rs".to_string()]);
        assert!(record.body.contains("Fix cache eviction"), "{}", record.body);
    }

    #[test]
    fn new_file_in_a_new_directory_and_deleted_file_are_listed() {
        let dir = repo();
        let root = dir.path();
        start_turn(root, &input("Move a.rs", Some("g1"))).unwrap();
        write(root, "src/new/x.rs", "fn x() {}\n");
        std::fs::remove_file(root.join("src/a.rs")).unwrap();

        let record = turn_record(root, "conv-1").expect("files changed");
        assert_eq!(
            record.files,
            vec!["src/a.rs".to_string(), "src/new/x.rs".to_string()]
        );
    }

    #[test]
    fn snapshot_file_itself_is_not_a_change() {
        let dir = repo();
        let root = dir.path();
        start_turn(root, &input("Explain", Some("g1"))).unwrap();
        start_turn(root, &input("Explain again", Some("g2"))).unwrap();
        assert_eq!(turn_record(root, "conv-1"), None);
    }

    #[test]
    fn t5_same_turn_end_twice_gives_the_same_id() {
        let dir = repo();
        let root = dir.path();
        start_turn(root, &input("Edit", Some("g1"))).unwrap();
        write(root, "src/a.rs", "fn a() { 1 }\n");
        let first = turn_record(root, "conv-1").unwrap();
        let second = turn_record(root, "conv-1").unwrap();
        assert_eq!(first.id, second.id);
    }

    #[test]
    fn t5_different_turns_get_different_ids() {
        let dir = repo();
        let root = dir.path();
        start_turn(root, &input("Edit", Some("g1"))).unwrap();
        write(root, "src/a.rs", "fn a() { 1 }\n");
        let cursor_one = turn_record(root, "conv-1").unwrap().id;
        start_turn(root, &input("Edit", Some("g2"))).unwrap();
        write(root, "src/a.rs", "fn a() { 2 }\n");
        let cursor_two = turn_record(root, "conv-1").unwrap().id;
        assert_ne!(cursor_one, cursor_two);

        start_turn(root, &input("Edit", None)).unwrap();
        write(root, "src/a.rs", "fn a() { 3 }\n");
        let claude_one = turn_record(root, "conv-1").unwrap().id;
        start_turn(root, &input("Edit", None)).unwrap();
        write(root, "src/a.rs", "fn a() { 4 }\n");
        let claude_two = turn_record(root, "conv-1").unwrap().id;
        assert_ne!(claude_one, claude_two);
        assert_ne!(claude_one, cursor_two);
    }

    #[test]
    fn t6_no_snapshot_means_no_record() {
        let dir = repo();
        let root = dir.path();
        write(root, "src/a.rs", "fn a() { 1 }\n");
        assert_eq!(turn_record(root, "conv-1"), None);
        start_turn(root, &input("Edit", Some("g1"))).unwrap();
        write(root, "src/a.rs", "fn a() { 2 }\n");
        assert_eq!(turn_record(root, "other-conversation"), None);
    }

    #[tokio::test]
    async fn t7_per_turn_false_in_ax_json_disables_both_hooks() {
        let dir = repo();
        let root = dir.path();
        start_turn(root, &input("Edit", Some("g1"))).unwrap();
        write(root, "src/a.rs", "fn a() { 1 }\n");
        write(root, "ax.json", r#"{ "memory": { "perTurn": false } }"#);

        assert_eq!(end_turn(root, "conv-1", NOW).await, None);
        std::fs::remove_dir_all(root.join(".ax/turns")).unwrap();
        assert_eq!(start_turn(root, &input("Edit", Some("g2"))), None);
        assert!(!root.join(".ax/turns").exists());
    }

    #[test]
    fn per_turn_true_or_other_ax_json_keeps_it_on() {
        let dir = repo();
        let root = dir.path();
        write(root, "ax.json", r#"{ "memory": { "perTurn": true }, "members": [] }"#);
        start_turn(root, &input("Edit", Some("g1"))).unwrap();
        write(root, "src/a.rs", "fn a() { 1 }\n");
        assert!(turn_record(root, "conv-1").is_some());
    }

    #[test]
    fn t12_secret_in_the_prompt_never_reaches_disk_or_the_record() {
        let dir = repo();
        let root = dir.path();
        let key = "sk-abcdefghijklmnopqrstuvwx1234";
        start_turn(root, &input(&format!("Use key {key} for the client"), Some("g1"))).unwrap();
        for entry in std::fs::read_dir(root.join(".ax/turns")).unwrap() {
            let text = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            assert!(!text.contains(key), "snapshot leaked the key");
        }
        write(root, "src/a.rs", "fn a() { 1 }\n");
        let record = turn_record(root, "conv-1").unwrap();
        assert!(!record.body.contains(key) && !record.title.contains(key));
        assert!(record.body.contains("[redacted]"), "{}", record.body);
    }

    #[test]
    fn prompt_is_cut_to_300_characters_and_title_to_80() {
        let dir = repo();
        let root = dir.path();
        let prompt = "word ".repeat(100);
        start_turn(root, &input(&prompt, Some("g1"))).unwrap();
        write(root, "src/a.rs", "fn a() { 1 }\n");
        let record = turn_record(root, "conv-1").unwrap();
        assert_eq!(record.title.chars().count(), 80);
        assert!(record.body.contains(&prompt[..300]));
        assert!(!record.body.contains(&prompt[..301]));
    }

    #[test]
    fn t14_directory_without_git_or_ax_writes_nothing() {
        let plain = tempfile::tempdir().unwrap();
        assert_eq!(start_turn(plain.path(), &input("Edit", Some("g1"))), None);
        assert!(!plain.path().join(".ax").exists());

        let no_git = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(no_git.path().join(".ax")).unwrap();
        assert_eq!(start_turn(no_git.path(), &input("Edit", Some("g1"))), None);
        assert_eq!(turn_record(no_git.path(), "conv-1"), None);
    }

    #[tokio::test]
    async fn end_turn_saves_the_memory_and_prunes_old_turns() {
        let dir = repo();
        let root = dir.path();
        let ax = ax_core::Ax::init(root).await.unwrap();
        let old = TurnRecord {
            id: "turn-old".into(),
            title: "old".into(),
            body: "old".into(),
            files: vec![],
        };
        ax_memory::save_turn(ax.db_pool(), &old, NOW - 31 * DAY_MS).await.unwrap();
        drop(ax);

        start_turn(root, &input("Fix the cache bug", Some("g1"))).unwrap();
        write(root, "src/a.rs", "fn a() { fixed() }\n");
        let id = end_turn(root, "conv-1", NOW).await.expect("memory written");
        assert_eq!(end_turn(root, "conv-1", NOW).await.as_deref(), Some(id.as_str()));

        let ax = ax_core::Ax::open(root).await.unwrap();
        let row = ax_memory::get(ax.db_pool(), &id).await.unwrap().expect("saved");
        assert_eq!(row.kind, ax_memory::TURN_KIND);
        assert_eq!(row.files, vec!["src/a.rs".to_string()]);
        assert!(ax_memory::get(ax.db_pool(), "turn-old").await.unwrap().is_none());
    }
}
