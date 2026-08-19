//! Process-global pending-file registry.
//!
//! The MCP tool-serving `Ax` and the background watcher run as separate `Ax`
//! instances. This registry lets MCP responses (staleness banners, status,
//! preflight) see files the watcher has queued even when the tool `Ax` has no
//! local `FileWatcher`.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

use ax_types::PendingFile;

fn registry() -> &'static Mutex<HashMap<String, HashMap<String, PendingFile>>> {
    static REG: OnceLock<Mutex<HashMap<String, HashMap<String, PendingFile>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Stable key for a project root (forward slashes, absolute when possible).
pub fn project_key(project_root: &Path) -> String {
    let abs = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    abs.to_string_lossy().replace('\\', "/")
}

/// Replace the pending set for a project (called after every watcher mutation).
pub fn publish(project_root: &Path, pending: &[PendingFile]) {
    let key = project_key(project_root);
    let mut map = HashMap::with_capacity(pending.len());
    for p in pending {
        map.insert(p.path.clone(), p.clone());
    }
    if let Ok(mut guard) = registry().lock() {
        if map.is_empty() {
            guard.remove(&key);
        } else {
            guard.insert(key, map);
        }
    }
}

/// Clear all pending entries for a project (watcher stop).
pub fn clear_project(project_root: &Path) {
    let key = project_key(project_root);
    if let Ok(mut guard) = registry().lock() {
        guard.remove(&key);
    }
}

/// Snapshot of pending files for a project (any Ax / MCP reader).
pub fn pending_files(project_root: &Path) -> Vec<PendingFile> {
    let key = project_key(project_root);
    registry()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&key).cloned())
        .map(|m| m.into_values().collect())
        .unwrap_or_default()
}

/// Test helper: inject pending files without a live watcher.
#[cfg(test)]
pub fn publish_for_test(project_root: &Path, pending: Vec<PendingFile>) {
    publish(project_root, &pending);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn publish_and_read_roundtrip() {
        let root = PathBuf::from("/tmp/ax-pending-test-root");
        clear_project(&root);
        publish(
            &root,
            &[PendingFile {
                path: "src/a.rs".into(),
                first_seen_ms: 1,
                last_seen_ms: 2,
                indexing: false,
            }],
        );
        let got = pending_files(&root);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "src/a.rs");
        clear_project(&root);
        assert!(pending_files(&root).is_empty());
    }
}
