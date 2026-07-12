//! Git hook installer for ax sync, ship evaluate, and memory capture.

use std::fs;
use std::path::Path;

use ax_utils::errors::{AxError, FileError};

const SYNC_LINE: &str = "ax sync --quiet";
const SHIP_LINE: &str = "ax ship --evaluate";
const CAPTURE_COMMIT_LINE: &str = "ax capture-git --limit 1 --quiet";
const CAPTURE_MERGE_LINE: &str = "ax capture-git --limit 20 --quiet";

fn hook_lines(name: &str) -> &'static [&'static str] {
    match name {
        "post-commit" => &[SYNC_LINE, SHIP_LINE, CAPTURE_COMMIT_LINE],
        "post-merge" => &[SYNC_LINE, SHIP_LINE, CAPTURE_MERGE_LINE],
        "post-checkout" => &[SYNC_LINE, SHIP_LINE],
        _ => &[SYNC_LINE, SHIP_LINE],
    }
}

fn merge_hook_content(existing: &str, required: &[&str]) -> String {
    let mut lines: Vec<String> = existing.lines().map(String::from).collect();
    for line in required {
        if !existing.contains(line) {
            lines.push((*line).into());
        }
    }
    lines.join("\n") + "\n"
}

pub fn install_git_sync_hooks(project_root: &Path) -> Result<(), AxError> {
    let hooks_dir = project_root.join(".git").join("hooks");
    if !hooks_dir.exists() {
        return Ok(());
    }
    for name in ["post-commit", "post-merge", "post-checkout"] {
        let hook_path = hooks_dir.join(name);
        let required = hook_lines(name);
        let content = if hook_path.exists() {
            let existing = fs::read_to_string(hook_path.display().to_string()).unwrap_or_default();
            if required.iter().all(|line| existing.contains(line)) {
                continue;
            }
            merge_hook_content(&existing, required)
        } else {
            required.join("\n") + "\n"
        };
        fs::write(hook_path.display().to_string(), content).map_err(|e| {
            AxError::File(FileError::with_path(
                e.to_string(),
                hook_path.display().to_string(),
            ))
        })?;
    }
    Ok(())
}

pub fn remove_git_sync_hooks(project_root: &Path) -> Result<(), AxError> {
    let hooks_dir = project_root.join(".git").join("hooks");
    if !hooks_dir.exists() {
        return Ok(());
    }
    for name in ["post-commit", "post-merge", "post-checkout"] {
        let hook_path = hooks_dir.join(name);
        if hook_path.exists() {
            let content = fs::read_to_string(hook_path.display().to_string()).unwrap_or_default();
            let filtered: String = content
                .lines()
                .filter(|l| {
                    !l.contains("ax sync")
                        && !l.contains("ax ship")
                        && !l.contains("ax capture-git")
                })
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(hook_path.display().to_string(), filtered).map_err(|e| {
                AxError::File(FileError::with_path(
                    e.to_string(),
                    hook_path.display().to_string(),
                ))
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_commit_includes_capture_git() {
        let lines = hook_lines("post-commit");
        assert!(lines.contains(&CAPTURE_COMMIT_LINE));
    }

    #[test]
    fn post_checkout_skips_capture_git() {
        let lines = hook_lines("post-checkout");
        assert!(!lines.iter().any(|l| l.contains("capture-git")));
    }

    #[test]
    fn merge_adds_missing_lines() {
        let merged = merge_hook_content("ax sync --quiet\n", hook_lines("post-commit"));
        assert!(merged.contains(SHIP_LINE));
        assert!(merged.contains(CAPTURE_COMMIT_LINE));
    }
}
