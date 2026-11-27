use ax_memory::anchor_memory;

#[tokio::test]
async fn anchoring_a_memory_is_idempotent() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    assert!(!anchor_memory(db.pool(), "", "kept", 1).await.unwrap());
    assert!(!anchor_memory(db.pool(), "m1", "", 1).await.unwrap());

    let first = anchor_memory(db.pool(), "m1", "kept", 1).await.unwrap();
    let second = anchor_memory(db.pool(), "m1", "kept", 1).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_anchors")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;

    assert!(first);
    assert!(!second);
    assert_eq!(count, 1);
}

use ax_db::graph_history::{record_node_version, GraphNodeVersion};
use ax_memory::{reanchor_renames, refresh_anchor_flags};
use sqlx::SqlitePool;

async fn open_db() -> (tempfile::TempDir, ax_db::Database) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".ax")).unwrap();
    let db = ax_db::Database::open(&dir.path().join(".ax/ax.db"))
        .await
        .unwrap();
    (dir, db)
}

async fn insert_node(pool: &SqlitePool, id: &str, name: &str, file: &str) {
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, updated_at)
         VALUES (?, 'function', ?, ?, ?, 'rust', 1, 2, 0, 0, 1)",
    )
    .bind(id)
    .bind(name)
    .bind(name)
    .bind(file)
    .execute(pool)
    .await
    .unwrap();
}

