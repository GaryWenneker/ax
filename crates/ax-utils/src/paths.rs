//! Path validation utilities.

use std::path::{Component, Path, PathBuf};

use crate::errors::{AxError, FileError};

/// Ensure a path resolves within the project root (no traversal escape).
pub fn validate_path_within_root(root: &Path, candidate: &Path) -> Result<PathBuf, AxError> {
    let root = root
        .canonicalize()
        .map_err(|e| AxError::File(FileError::with_path(e.to_string(), root.to_string_lossy())))?;
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        root.join(candidate)
    };
    let canonical = joined.canonicalize().map_err(|e| {
        AxError::File(FileError::with_path(
            e.to_string(),
            candidate.to_string_lossy(),
        ))
    })?;

    if !canonical.starts_with(&root) {
        return Err(AxError::File(FileError::new("path escapes project root")));
    }

    for component in candidate.components() {
        if matches!(component, Component::ParentDir) {
            return Err(AxError::File(FileError::new(
                "path contains parent directory traversal",
            )));
        }
    }

    Ok(canonical)
}

/// Env override for the machine-wide `global.db` (tests and custom installs).
pub const AX_GLOBAL_DB_ENV: &str = "AX_GLOBAL_DB";

/// The user's home directory. `AX_HOME_DIR` overrides it: on Windows `dirs::home_dir()`
/// ignores `HOME` and `USERPROFILE`, so tests need their own way to point ax at a temp dir.
pub fn home_dir() -> Option<PathBuf> {
    match std::env::var_os("AX_HOME_DIR") {
        Some(dir) if !dir.is_empty() => Some(PathBuf::from(dir)),
        _ => dirs::home_dir(),
    }
}

/// `AX_GLOBAL_DB` when set and non-empty, else `<home>/.ax/global.db`.
pub fn resolve_global_db_path(home: Option<PathBuf>) -> Option<PathBuf> {
    if let Ok(p) = std::env::var(AX_GLOBAL_DB_ENV) {
        if !p.is_empty() {
            return Some(PathBuf::from(p));
        }
    }
    home.map(|h| h.join(".ax").join("global.db"))
}

/// Directory names whose contents are tests.
const TEST_DIRS: [&str; 3] = ["test", "tests", "__tests__"];

/// Whether `path` (relative or absolute, `/` or `\` separated) names a test
/// file: anything under a `test/`, `tests/` or `__tests__/` directory, a file
/// named `*_test.*`, `*.test.*` or `*.spec.*`, or a Python `test_*.py`.
pub fn is_test_path(path: &str) -> bool {
    let lower = path.replace('\\', "/").to_lowercase();
    let mut parts: Vec<&str> = lower.split('/').filter(|p| !p.is_empty()).collect();
    let Some(name) = parts.pop() else {
        return false;
    };
    if parts.iter().any(|dir| TEST_DIRS.contains(dir)) {
        return true;
    }
    name.contains("_test.")
        || name.contains(".test.")
        || name.contains(".spec.")
        || (name.starts_with("test_") && name.ends_with(".py"))
}

#[cfg(test)]
mod test_path_tests {
    use super::is_test_path;

    #[test]
    fn recognises_test_directories_and_names() {
        assert!(is_test_path("tests/cli.rs"));
        assert!(is_test_path("crates/ax-db/tests/integrity.rs"));
        assert!(is_test_path("/abs/repo/test/x.c"));
        assert!(is_test_path("src\\__tests__\\a.tsx"));
        assert!(is_test_path("a/b.spec.ts"));
        assert!(is_test_path("a/b.test.js"));
        assert!(is_test_path("a/b_test.go"));
        assert!(is_test_path("pkg/test_x.py"));
        assert!(!is_test_path("src/testing.rs"));
        assert!(!is_test_path("src/test_helpers.rs"));
        assert!(!is_test_path("src/latest/mod.rs"));
        assert!(!is_test_path("tests"));
        assert!(!is_test_path(""));
    }
}
