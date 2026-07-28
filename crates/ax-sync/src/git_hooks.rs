//! Git hook installer for ax sync, ship evaluate, memory capture, and policy pack sync.

use std::fs;
use std::path::Path;

use ax_utils::errors::{AxError, FileError};

const SYNC_LINE: &str = "ax sync --quiet";
const SHIP_LINE: &str = "ax ship --evaluate";
const CAPTURE_COMMIT_LINE: &str = "ax capture-git --limit 1 --quiet";
const CAPTURE_MERGE_LINE: &str = "ax capture-git --limit 20 --quiet";
const MEMORY_EXPORT_LINE: &str = "ax memory export --quiet";
const MEMORY_IMPORT_LINE: &str = "ax memory import --quiet";
const POLICY_PACK_EXPORT_LINE: &str = "ax policy pack export --quiet";
const POLICY_PACK_IMPORT_LINE: &str = "ax policy pack import --quiet";

fn hook_lines(name: &str, memory_sync: bool, policy_sync: bool) -> Vec<&'static str> {
    let mut lines: Vec<&'static str> = match name {
        "post-commit" => vec![SYNC_LINE, SHIP_LINE, CAPTURE_COMMIT_LINE],
        "post-merge" => vec![SYNC_LINE, SHIP_LINE, CAPTURE_MERGE_LINE],
        "post-checkout" => vec![SYNC_LINE, SHIP_LINE],
        _ => vec![SYNC_LINE, SHIP_LINE],
    };
    if memory_sync {
        match name {
            "post-commit" => lines.push(MEMORY_EXPORT_LINE),
            "post-merge" => lines.push(MEMORY_IMPORT_LINE),
            _ => {}
        }
    }
    if policy_sync {
        match name {
            "post-commit" => lines.push(POLICY_PACK_EXPORT_LINE),
            "post-merge" => lines.push(POLICY_PACK_IMPORT_LINE),
            _ => {}
        }
    }
    lines
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
    let memory_sync = root_bool_flag(project_root, "memorySync");
    let policy_sync = root_bool_flag(project_root, "policySync");
    for name in ["post-commit", "post-merge", "post-checkout"] {
        let hook_path = hooks_dir.join(name);
        let required = hook_lines(name, memory_sync, policy_sync);
        let content = if hook_path.exists() {
            let existing = fs::read_to_string(hook_path.display().to_string()).unwrap_or_default();
            if required.iter().all(|line| existing.contains(line)) {
                continue;
            }
            merge_hook_content(&existing, &required)
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

fn root_bool_flag(project_root: &Path, key: &str) -> bool {
    for name in ["ax.json", ".ax.json"] {
        let path = project_root.join(name);
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if v.get(key).and_then(|x| x.as_bool()) == Some(true) {
                return true;
            }
        }
    }
    false
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
                        && !l.contains("ax memory export")
                        && !l.contains("ax memory import")
                        && !l.contains("ax policy pack export")
                        && !l.contains("ax policy pack import")
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
        let lines = hook_lines("post-commit", false, false);
        assert!(lines.contains(&CAPTURE_COMMIT_LINE));
    }

    #[test]
    fn post_checkout_skips_capture_git() {
        let lines = hook_lines("post-checkout", false, false);
        assert!(!lines.iter().any(|l| l.contains("capture-git")));
    }

    #[test]
    fn memory_sync_adds_export_on_commit() {
        let lines = hook_lines("post-commit", true, false);
        assert!(lines.contains(&MEMORY_EXPORT_LINE));
        let merge = hook_lines("post-merge", true, false);
        assert!(merge.contains(&MEMORY_IMPORT_LINE));
    }

    #[test]
    fn policy_sync_adds_pack_on_commit_and_merge() {
        let lines = hook_lines("post-commit", false, true);
        assert!(lines.contains(&POLICY_PACK_EXPORT_LINE));
        let merge = hook_lines("post-merge", false, true);
        assert!(merge.contains(&POLICY_PACK_IMPORT_LINE));
    }

    #[test]
    fn merge_adds_missing_lines() {
        let lines = hook_lines("post-commit", false, false);
        let merged = merge_hook_content("ax sync --quiet\n", &lines);
        assert!(merged.contains(SHIP_LINE));
        assert!(merged.contains(CAPTURE_COMMIT_LINE));
    }
}
