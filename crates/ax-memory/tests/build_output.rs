use ax_db::Database;
use ax_memory::{capture_graph_episodes, get, search, Depth, SearchRequest};

async fn open() -> (tempfile::TempDir, Database) {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("ax.db")).await.unwrap();
    (dir, db)
}

#[tokio::test]
async fn migration_34_demotes_build_output_and_keeps_it_fetchable() {
    let (dir, db) = open().await;
    let path = dir.path().join("ax.db");
    sqlx::query(
        "INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, status, tier, enabled)
         VALUES ('noise-row', 'graph_diff', 'changed function built', ?, '[]', ?, 1.0, 'graph-sync', 1, 1, 'active', 't3', 1)",
    )
    .bind("changed symbol built (function) in target-dev/debug/built.rs. marker zzbuildnoise.")
    .bind(serde_json::json!(["target-dev/debug/built.rs"]).to_string())
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, status, tier, enabled)
         VALUES ('source-row', 'graph_diff', 'changed function real', ?, '[]', ?, 1.0, 'graph-sync', 1, 1, 'active', 't3', 1)",
    )
    .bind("changed symbol real (function) in crates/ax-memory/src/brain.rs.")
    .bind(serde_json::json!(["crates/ax-memory/src/brain.rs"]).to_string())
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, status, tier, enabled)
         VALUES ('near-row', 'note', 'target design', 'The target-design note stays.', '[]', ?, 1.0, 'manual', 1, 1, 'active', 't2', 1)",
    )
    .bind(serde_json::json!(["target-design/note.rs"]).to_string())
    .execute(db.pool())
    .await
    .unwrap();
    sqlx::query("DELETE FROM schema_versions WHERE version >= 34")
        .execute(db.pool())
        .await
        .unwrap();
    db.close().await;

    let db = Database::open(&path).await.unwrap();
    let noise: (String, String, i64) =
        sqlx::query_as("SELECT status, tier, enabled FROM memories WHERE id = 'noise-row'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let source: (String, i64) =
        sqlx::query_as("SELECT status, enabled FROM memories WHERE id = 'source-row'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let near: String = sqlx::query_scalar("SELECT status FROM memories WHERE id = 'near-row'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(noise, ("noise".into(), "t3".into(), 0));
    assert_eq!(source, ("active".into(), 1));
    assert_eq!(near, "active");

    let found = search(
        db.pool(),
        &SearchRequest {
            query: "zzbuildnoise".into(),
            cue: None,
            id: None,
            depth: Depth::Archive,
            files: vec![],
            limit: 10,
        },
    )
    .await
    .unwrap();
    assert!(
        found.iter().all(|hit| hit.memory.id != "noise-row"),
        "archive search returned the build-output row"
    );
    assert!(get(db.pool(), "noise-row").await.unwrap().is_some());
    db.close().await;
}

#[tokio::test]
async fn graph_capture_skips_build_output() {
    let (_dir, db) = open().await;
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, updated_at)
         VALUES ('built', 'function', 'built', 'built', 'target-dev/debug/built.rs', 'rust', 1, 2, 0, 1, 1),
                ('real', 'function', 'real', 'real', 'crates/ax-memory/src/brain.rs', 'rust', 1, 2, 0, 1, 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    assert_eq!(capture_graph_episodes(db.pool()).await.unwrap(), 0);
    // A signature change (line moves alone are not a change).
    sqlx::query("UPDATE nodes SET signature = 'fn changed()'")
        .execute(db.pool())
        .await
        .unwrap();
    capture_graph_episodes(db.pool()).await.unwrap();
    sqlx::query("DELETE FROM nodes WHERE id = 'built'")
        .execute(db.pool())
        .await
        .unwrap();
    capture_graph_episodes(db.pool()).await.unwrap();

    let noise: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE files LIKE '%target-dev/%'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let source: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memories WHERE files LIKE '%crates/ax-memory/src/brain.rs%'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(noise, 0);
    assert_eq!(source, 1);
}
