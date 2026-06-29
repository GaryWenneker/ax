//! Git hook installer for ax sync.

use std::fs;
use std::path::Path;

use ax_utils::errors::{AxError, FileError};

const HOOK_SCRIPT: &str = "ax sync --quiet\n";

pub fn install_git_sync_hooks(project_root: &Path) -> Result<(), AxError> {
    let hooks_dir = project_root.join(".git").join("hooks");
    if !hooks_dir.exists() {
        return Ok(());
    }
    for name in ["post-commit", "post-merge", "post-checkout"] {
        let hook_path = hooks_dir.join(name);
        let content = if hook_path.exists() {
            let existing = fs::read_to_string(hook_path.display().to_string()).unwrap_or_default();
            if existing.contains("ax sync") {
                continue;
            }
            format!("{}\n{}", existing, HOOK_SCRIPT)
        } else {
            HOOK_SCRIPT.to_string()
        };
        fs::write(hook_path.display().to_string(), content).map_err(|e| AxError::File(FileError::with_path(e.to_string(), hook_path.display().to_string())))?;
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
                .filter(|l| !l.contains("ax sync"))
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(hook_path.display().to_string(), filtered).map_err(|e| AxError::File(FileError::with_path(e.to_string(), hook_path.display().to_string())))?;
        }
    }
    Ok(())
}
