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

/// Env override for the machine-wide `global.db` (tests and custom installs).
pub const AX_GLOBAL_DB_ENV: &str = "AX_GLOBAL_DB";

/// `AX_GLOBAL_DB` when set and non-empty, else `<home>/.ax/global.db`.
pub fn resolve_global_db_path(home: Option<PathBuf>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var(AX_GLOBAL_DB_ENV) {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    home.map(|h| h.join(".ax").join("global.db"))
}
