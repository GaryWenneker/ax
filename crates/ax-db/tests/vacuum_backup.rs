use std::path::PathBuf;

use ax_db::integrity::{repair_database, RepairOptions};
use ax_db::Database;

const NEEDLE: &[u8] = b"NEEDLE-VACUUM-UNIQUE";

#[tokio::test]
async fn repair_backup_drops_a_deleted_page_that_a_byte_copy_keeps() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(".ax").join("ax.db");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = Database::open(&path).await.unwrap();
    sqlx::query(
        "INSERT INTO memories (id, title, body, created_at, updated_at) VALUES ('m1', 'Kept title', 'body', 1, 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memories (id, title, body, created_at, updated_at) VALUES ('gone', 'Gone', 'NEEDLE-VACUUM-UNIQUE', 1, 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("DELETE FROM memories WHERE id = 'gone'")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO memory_cues (cue, memory_id, weight, source) VALUES ('lost', 'missing-id', 1, 'test')",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(db.pool())
        .await
        .unwrap();
    db.close().await;

    let source = std::fs::read(&path).unwrap();
    assert!(
        source.windows(NEEDLE.len()).any(|window| window == NEEDLE),
        "the deleted row must still sit in a free page"
    );

    let repair = repair_database(&path, &RepairOptions::default()).await;
    assert!(repair.ok, "{:?}", repair.remaining);
    let backup = PathBuf::from(repair.backup.expect("backup"));
    let sidecar = PathBuf::from(format!("{}-wal", backup.display()));
    assert!(!sidecar.exists());
    let copied = std::fs::read(&backup).unwrap();
    assert!(
        !copied.windows(NEEDLE.len()).any(|window| window == NEEDLE),
        "VACUUM INTO drops the deleted page"
    );
    assert!(std::fs::read(&path)
        .unwrap()
        .windows(NEEDLE.len())
        .any(|w| w == NEEDLE));

    let opened = Database::open(&backup).await.unwrap();
    let kept: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE id = 'm1'")
        .fetch_one(opened.pool())
        .await
        .unwrap();
    let cue: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_cues WHERE memory_id = 'missing-id'")
            .fetch_one(opened.pool())
            .await
            .unwrap();
    opened.close().await;
    assert_eq!(kept, 1);
    assert_eq!(cue, 1);
}
