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
    /// How many id sets were loaded.
    loads: u64,
    /// The graph fingerprint at the last snapshot.
    fingerprint: Option<String>,
}

impl NodeTracker {
    /// Compare `ids` with the previous snapshot; ids absent from it are
    /// recorded as created at `now_ms`. The first snapshot records nothing.
    pub fn observe(&mut self, ids: HashSet<String>, now_ms: i64) {
        self.loads += 1;
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

/// Cheap fingerprint of the node table; it changes whenever a node is added,
/// removed or re-indexed.
const FINGERPRINT_SQL: &str = "SELECT count(*) || ':' || coalesce(max(rowid), 0) || ':' || \
     coalesce(max(updated_at), '') FROM nodes";

/// One snapshot at a time, so an older id set can never be applied after a
/// newer one (which would record the same nodes as created twice).
fn snapshot_lock() -> &'static tokio::sync::Mutex<()> {
    static LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(Default::default)
}

/// Snapshot the node ids of the graph at `db_path` into its tracker. The id
/// set is only loaded when the node table changed since the last snapshot.
pub async fn observe(pool: &SqlitePool, db_path: &Path) -> anyhow::Result<()> {
    let _one_at_a_time = snapshot_lock().lock().await;
    let fingerprint: String = sqlx::query_scalar(FINGERPRINT_SQL).fetch_one(pool).await?;
    {
        let all = trackers().lock().unwrap_or_else(|e| e.into_inner());
        if all
            .get(db_path)
            .is_some_and(|t| t.fingerprint.as_deref() == Some(fingerprint.as_str()))
        {
            return Ok(());
        }
    }
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM nodes")
        .fetch_all(pool)
        .await?;
    let mut all = trackers().lock().unwrap_or_else(|e| e.into_inner());
    let tracker = all.entry(db_path.to_path_buf()).or_default();
    tracker.observe(ids.into_iter().collect(), now_ms());
    tracker.fingerprint = Some(fingerprint);
    Ok(())
}

/// Forget the tracker of the graph at `db_path` (its workspace was closed).
pub fn forget(db_path: &Path) {
    let mut all = trackers().lock().unwrap_or_else(|e| e.into_inner());
    all.remove(db_path);
}

#[cfg(test)]
fn loads(db_path: &Path) -> Option<u64> {
    let all = trackers().lock().unwrap_or_else(|e| e.into_inner());
    all.get(db_path).map(|t| t.loads)
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

    async fn graph(ids: &[&str]) -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("CREATE TABLE nodes (id TEXT PRIMARY KEY, updated_at TEXT)")
            .execute(&pool)
            .await
            .unwrap();
        for id in ids {
            add_node(&pool, id).await;
        }
        pool
    }

    async fn add_node(pool: &SqlitePool, id: &str) {
        sqlx::query("INSERT INTO nodes (id, updated_at) VALUES (?, '1')")
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn an_unchanged_graph_is_not_loaded_again() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        let pool = graph(&["a", "b"]).await;
        observe(&pool, &db).await.unwrap();
        observe(&pool, &db).await.unwrap();
        assert_eq!(loads(&db), Some(1));
        add_node(&pool, "c").await;
        observe(&pool, &db).await.unwrap();
        assert_eq!(loads(&db), Some(2));
        assert_eq!(created_since(&db, 0, 10), vec!["c"]);
    }

    #[tokio::test]
    async fn a_node_that_replaces_the_newest_row_is_still_seen() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        let pool = graph(&["a", "b"]).await;
        observe(&pool, &db).await.unwrap();
        sqlx::query("DELETE FROM nodes WHERE id = 'b'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO nodes (id, updated_at) VALUES ('c', '2')")
            .execute(&pool)
            .await
            .unwrap();
        observe(&pool, &db).await.unwrap();
        assert_eq!(created_since(&db, 0, 10), vec!["c"]);
    }

    #[tokio::test]
    async fn concurrent_snapshots_record_a_new_node_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        let pool = graph(&["a"]).await;
        observe(&pool, &db).await.unwrap();
        add_node(&pool, "b").await;
        let runs = (0..16).map(|_| observe(&pool, &db));
        for r in futures_util::future::join_all(runs).await {
            r.unwrap();
        }
        assert_eq!(created_since(&db, 0, 10), vec!["b"]);
    }

    #[tokio::test]
    async fn a_closed_workspace_drops_its_tracker() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("ax.db");
        let pool = graph(&["a"]).await;
        observe(&pool, &db).await.unwrap();
        assert!(loads(&db).is_some());
        forget(&db);
        assert_eq!(loads(&db), None);
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
