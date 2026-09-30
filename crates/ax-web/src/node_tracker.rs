//! Which graph nodes are truly new (docs/specs/live-updates.md, revision 3).
//!
//! A re-index deletes and re-inserts a file's nodes, so `updated_at` cannot
//! tell a new node from a rewritten one. The tracker keeps the id set seen at
//! the last snapshot and records ids that were absent from it.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use sqlx::SqlitePool;

pub const CREATED_CAP: usize = 1000;

#[derive(Default)]
pub struct NodeTracker {
    known: Option<HashSet<String>>,
    created: VecDeque<(i64, String)>,
}

impl NodeTracker {
    /// Compare `ids` with the previous snapshot; ids absent from it are
    /// recorded as created at `now_ms`. The first snapshot records nothing.
    pub fn observe(&mut self, ids: HashSet<String>, now_ms: i64) {
        if let Some(known) = &self.known {
            for id in ids.difference(known) {
                self.created.push_back((now_ms, id.clone()));
            }
            while self.created.len() > CREATED_CAP {
                self.created.pop_front();
            }
        }
        self.known = Some(ids);
    }

    /// Created ids after `since_ms`, newest first, at most `limit`.
    pub fn created_since(&self, since_ms: i64, limit: usize) -> Vec<String> {
        self.created
            .iter()
            .rev()
            .filter(|(at, _)| *at > since_ms)
            .map(|(_, id)| id.clone())
            .take(limit)
            .collect()
    }
}

fn trackers() -> &'static Mutex<HashMap<PathBuf, NodeTracker>> {
    static TRACKERS: OnceLock<Mutex<HashMap<PathBuf, NodeTracker>>> = OnceLock::new();
    TRACKERS.get_or_init(Default::default)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default()
}

/// Snapshot the node ids of the graph at `db_path` into its tracker.
pub async fn observe(pool: &SqlitePool, db_path: &Path) -> anyhow::Result<()> {
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM nodes").fetch_all(pool).await?;
    let mut all = trackers().lock().unwrap_or_else(|e| e.into_inner());
    all.entry(db_path.to_path_buf())
        .or_default()
        .observe(ids.into_iter().collect(), now_ms());
    Ok(())
}

/// Created ids for the graph at `db_path` after `since_ms`, newest first.
pub fn created_since(db_path: &Path, since_ms: i64, limit: usize) -> Vec<String> {
    let all = trackers().lock().unwrap_or_else(|e| e.into_inner());
    all.get(db_path)
        .map(|t| t.created_since(since_ms, limit))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_first_snapshot_records_nothing() {
        let mut t = NodeTracker::default();
        t.observe(set(&["a", "b"]), 10);
        assert!(t.created_since(0, 10).is_empty());
    }

    #[test]
    fn an_id_absent_from_the_last_snapshot_is_created() {
        let mut t = NodeTracker::default();
        t.observe(set(&["a"]), 10);
        t.observe(set(&["a", "b"]), 20);
        t.observe(set(&["a", "b", "c"]), 30);
        assert_eq!(t.created_since(0, 10), vec!["c", "b"]);
        assert_eq!(t.created_since(20, 10), vec!["c"]);
        assert_eq!(t.created_since(0, 1), vec!["c"]);
    }

    #[test]
    fn an_id_that_was_already_known_is_not_created() {
        let mut t = NodeTracker::default();
        t.observe(set(&["a", "b"]), 10);
        t.observe(set(&["a", "b"]), 20);
        assert!(t.created_since(0, 10).is_empty());
    }

    #[test]
    fn the_created_buffer_keeps_the_newest_entries_up_to_the_cap() {
        let mut t = NodeTracker::default();
        t.observe(HashSet::new(), 0);
        let ids: HashSet<String> = (0..CREATED_CAP + 5).map(|i| format!("n{i}")).collect();
        t.observe(ids, 1);
        assert_eq!(t.created_since(0, usize::MAX).len(), CREATED_CAP);
    }
}
