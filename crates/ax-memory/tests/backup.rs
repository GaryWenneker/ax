use ax_memory::{remember, write_full_backup, RememberInput};

#[tokio::test]
async fn full_backup_writes_jsonl_and_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let empty = write_full_backup(db.pool(), &root, 1).await.unwrap();
    assert_eq!(empty.count, 0);
    assert!(empty.path.exists());
    assert!(empty.manifest_path.exists());
    assert!(empty
        .path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext == "zst"));

    let row = remember(
        db.pool(),
        RememberInput {
            title: "kept".into(),
            body: "body".into(),
            kind: Some("note".into()),
            tags: vec![],
            files: vec![],
            source: Some("manual".into()),
        },
    )
    .await
    .unwrap();

    let full = write_full_backup(db.pool(), &root, 2).await.unwrap();
    let compressed = std::fs::read(&full.path).unwrap();
    let jsonl = String::from_utf8(zstd::decode_all(compressed.as_slice()).unwrap()).unwrap();
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&full.manifest_path).unwrap()).unwrap();
    db.close().await;

    assert_eq!(full.count, 1);
    assert!(jsonl.contains(&row.id));
    assert!(jsonl.contains("kept"));
    assert_eq!(manifest["count"], 1);
    assert_eq!(manifest["encoding"], "zstd");
    assert_eq!(manifest["checksum"], full.checksum);
    assert!(!full.checksum.is_empty());
}

#[tokio::test]
async fn incremental_backup_holds_only_rows_updated_after_since() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".ax")).unwrap();
    let db = ax_db::Database::open(&dir.path().join(".ax/ax.db"))
        .await
        .unwrap();
    for (title, updated) in [("older", 100), ("newer", 300)] {
        let row = remember(
            db.pool(),
            RememberInput {
                title: title.into(),
                body: "body".into(),
                kind: Some("note".into()),
                ..RememberInput::default()
            },
        )
        .await
        .unwrap();
        sqlx::query("UPDATE memories SET updated_at = ? WHERE id = ?")
            .bind(updated)
            .bind(&row.id)
            .execute(db.pool())
            .await
            .unwrap();
    }
    let incr = ax_memory::write_incremental_backup(db.pool(), dir.path(), 200, 9)
        .await
        .unwrap();
    let jsonl =
        String::from_utf8(zstd::decode_all(std::fs::read(&incr.path).unwrap().as_slice()).unwrap())
            .unwrap();
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&incr.manifest_path).unwrap()).unwrap();
    db.close().await;

    assert_eq!(incr.count, 1);
    assert!(jsonl.contains("newer") && !jsonl.contains("older"));
    assert_eq!(manifest["kind"], "incremental");
    assert_eq!(manifest["since"], 200);
    assert_eq!(manifest["count"], 1);
}
