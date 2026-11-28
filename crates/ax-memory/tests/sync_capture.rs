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

async fn open_db(tag: &str) -> (std::path::PathBuf, ax_db::Database) {
    let root = std::env::temp_dir().join(format!("ax-sync-cap-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    (root, db)
}

async fn file_change_rows(db: &ax_db::Database) -> Vec<(String, String, String)> {
    sqlx::query_as(
        "SELECT title, body, files FROM memories WHERE kind = 'file_change' ORDER BY title",
    )
    .fetch_all(db.pool())
    .await
    .unwrap()
}

#[tokio::test]
async fn small_batches_write_one_memory_per_path() {
    let (root, db) = open_db("small").await;
    let paths: Vec<String> = vec![
        "src/a.rs".into(),
        " src/b.rs ".into(),
        "src/a.rs".into(),
        "".into(),
    ];
    let written = ax_memory::capture_affected_paths(db.pool(), &root, &paths, 1)
        .await
        .unwrap();
    let rows = file_change_rows(&db).await;
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(written, 2);
    assert_eq!(rows[0].0, "changed src/a.rs");
    assert_eq!(rows[1].0, "changed src/b.rs");
}

#[tokio::test]
async fn a_burst_of_changes_becomes_one_summary_memory() {
    let (root, db) = open_db("burst").await;
    let total = ax_memory::FILE_CHANGE_BURST_LISTED + 50;
    let mut paths: Vec<String> = (0..total - 10)
        .map(|i| format!("crates/ax-web/src/f{i:03}.rs"))
        .collect();
    paths.extend((0..10).map(|i| format!("docs/page{i}.md")));
    let written = ax_memory::capture_affected_paths(db.pool(), &root, &paths, 1)
        .await
        .unwrap();
    // The same burst again (same session/turn) is not written twice.
    let again = ax_memory::capture_affected_paths(db.pool(), &root, &paths, 2)
        .await
        .unwrap();
    let rows = file_change_rows(&db).await;
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(written, 1);
    assert_eq!(again, 0);
    assert_eq!(rows.len(), 1);
    let (title, body, files) = &rows[0];
    assert_eq!(
        title,
        &format!(
            "changed {total} files in crates/ax-web ({}), docs (10)",
            total - 10
        )
    );
    assert!(body.contains("crates/ax-web/src/f000.rs"));
    assert!(body.ends_with("… 50 more"), "{body}");
    let files: Vec<String> = serde_json::from_str(files).unwrap();
    assert_eq!(files.len(), ax_memory::FILE_CHANGE_BURST_LISTED);
}

#[tokio::test]
async fn the_burst_limit_itself_still_writes_per_path() {
    let (root, db) = open_db("limit").await;
    let paths: Vec<String> = (0..ax_memory::FILE_CHANGE_BURST_LIMIT)
        .map(|i| format!("src/f{i}.rs"))
        .collect();
    let written = ax_memory::capture_affected_paths(db.pool(), &root, &paths, 1)
        .await
        .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(written, ax_memory::FILE_CHANGE_BURST_LIMIT);
}
