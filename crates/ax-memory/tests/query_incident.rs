//! A memories table damaged after open makes reads fail as a recorded incident.

use std::path::PathBuf;

use ax_db::integrity::{sql_error_is_incident, writers_blocked};
use ax_db::Database;

/// Creates a database and overwrites the first byte of every `memories` b-tree root page with
/// an invalid page type. Fails unless at least one page inside the file was damaged.
async fn damaged_db() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("ax.db");
    let db = Database::open(&path).await.unwrap();
    let pages: Vec<i64> = sqlx::query_scalar(
        "SELECT rootpage FROM sqlite_master WHERE tbl_name = 'memories' AND rootpage > 1",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    let page_size: i64 = sqlx::query_scalar("PRAGMA page_size")
        .fetch_one(db.pool())
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(db.pool())
        .await
        .unwrap();
    db.close().await;
    assert!(!pages.is_empty(), "memories has no page to damage");
    let mut bytes = std::fs::read(&path).unwrap();
    for rootpage in pages {
        let offset = ((rootpage - 1) * page_size) as usize;
        assert!(offset < bytes.len(), "memories page is outside the file");
        bytes[offset] = 0x03;
    }
    std::fs::write(&path, &bytes).unwrap();
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    (root, path)
}

#[tokio::test]
async fn a_memory_read_after_open_records_a_malformed_error() {
    let (_root, path) = damaged_db().await;
    let db = Database::open(&path)
        .await
        .expect("open must survive a damaged memories page");
    let err = ax_memory::store::get(db.pool(), "missing")
        .await
        .expect_err("a corrupt memories table must fail the read");
    assert!(sql_error_is_incident(&err.to_string()), "{err}");
    assert!(writers_blocked(&path), "{err}");
    db.close().await;
}

#[tokio::test]
async fn a_brain_query_after_open_records_a_malformed_error() {
    let (_root, path) = damaged_db().await;
    let db = Database::open(&path)
        .await
        .expect("open must survive a damaged memories page");
    let err = ax_memory::brain::is_injectable(db.pool(), "missing")
        .await
        .expect_err("a corrupt memories table must fail the brain query");
    assert!(sql_error_is_incident(&err.to_string()), "{err}");
    assert!(writers_blocked(&path), "{err}");
    db.close().await;
}
