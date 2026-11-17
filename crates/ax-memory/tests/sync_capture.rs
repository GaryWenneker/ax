use ax_memory::capture_sync_file_change;

#[tokio::test]
async fn sync_capture_writes_only_when_the_hash_changes() {
    let root = std::env::temp_dir().join(format!("ax-sync-cap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let same = capture_sync_file_change(
        db.pool(),
        "src/a.rs",
        Some("abc"),
        "abc",
        "conv-1",
        "gen-1",
        1,
    )
    .await
    .unwrap();
    assert_eq!(same, 0);

    let blank = capture_sync_file_change(db.pool(), "", None, "abc", "conv-1", "gen-1", 1)
        .await
        .unwrap();
    assert_eq!(blank, 0);

    let written = capture_sync_file_change(
        db.pool(),
        "src/a.rs",
        Some("old"),
        "new",
        "conv-1",
        "gen-1",
        1,
    )
    .await
    .unwrap();
    assert_eq!(written, 1);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE kind = 'file_change'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(count, 1);
}
