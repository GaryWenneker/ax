//! `get_stats` reports the real database size.
//!
//! `db_size_bytes` used to be hard-coded to 0, so `ax_status` and preflight
//! showed an empty database while `.ax/ax.db` was hundreds of megabytes.
//!
//! Run: cargo test -p ax-db --test stats_db_size

use std::path::{Path, PathBuf};

use ax_db::queries::QueryBuilder;
use ax_db::Database;

fn scratch_db(name: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "ax-stats-db-size-{name}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&p);
    p
}

fn cleanup(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.to_path_buf();
        if !suffix.is_empty() {
            p.set_file_name(format!(
                "{}{suffix}",
                path.file_name().unwrap().to_string_lossy()
            ));
        }
        let _ = std::fs::remove_file(&p);
    }
}

#[tokio::test]
async fn stats_report_nonzero_db_size_matching_sqlite_pages() {
    let path = scratch_db("nonzero");
    let db = Database::open(&path).await.expect("open db");
    let queries = QueryBuilder::new(db.pool().clone());

    let stats = queries.get_stats().await.expect("stats");

    let page_count: i64 = sqlx::query_scalar("PRAGMA page_count")
        .fetch_one(db.pool())
        .await
        .expect("page_count");
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(db.pool())
        .await
        .expect("page_size");

    assert!(stats.db_size_bytes > 0, "migrated schema has pages");
    assert_eq!(stats.db_size_bytes, page_count * page_size);

    drop(queries);
    drop(db);
    cleanup(&path);
}
