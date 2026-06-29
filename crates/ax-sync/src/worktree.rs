//! Git worktree detection.

use std::path::Path;

pub fn detect_worktree_index_mismatch(start_path: &Path, resolved_root: &Path) -> Option<String> {
    let start = start_path.canonicalize().ok();
    let resolved = resolved_root.canonicalize().ok();
    if let (Some(s), Some(r)) = (start, resolved) {
        if s != r {
            return Some(format!("worktree path {} differs from index root {}", s.display(), r.display()));
        }
    }
    None
}

pub fn worktree_mismatch_warning(msg: &str) -> String {
    format!("Warning: {}", msg)
}
