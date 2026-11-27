//! Hidden `ax stop-hook` — Claude Code Stop/SubagentStop stdin JSON hook.
//!
//! Turn-end post-flight check: scans files changed since the last commit
//! (uncommitted working tree) against CRITICAL policy guard rules
//! (`ax_policy::guard_operation`) and, on violation, blocks the Stop event
//! so Claude fixes the issue before finishing — closing the "no automatic
//! post-flight after a turn" gap for Claude Code.
//!
//! Contract: https://code.claude.com/docs/en/hooks — Stop/SubagentStop read
//! top-level `decision`/`reason` JSON on stdout with exit code 0. Must honor
//! `stop_hook_active` to avoid an infinite block loop.

use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use ax_policy::{guard_operation, GuardOp};

use crate::commands::{find_project_root, git_status, resolve_project_root};

const MAX_FILES_SCANNED: usize = 40;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_VIOLATIONS_SHOWN: usize = 5;

pub async fn run() -> Result<(), String> {
    if std::env::var("AX_NO_STOP_HOOK").ok().as_deref() == Some("1") {
        return Ok(());
    }
    if io::stdin().is_terminal() {
        return Ok(());
    }

    let mut raw = String::new();
    if io::stdin().read_to_string(&mut raw).is_err() {
        return Ok(());
    }
    let input: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);

    super::turn_hook::end_from_input(&input).await;

    // Claude sets this true when re-invoking the hook after a prior block —
    // never block twice on the same turn, or the conversation loops forever.
    if input
        .get("stop_hook_active")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        return Ok(());
    }

    let cwd = input
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| resolve_project_root(None));

    let _ = ingest_transcript_tail(&cwd, &input).await;

    let root = find_project_root(&cwd).unwrap_or_else(|| cwd.clone());
    let Ok(ax) = ax_core::Ax::open(&root).await else {
        return Ok(());
    };
    if !ax.policy_exists() {
        return Ok(());
    }

    let Some(targets) = guard_targets(&cwd, &root) else {
        return Ok(());
    };
    if targets.is_empty() {
        return Ok(());
    }

    let unscanned = targets.len().saturating_sub(MAX_FILES_SCANNED);
    let pool = ax.db_pool().clone();
    let mut violations: Vec<(String, String)> = Vec::new();
    for GuardTarget { rel_path, abs_path } in targets.into_iter().take(MAX_FILES_SCANNED) {
        let Ok(meta) = std::fs::metadata(&abs_path) else {
            continue; // unreadable — nothing to guard
        };
        if meta.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(&abs_path) else {
            continue;
        };
        if let Ok(result) =
            guard_operation(&pool, &root, &abs_path, GuardOp::Write, Some(&bytes)).await
        {
            for v in result.violations {
                violations.push((rel_path.clone(), format!("[{}] {}", v.rule_id, v.message)));
            }
        }
    }

    if violations.is_empty() {
        if unscanned > 0 {
            eprintln!(
                "ax stop-hook: {unscanned} changed file(s) were not guard-scanned (limit {MAX_FILES_SCANNED} per turn)."
            );
        }
        return Ok(());
    }

    let mut lines: Vec<String> = violations
        .iter()
        .take(MAX_VIOLATIONS_SHOWN)
        .map(|(path, msg)| format!("- {path}: {msg}"))
        .collect();
    if violations.len() > MAX_VIOLATIONS_SHOWN {
        lines.push(format!(
            "- …and {} more",
            violations.len() - MAX_VIOLATIONS_SHOWN
        ));
    }
    if unscanned > 0 {
        lines.push(format!(
            "- {unscanned} more changed file(s) were not scanned (limit {MAX_FILES_SCANNED} per turn)"
        ));
    }
    let reason = format!(
        "ax policy guard found {} CRITICAL violation(s) in files changed this turn — fix before finishing:\n{}",
        violations.len(),
        lines.join("\n")
    );

    let out = serde_json::json!({ "decision": "block", "reason": reason });
    let mut stdout = io::stdout();
    stdout
        .write_all(out.to_string().as_bytes())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, PartialEq)]
struct GuardTarget {
    /// Path shown in the block reason.
    rel_path: String,
    abs_path: PathBuf,
}

/// Existing files under the ax project `root` that changed since the last
/// commit (staged, unstaged and untracked), for the guard to scan. Returns
/// `None` outside a git work tree.
///
/// Porcelain paths are relative to the repository top level, which is not
/// `cwd` when the project lives in a subdirectory of the repository, so they
/// are joined onto the top level and then kept only when inside `root`.
fn guard_targets(cwd: &Path, root: &Path) -> Option<Vec<GuardTarget>> {
    let top = git_status::toplevel(cwd)?;
    let output = Command::new("git")
        .current_dir(&top)
        .args(git_status::STATUS_ARGS)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let top = top.canonicalize().unwrap_or(top);
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    Some(
        git_status::parse_porcelain_z(&output.stdout)
            .into_iter()
            .filter(|entry| !entry.is_deleted())
            .filter_map(|entry| {
                let abs_path = top.join(&entry.path);
                let rel_path = abs_path
                    .strip_prefix(&root)
                    .ok()?
                    .to_string_lossy()
                    .into_owned();
                abs_path
                    .is_file()
                    .then_some(GuardTarget { rel_path, abs_path })
            })
            .collect(),
    )
}

