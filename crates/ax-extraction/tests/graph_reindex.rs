use std::fs;
use std::path::PathBuf;

use ax_db::queries::QueryBuilder;
use ax_db::Database;
use ax_extraction::reindex_damaged_files;

fn temp_root(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("ax-graph-reindex-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(path.join("src")).unwrap();
    path
}

#[tokio::test]
async fn reindex_replaces_the_stale_node_and_skips_a_path_that_leaves_the_project() {
    let root = temp_root("kept");
    fs::write(
        root.join("src/kept.rs"),
        "pub fn kept() -> u32 {\n    7\n}\n",
    )
    .unwrap();
    let leak_name = format!("ax-reindex-leak-{}.rs", std::process::id());
    let leak = root.parent().unwrap().join(&leak_name);
    fs::write(&leak, "pub fn leaked() -> u32 {\n    1\n}\n").unwrap();

    let db_path = root.join(".ax/ax.db");
    fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let db = Database::open(&db_path).await.unwrap();
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES
         ('stale-id', 'function', 'stale', 'stale', 'src/kept.rs', 'rust', 1, 1, 0, 1, 'fn stale()', 1),
         ('other-id', 'function', 'other', 'other', 'src/other.rs', 'rust', 1, 1, 0, 1, 'fn other()', 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let queries = QueryBuilder::new(db.pool().clone());
    let done = reindex_damaged_files(
        &root,
        &queries,
        &[
            "reindex:src/kept.rs".to_string(),
            format!("reindex:../{leak_name}"),
        ],
    )
    .await
    .unwrap();
    assert_eq!(done, vec!["reindex:1".to_string()]);

    let kept: Vec<String> =
        sqlx::query_scalar("SELECT name FROM nodes WHERE file_path = 'src/kept.rs' ORDER BY name")
            .fetch_all(db.pool())
            .await
            .unwrap();
    let other: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM nodes WHERE file_path = 'src/other.rs' AND name = 'other'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    let leaked: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE name = 'leaked'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    let _ = fs::remove_file(&leak);
    let _ = fs::remove_dir_all(&root);

    assert!(kept.iter().any(|name| name == "kept"), "{kept:?}");
    assert!(kept.iter().all(|name| name != "stale"), "{kept:?}");
    assert_eq!(other, 1);
    assert_eq!(leaked, 0);
}
