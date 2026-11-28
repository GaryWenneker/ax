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
    // `..` can leave the project from a relative or an absolute path alike.
    if candidate
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return false;
    }
    // A leading slash is absolute on Unix and a drive-root path on Windows.
    // Either way it is outside the project unless it sits under the root.
    if candidate.is_absolute() || trimmed.starts_with('/') || trimmed.starts_with('\\') {
        return candidate.starts_with(project_root);
    }
    // A Windows drive-relative path (`C:foo`) has a prefix but is not absolute.
    !candidate
        .components()
        .any(|part| matches!(part, Component::Prefix(_)))
}