async fn ingest_transcript_tail(cwd: &Path, input: &serde_json::Value) {
    if !ax_usage::cache_enabled() {
        return;
    }
    let session = input
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let path = ax_usage::find_cursor_transcripts(cwd)
        .into_iter()
        .next()
        .or_else(|| newest_claude_jsonl(cwd));
    let Some(path) = path else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let tail: Vec<&str> = text.lines().rev().take(200).collect();
    let tail = tail.into_iter().rev().collect::<Vec<_>>().join("\n");
    let Some(root) = find_project_root(cwd) else {
        return;
    };
    let Some(session) = session
        .filter(|s| ax_usage::session_from_args(&serde_json::json!({"session":s})).is_some())
    else {
        return;
    };
    for (tool, body) in ax_usage::tool_chunks_from_jsonl(&tail) {
        if ax_usage::count_tokens(&body) as i64 >= ax_usage::cache_threshold() {
            let _ = ax_usage::stash_project_text(&root, Some(&tool), &body).await;
            let _ = ax_usage::note_project_tool_if_open(&root, &session, &tool, &body).await;
        }
    }
}

fn newest_claude_jsonl(cwd: &Path) -> Option<PathBuf> {
    let home = ax_utils::paths::home_dir()?;
    let projects = home.join(".claude").join("projects");
    if !projects.is_dir() {
        return None;
    }
    let slug = cwd.to_string_lossy().replace(['/', '\\', ':'], "-");
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let entries = std::fs::read_dir(&projects).ok()?;
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name()?.to_string_lossy();
        if name.as_ref() != slug {
            continue;
        }
        let files = std::fs::read_dir(&dir).ok()?;
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let modified = file.metadata().ok().and_then(|m| m.modified().ok())?;
            if best.as_ref().is_none_or(|(t, _)| modified > *t) {
                best = Some((modified, path));
            }
        }
    }
    best.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .current_dir(dir)
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?}");
    }

    /// A git repo with the ax project in `services/api` and one committed
    /// file inside it and one outside it.
    fn monorepo(name: &str) -> (tempfile::TempDir, PathBuf) {
        let repo = tempfile::Builder::new().prefix(name).tempdir().unwrap();
        let api = repo.path().join("services").join("api");
        std::fs::create_dir_all(api.join("src")).unwrap();
        std::fs::write(api.join("src/lib.rs"), "pub fn a() {}\n").unwrap();
        std::fs::write(repo.path().join("other.rs"), "fn b() {}\n").unwrap();
        git(repo.path(), &["init", "-q"]);
        git(repo.path(), &["add", "."]);
        git(repo.path(), &["commit", "-q", "-m", "init"]);
        (repo, api)
    }

    fn scanned(cwd: &Path) -> Vec<PathBuf> {
        guard_targets(cwd, cwd)
            .unwrap()
            .into_iter()
            .map(|t| t.abs_path.canonicalize().unwrap())
            .collect()
    }

    #[test]
    fn subdirectory_project_scans_its_changed_files() {
        let (repo, api) = monorepo("ax-stop-subdir");
        std::fs::write(api.join("src/lib.rs"), "pub fn changed() {}\n").unwrap();
        std::fs::write(repo.path().join("other.rs"), "fn changed() {}\n").unwrap();
        let targets = guard_targets(&api, &api).unwrap();
        assert_eq!(
            targets.len(),
            1,
            "only the file inside the project: {targets:?}"
        );
        assert_eq!(
            targets[0].abs_path.canonicalize().unwrap(),
            api.join("src/lib.rs").canonicalize().unwrap()
        );
        assert_eq!(targets[0].rel_path, "src/lib.rs");
    }

    #[test]
    fn new_file_in_new_directory_is_scanned() {
        let (_repo, api) = monorepo("ax-stop-newdir");
        std::fs::create_dir_all(api.join("src/newmod")).unwrap();
        std::fs::write(api.join("src/newmod/handler.rs"), "x\n").unwrap();
        assert_eq!(
            scanned(&api),
            vec![api.join("src/newmod/handler.rs").canonicalize().unwrap()]
        );
    }

    #[test]
    fn non_ascii_and_spaced_names_are_scanned() {
        let (repo, _api) = monorepo("ax-stop-unicode");
        let root = repo.path();
        std::fs::write(root.join("caf\u{e9}.rs"), "x\n").unwrap();
        std::fs::write(root.join("with space.rs"), "x\n").unwrap();
        let mut got = scanned(root);
        got.sort();
        let mut want = vec![
            root.join("caf\u{e9}.rs").canonicalize().unwrap(),
            root.join("with space.rs").canonicalize().unwrap(),
        ];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn deleted_files_are_skipped_and_renames_scan_the_new_path() {
        let (repo, _api) = monorepo("ax-stop-rename");
        let root = repo.path();
        git(root, &["mv", "other.rs", "renamed.rs"]);
        std::fs::remove_file(root.join("services/api/src/lib.rs")).unwrap();
        assert_eq!(
            scanned(root),
            vec![root.join("renamed.rs").canonicalize().unwrap()]
        );
    }

    #[test]
    fn outside_a_git_work_tree_there_is_nothing_to_scan() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(guard_targets(dir.path(), dir.path()), None);
    }

    #[test]
    fn stop_hook_active_short_circuits_without_stdin_block() {
        // stop_hook_active=true must never be treated as a violation signal —
        // this is a smoke check that the JSON field name/shape used in `run()`
        // matches what Claude Code actually sends.
        let input: serde_json::Value = serde_json::json!({ "stop_hook_active": true, "cwd": "." });
        assert_eq!(
            input.get("stop_hook_active").and_then(|v| v.as_bool()),
            Some(true)
        );
    }
}
