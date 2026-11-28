//! One policy writer at a time per project.
//!
//! An index pass reads the policy files and then writes (and prunes) the
//! rows; a save writes a file and then its row. Run side by side (the
//! Command Center's background watcher indexes while a user saves), the pass
//! can prune the new rule or put an older body back. Every policy row writer
//! takes this lock: the index pass, the database-mode refresh in
//! `ensure_policy_ready`, saves, deletes and enable/disable (which reads and
//! writes back under one hold). The lock is not reentrant, so a holder calls
//! the `*_locked` helpers. It covers writers in this process only.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::sync::OwnedMutexGuard;

fn locks() -> &'static Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>> {
    static LOCKS: std::sync::OnceLock<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>> =
        std::sync::OnceLock::new();
    LOCKS.get_or_init(Default::default)
}

/// Waits for, then holds, the policy write lock of `project_root`.
pub(crate) async fn policy_write_lock(project_root: &Path) -> OwnedMutexGuard<()> {
    let key = std::fs::canonicalize(project_root).unwrap_or_else(|_| project_root.to_path_buf());
    let lock = locks()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(key)
        .or_default()
        .clone();
    lock.lock_owned().await
}
