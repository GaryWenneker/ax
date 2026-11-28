//! `needs_full_rebuild` / `last_full_sync` metadata (perf-08).

use std::path::PathBuf;

use ax_db::sync_strategy::needs_full_rebuild_from_timestamp;
use ax_db::{queries::QueryBuilder, sync_strategy::LAST_FULL_SYNC_METADATA_KEY, Database};

fn scratch_db(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ax-sync-strategy-{name}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn needs_full_rebuild_false_when_stamp_is_fresh() {
    let now = 1_700_000_000_000i64;
    assert!(!needs_full_rebuild_from_timestamp(Some(now), now));
}

#[test]
fn needs_full_rebuild_true_when_stamp_missing_or_stale() {
    let now = 1_700_000_000_000i64;
    assert!(needs_full_rebuild_from_timestamp(None, now));
    let week_ms = 7 * 24 * 60 * 60 * 1000;
    assert!(needs_full_rebuild_from_timestamp(
        Some(now - week_ms - 1),
        now
    ));
}

#[tokio::test]
async fn needs_full_rebuild_reads_last_full_sync_metadata() {
    let path = scratch_db("metadata");
    let db = Database::open(&path).await.expect("open db");
    let queries = QueryBuilder::new(db.pool().clone());

    assert!(
        queries.needs_full_rebuild().await.expect("query"),
        "missing last_full_sync must request full rebuild"
    );

    let fresh = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    queries
        .set_metadata(LAST_FULL_SYNC_METADATA_KEY, &fresh.to_string())
        .await
        .expect("write metadata");

    assert!(
        !queries.needs_full_rebuild().await.expect("query"),
        "recent last_full_sync must allow incremental sync"
    );

    let _ = std::fs::remove_file(path);
}
