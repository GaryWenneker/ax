//! Path validation utilities.

use std::path::{Component, Path, PathBuf};

use crate::errors::{AxError, FileError};

/// Ensure a path resolves within the project root (no traversal escape).
pub fn validate_path_within_root(root: &Path, candidate: &Path) -> Result<PathBuf, AxError> {
    let root = root.canonicalize().map_err(|e| {
        AxError::File(FileError::with_path(e.to_string(), root.to_string_lossy()))
    })?;
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let canonical = joined
        .canonicalize()
        .map_err(|e| AxError::File(FileError::with_path(e.to_string(), candidate.to_string_lossy())))?;

    if !canonical.starts_with(&root) {
        return Err(AxError::File(FileError::new("path escapes project root")));
    }

    for component in candidate.components() {
        if matches!(component, Component::ParentDir) {
            return Err(AxError::File(FileError::new("path contains parent directory traversal")));
        }
    }

    Ok(canonical)
}