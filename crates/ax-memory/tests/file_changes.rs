use ax_memory::save_file_changes;

#[tokio::test]
async fn saving_file_changes_writes_one_memory_per_path() {
    let root = std::env::temp_dir().join(format!("ax-file-change-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let none = save_file_changes(db.pool(), &[], "conv-1", "gen-1", 1)
        .await
        .unwrap();
    assert_eq!(none, 0);

    let written = save_file_changes(
        db.pool(),
        &["src/a.rs".into(), "src/b.rs".into()],
        "conv-1",
        "gen-1",
        1,
    )
    .await
    .unwrap();
    assert_eq!(written, 2);

    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT kind, files, session_id, turn_id FROM memories WHERE kind = 'file_change' ORDER BY files",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "file_change");
    assert_eq!(rows[0].1, "[\"src/a.rs\"]");
    assert_eq!(rows[0].2, "conv-1");
    assert_eq!(rows[0].3, "gen-1");
    assert_eq!(rows[1].1, "[\"src/b.rs\"]");
}
