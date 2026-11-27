use ax_db::graph_history::{record_node_version, GraphNodeVersion};
use ax_memory::{auto_attach_memory, refresh_anchor_flags, remember, RememberInput};

#[tokio::test]
async fn auto_attach_links_memory_files_to_node_versions() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    let pool = db.pool();

    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, updated_at)
         VALUES ('n1', 'function', 'parse', 'parse', 'src/a.rs', 'rust', 1, 2, 0, 0, 1)",
    )
    .execute(pool)
    .await
    .unwrap();
    record_node_version(
        pool,
        &GraphNodeVersion {
            node_key: "n1".into(),
            body: "parse\nsig".into(),
            signature: "sig".into(),
            kind: "function".into(),
            file_path: "src/a.rs".into(),
            start_line: 1,
            end_line: 2,
            valid_from: 10,
            commit_id: "".into(),
            session_id: "s".into(),
            turn_id: "t".into(),
        },
    )
    .await
    .unwrap();

    let row = remember(
        pool,
        RememberInput {
            title: "note".into(),
            body: "about parse".into(),
            kind: Some("note".into()),
            tags: vec![],
            files: vec!["src/a.rs".into()],
            source: Some("manual".into()),
        },
    )
    .await
    .unwrap();

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_anchors WHERE memory_id = ?")
        .bind(&row.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(count >= 1, "expected auto-attach anchors");

    // Second attach is idempotent
    assert_eq!(auto_attach_memory(pool, &row.id).await.unwrap(), 0);

    let (stale, orphan) = refresh_anchor_flags(pool).await.unwrap();
    assert_eq!(stale + orphan, 0);

    db.close().await;
}
