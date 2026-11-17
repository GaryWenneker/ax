use std::path::PathBuf;

use ax_db::integrity::{check_database, repair_database, CheckOptions, RepairOptions};
use ax_db::Database;

fn temp_db(name: &str) -> (PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "ax-vector-repair-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = root.join(".ax").join("ax.db");
    (root, db)
}

#[tokio::test]
async fn reembed_stale_rewrites_a_foreign_model() {
    let (root, path) = temp_db("reembed");
    let db = Database::open(&path).await.unwrap();
    sqlx::query(
        "INSERT INTO memories (id, title, body, tags, tier, embedding_model, created_at, updated_at)
         VALUES ('m1', 'Tokenizer', 'use tiktoken', '[]', 't2', 'other-model', 1, 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let updated = ax_memory::reembed_stale(db.pool(), 10).await.unwrap();
    assert_eq!(updated, 1);
    let model: String = sqlx::query_scalar("SELECT embedding_model FROM memories WHERE id = 'm1'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    let (_, active) = ax_memory::embed::embed_labeled("ax");
    assert_eq!(model, active);
    db.close().await;
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn one_pass_rewrites_every_foreign_model_and_leaves_the_hash_row() {
    let (root, path) = temp_db("reembed-all");
    let db = Database::open(&path).await.unwrap();
    sqlx::query(
        "INSERT INTO memories (id, title, body, tags, tier, embedding_model, enabled, created_at, updated_at)
         VALUES
         ('keep', 'Already hashed', 'stays', '[]', 't2', 'hash-v1', 1, 1, 1),
         ('hot', 'Foreign hot', 'minilm row', '[]', 't2', 'minilm', 1, 1, 1),
         ('off', 'Foreign off', 'disabled row', '[]', 't2', 'minilm-256', 0, 1, 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    let updated = ax_memory::reembed_all_foreign(db.pool()).await.unwrap();
    assert_eq!(updated, 2);
    let models: Vec<(String, Option<Vec<u8>>)> =
        sqlx::query_as("SELECT id, embedding FROM memories ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap();
    let by_id: std::collections::BTreeMap<_, _> = models.into_iter().collect();
    assert!(by_id["keep"].is_none());
    assert_eq!(by_id["hot"].as_ref().map(Vec::len), Some(1024));
    assert_eq!(by_id["off"].as_ref().map(Vec::len), Some(1024));
    let names: Vec<(String, String)> = sqlx::query_as("SELECT id, embedding_model FROM memories")
        .fetch_all(db.pool())
        .await
        .unwrap();
    for (_id, model) in names {
        assert_eq!(model, "hash-v1");
    }
    db.close().await;
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn rebuild_vector_index_replaces_a_dangling_map_after_backup() {
    let (root, path) = temp_db("rebuild");
    let db = Database::open(&path).await.unwrap();
    let (vector, model) = ax_memory::embed::embed_labeled("Tokenizer use tiktoken");
    sqlx::query(
        "INSERT INTO memories (id, title, body, tags, tier, embedding, embedding_model, created_at, updated_at)
         VALUES ('m1', 'Tokenizer', 'use tiktoken', '[]', 't3', ?, ?, 1, 1)",
    )
    .bind(ax_memory::embed::embedding_to_blob(&vector))
    .bind(model)
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(embedding float[256])")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS memory_vec_map (memory_id TEXT PRIMARY KEY, rowid INTEGER NOT NULL)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO memory_vec_map (memory_id, rowid) VALUES ('m1', 999)")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(db.pool())
        .await
        .unwrap();
    db.close().await;

    let report = check_database(
        &path,
        &CheckOptions {
            quick: false,
            embedding_model: Some(model.to_string()),
        },
    )
    .await;
    assert!(
        report
            .findings
            .iter()
            .any(|finding| finding.code == "vector_dangling_row"),
        "{:?}",
        report.findings
    );
    let repair = repair_database(
        &path,
        &RepairOptions {
            embedding_model: Some(model.to_string()),
            ..RepairOptions::default()
        },
    )
    .await;
    assert!(repair.proceed, "{:?}", repair.remaining);
    let backup = repair.backup.expect("backup before rewrite");
    assert!(std::path::Path::new(&backup).is_file());

    let db = Database::open(&path).await.unwrap();
    let indexed = ax_memory::rebuild_vector_index(db.pool()).await.unwrap();
    assert_eq!(indexed, 1);
    let rowid: i64 = sqlx::query_scalar("SELECT rowid FROM memory_vec_map WHERE memory_id = 'm1'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    let present: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_vec WHERE rowid = ?")
        .bind(rowid)
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(present, 1);
    db.close().await;
    let _ = std::fs::remove_dir_all(root);
}
