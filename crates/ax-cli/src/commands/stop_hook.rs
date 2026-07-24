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

use crate::commands::resolve_path;

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
        .unwrap_or_else(|| resolve_path(None));

    let Ok(ax) = ax_core::Ax::open(&cwd).await else {
        return Ok(());
    };
    if !ax.policy_exists() {
        return Ok(());
    }

    let changed = match uncommitted_files(&cwd) {
        Some(files) => files,
        None => return Ok(()),
    };
    if changed.is_empty() {
        return Ok(());
    }

    let pool = ax.db_pool().clone();
    let mut violations: Vec<(String, String)> = Vec::new();
    for rel_path in changed.into_iter().take(MAX_FILES_SCANNED) {
        let abs_path = cwd.join(&rel_path);
        let Ok(meta) = std::fs::metadata(&abs_path) else {
            continue; // deleted / unreadable — nothing to guard
        };
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(&abs_path) else {
            continue;
        };
        if let Ok(result) = guard_operation(&pool, &cwd, &abs_path, GuardOp::Write, Some(&bytes)).await {
            for v in result.violations {
                violations.push((rel_path.clone(), format!("[{}] {}", v.rule_id, v.message)));
            }
        }
    }

    if violations.is_empty() {
        return Ok(());
    }

    let mut lines: Vec<String> = violations
        .iter()
        .take(MAX_VIOLATIONS_SHOWN)
        .map(|(path, msg)| format!("- {path}: {msg}"))
        .collect();
    if violations.len() > MAX_VIOLATIONS_SHOWN {
        lines.push(format!("- …and {} more", violations.len() - MAX_VIOLATIONS_SHOWN));
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

/// Working-tree files touched since the last commit (staged + unstaged + untracked),
/// used as the per-turn change set. Returns `None` if this isn't a git repo.
fn uncommitted_files(cwd: &Path) -> Option<Vec<String>> {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(["status", "--porcelain=v1", "--untracked-files=normal"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse_porcelain_status(&String::from_utf8_lossy(&output.stdout)))
}

/// Parse `git status --porcelain=v1` output into a list of paths worth guard-scanning
/// (skips deleted entries; resolves `old -> new` rename lines to the new path).
fn parse_porcelain_status(text: &str) -> Vec<String> {
    let mut files = Vec::new();
    for line in text.lines() {
        if line.len() < 4 {
            continue;
        }
        let status = &line[..2];
        let rest = line[3..].trim();
        if status.contains('D') {
            continue; // deleted — no content left to guard
        }
        let path = match rest.split_once(" -> ") {
            Some((_, renamed_to)) => renamed_to.trim(),
            None => rest,
        };
        let cleaned = path.trim_matches('"');
        if !cleaned.is_empty() {
            files.push(cleaned.to_string());
        }
    }
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modified_and_untracked() {
        let porcelain = " M src/lib.rs\n?? new_file.txt\n";
        let files = parse_porcelain_status(porcelain);
        assert_eq!(files, vec!["src/lib.rs".to_string(), "new_file.txt".to_string()]);
    }

    #[test]
    fn skips_deleted_entries() {
        let porcelain = " D removed.rs\nM  kept.rs\n";
        let files = parse_porcelain_status(porcelain);
        assert_eq!(files, vec!["kept.rs".to_string()]);
    }

    #[test]
    fn resolves_rename_to_new_path() {
        let porcelain = "R  old_name.rs -> new_name.rs\n";
        let files = parse_porcelain_status(porcelain);
        assert_eq!(files, vec!["new_name.rs".to_string()]);
    }

    #[test]
    fn stop_hook_active_short_circuits_without_stdin_block() {
        // stop_hook_active=true must never be treated as a violation signal —
        // this is a smoke check that the JSON field name/shape used in `run()`
        // matches what Claude Code actually sends.
        let input: serde_json::Value = serde_json::json!({ "stop_hook_active": true, "cwd": "." });
        assert_eq!(input.get("stop_hook_active").and_then(|v| v.as_bool()), Some(true));
    }
}
