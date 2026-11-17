use std::path::{Component, Path, PathBuf};

use ax_utils::errors::AxError;

/// Paths that do not stay inside `project_root`. Relative paths are allowed.
pub fn scope_paths(project_root: &Path, files: &[String]) -> Vec<String> {
    files
        .iter()
        .filter(|path| !path_is_in_project(project_root, path))
        .cloned()
        .collect()
}

pub fn assert_scope(project_root: &Path, files: &[String]) -> Result<(), AxError> {
    let foreign = scope_paths(project_root, files);
    if foreign.is_empty() {
        return Ok(());
    }
    Err(AxError::Other(format!(
        "foreign memory path {}. Use projectPath={}",
        foreign.join(", "),
        project_root.display()
    )))
}

fn path_is_in_project(project_root: &Path, path: &str) -> bool {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return true;
    }
    let candidate = PathBuf::from(trimmed);
    if candidate.is_absolute() {
        return candidate.starts_with(project_root);
    }
    !candidate
        .components()
        .any(|part| matches!(part, Component::ParentDir))
}
