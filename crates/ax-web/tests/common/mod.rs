//! Setup shared by the ax-web integration tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use tempfile::TempDir;

static HOME: OnceLock<TempDir> = OnceLock::new();

/// Points `HOME`, `AX_HOME_DIR` and `AX_GLOBAL_DB` at one temporary folder
/// for the whole test binary, so no test reads or writes the machine's
/// `~/.ax` or its `global.db`. `AX_GLOBAL_DB` wins over `HOME` when the
/// global database path is resolved, so it must be set too.
pub fn isolate_home() -> &'static Path {
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        isolate_home_at(dir.path());
        dir
    })
    .path()
}

/// [`isolate_home`] for a folder the caller owns.
pub fn isolate_home_at(dir: &Path) {
    std::env::set_var("HOME", dir);
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_GLOBAL_DB", global_db(dir));
}

/// The global database path inside an isolated home.
pub fn global_db(home: &Path) -> PathBuf {
    home.join("global.db")
}