async fn delete_node(pool: &SqlitePool, id: &str) {
    sqlx::query("DELETE FROM nodes WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

async fn record(pool: &SqlitePool, key: &str, body: &str, file: &str, at: i64) {
    record_node_version(
        pool,
        &GraphNodeVersion {
            node_key: key.into(),
            body: body.into(),
            signature: "fn db_err(pool: &SqlitePool) -> AxError".into(),
            kind: "function".into(),
            file_path: file.into(),
            start_line: 1,
            end_line: 2,
            valid_from: at,
            commit_id: String::new(),
            session_id: String::new(),
            turn_id: String::new(),
        },
    )
    .await
    .unwrap();
}

async fn anchors_of(pool: &SqlitePool, memory_id: &str) -> Vec<(String, i64, i64)> {
    sqlx::query_as(
        "SELECT node_key, stale, orphan FROM memory_anchors WHERE memory_id = ? ORDER BY node_key",
    )
    .bind(memory_id)
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn orphan_flag_clears_when_the_node_comes_back() {
    let (_dir, db) = open_db().await;
    let pool = db.pool();
    insert_node(pool, "a.rs::parse", "parse", "a.rs").await;
    record(pool, "a.rs::parse", "parse\nsig", "a.rs", 10).await;
    anchor_memory(pool, "m1", "a.rs::parse", 1).await.unwrap();

    delete_node(pool, "a.rs::parse").await;
    assert_eq!(refresh_anchor_flags(pool).await.unwrap(), (0, 1));
    assert_eq!(anchors_of(pool, "m1").await[0].2, 1);
    // An anchor that is already orphaned is not flagged or counted again.
    assert_eq!(refresh_anchor_flags(pool).await.unwrap(), (0, 0));

    // The file comes back (branch switch): the orphan flag is cleared.
    insert_node(pool, "a.rs::parse", "parse", "a.rs").await;
    assert_eq!(refresh_anchor_flags(pool).await.unwrap(), (0, 0));
    assert_eq!(anchors_of(pool, "m1").await[0].2, 0);
    db.close().await;
}

#[tokio::test]
async fn stale_flag_follows_the_live_body_hash() {
    let (_dir, db) = open_db().await;
    let pool = db.pool();
    insert_node(pool, "a.rs::parse", "parse", "a.rs").await;
    record(pool, "a.rs::parse", "parse\nsig-1", "a.rs", 10).await;
    anchor_memory(pool, "m1", "a.rs::parse", 1).await.unwrap();
    assert_eq!(refresh_anchor_flags(pool).await.unwrap(), (0, 0));

    record(pool, "a.rs::parse", "parse\nsig-2", "a.rs", 20).await;
    assert_eq!(refresh_anchor_flags(pool).await.unwrap(), (1, 0));
    assert_eq!(refresh_anchor_flags(pool).await.unwrap(), (0, 0));
    assert_eq!(anchors_of(pool, "m1").await[0].1, 1);

    // The change is reverted: the live hash matches the anchored version again.
    record(pool, "a.rs::parse", "parse\nsig-1", "a.rs", 30).await;
    assert_eq!(refresh_anchor_flags(pool).await.unwrap(), (0, 0));
    assert_eq!(anchors_of(pool, "m1").await[0].1, 0);
    db.close().await;
}

#[tokio::test]
async fn reanchor_leaves_two_live_symbols_with_the_same_signature_alone() {
    let (_dir, db) = open_db().await;
    let pool = db.pool();
    insert_node(pool, "a.rs::db_err", "db_err", "a.rs").await;
    insert_node(pool, "b.rs::db_err", "db_err", "b.rs").await;
    record(pool, "a.rs::db_err", "db_err\nsig", "a.rs", 10).await;
    record(pool, "b.rs::db_err", "db_err\nsig", "b.rs", 10).await;
    anchor_memory(pool, "m1", "a.rs::db_err", 1).await.unwrap();

    assert_eq!(reanchor_renames(pool).await.unwrap(), 0);
    let anchors = anchors_of(pool, "m1").await;
    assert_eq!(anchors.len(), 1);
    assert_eq!(anchors[0].0, "a.rs::db_err");
    db.close().await;
}

#[tokio::test]
async fn reanchor_moves_anchors_when_a_symbol_is_renamed() {
    let (_dir, db) = open_db().await;
    let pool = db.pool();
    insert_node(pool, "z.rs::parse", "parse", "z.rs").await;
    record(pool, "z.rs::parse", "parse\nsig", "z.rs", 10).await;
    anchor_memory(pool, "m1", "z.rs::parse", 1).await.unwrap();

    // The file is moved: the old key disappears, the new key appears later.
    delete_node(pool, "z.rs::parse").await;
    insert_node(pool, "a.rs::parse", "parse", "a.rs").await;
    record(pool, "a.rs::parse", "parse\nsig", "a.rs", 20).await;

    assert_eq!(reanchor_renames(pool).await.unwrap(), 1);
    let anchors = anchors_of(pool, "m1").await;
    assert_eq!(anchors.len(), 1);
    assert_eq!(anchors[0].0, "a.rs::parse");
    // The removed key's version is closed, so it leaves the rename join.
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_node_versions WHERE node_key = 'z.rs::parse' AND valid_to IS NULL",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(open, 0);
    let renamed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_links
         WHERE from_id = 'node:z.rs::parse' AND to_id = 'node:a.rs::parse' AND kind = 'renamed'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(renamed, 1);
    assert_eq!(reanchor_renames(pool).await.unwrap(), 0);
    db.close().await;
}

#[tokio::test]
async fn reanchor_skips_a_removed_symbol_with_several_candidates() {
    let (_dir, db) = open_db().await;
    let pool = db.pool();
    insert_node(pool, "a.rs::db_err", "db_err", "a.rs").await;
    record(pool, "a.rs::db_err", "db_err\nsig", "a.rs", 10).await;
    anchor_memory(pool, "m1", "a.rs::db_err", 1).await.unwrap();
    delete_node(pool, "a.rs::db_err").await;
    for key in ["b.rs::db_err", "c.rs::db_err"] {
        insert_node(pool, key, "db_err", key).await;
        record(pool, key, "db_err\nsig", key, 20).await;
    }

    assert_eq!(reanchor_renames(pool).await.unwrap(), 0);
    assert_eq!(anchors_of(pool, "m1").await[0].0, "a.rs::db_err");
    db.close().await;
}

async fn open_versions(pool: &SqlitePool, key: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM graph_node_versions WHERE node_key = ? AND valid_to IS NULL",
    )
    .bind(key)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn reanchor_closes_a_removed_symbol_whose_candidates_fill_a_page() {
    let (_dir, db) = open_db().await;
    let pool = db.pool();
    // An old key with more same-body candidates than one page holds.
    insert_node(pool, "a.rs::trivial", "trivial", "a.rs").await;
    record(pool, "a.rs::trivial", "{}", "a.rs", 10).await;
    anchor_memory(pool, "m1", "a.rs::trivial", 1).await.unwrap();
    delete_node(pool, "a.rs::trivial").await;
    for i in 0..520 {
        let key = format!("b{i:04}.rs::trivial");
        insert_node(pool, &key, "trivial", &key).await;
        record(pool, &key, "{}", &key, 20).await;
    }
    // A later old key with one clear rename target.
    insert_node(pool, "z.rs::db_err", "db_err", "z.rs").await;
    record(pool, "z.rs::db_err", "db_err\nunique", "z.rs", 10).await;
    anchor_memory(pool, "m2", "z.rs::db_err", 1).await.unwrap();
    delete_node(pool, "z.rs::db_err").await;
    insert_node(pool, "zz.rs::db_err2", "db_err2", "zz.rs").await;
    record(pool, "zz.rs::db_err2", "db_err\nunique", "zz.rs", 20).await;

    let mut moved = 0;
    for _ in 0..2 {
        moved += reanchor_renames(pool).await.unwrap();
    }
    assert_eq!(open_versions(pool, "a.rs::trivial").await, 0);
    assert_eq!(open_versions(pool, "z.rs::db_err").await, 0);
    assert_eq!(moved, 1);
    assert_eq!(anchors_of(pool, "m1").await[0].0, "a.rs::trivial");
    assert_eq!(anchors_of(pool, "m2").await[0].0, "zz.rs::db_err2");
    db.close().await;
}
