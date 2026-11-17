use ax_memory::{
    purge_memory, remember, restore_from_backup, search, write_full_backup, Depth, RememberInput,
    SearchRequest,
};

#[tokio::test]
async fn restore_skips_existing_and_tombstoned_ids() {
    let root = std::env::temp_dir().join(format!("ax-restore-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

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
    let backup = write_full_backup(db.pool(), &root, 1).await.unwrap();

    // Wipe vault but keep tombstone path separate.
    sqlx::query("DELETE FROM memories")
        .execute(db.pool())
        .await
        .unwrap();
    let first = restore_from_backup(db.pool(), &backup.path).await.unwrap();
    assert_eq!(first.restored, 1);

    let second = restore_from_backup(db.pool(), &backup.path).await.unwrap();
    assert_eq!(second.skipped_existing, 1);

    assert!(purge_memory(db.pool(), &row.id).await.unwrap());
    let blocked = restore_from_backup(db.pool(), &backup.path).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(blocked.skipped_tombstone, 1);
    assert_eq!(count, 0);
}

/// Proof bar: depth=backup finds a memory that exists only in the backup file,
/// then a full restore into an empty vault recovers every row with regenerated cues.
#[tokio::test]
async fn depth_backup_finds_purged_row_and_full_restore_round_trips() {
    let root = std::env::temp_dir().join(format!("ax-backup-rt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let mut ids = Vec::new();
    for (title, body, tag) in [
        (
            "AlphaUniquePhrase",
            "alpha body about ledger retention",
            "alpha",
        ),
        (
            "BetaUniquePhrase",
            "beta body about rollback policy",
            "beta",
        ),
        (
            "GammaUniquePhrase",
            "gamma body about webhook tenant",
            "gamma",
        ),
    ] {
        let row = remember(
            db.pool(),
            RememberInput {
                title: title.into(),
                body: body.into(),
                kind: Some("decision".into()),
                tags: vec![tag.into()],
                files: vec![],
                source: Some("manual".into()),
            },
        )
        .await
        .unwrap();
        ids.push(row.id);
    }

    let before_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(before_count, 3);

    let backup = write_full_backup(db.pool(), &root, 42).await.unwrap();
    assert_eq!(backup.count, 3);
    assert!(backup.path.exists());
    assert_eq!(
        std::path::Path::new(&backup.path)
            .extension()
            .and_then(|e| e.to_str()),
        Some("zst")
    );

    // Remove live rows so the only copy is the backup file.
    sqlx::query("DELETE FROM memory_cues")
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("DELETE FROM memories")
        .execute(db.pool())
        .await
        .unwrap();
    let empty: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(empty, 0);

    let backup_hits = search(
        db.pool(),
        &SearchRequest {
            query: "AlphaUniquePhrase".into(),
            depth: Depth::Backup,
            limit: 5,
            ..SearchRequest::default()
        },
    )
    .await
    .unwrap();
    assert!(
        !backup_hits.is_empty(),
        "depth=backup must find the purged memory"
    );
    assert_eq!(backup_hits[0].memory.title, "AlphaUniquePhrase");
    assert!(backup_hits[0]
        .why
        .as_ref()
        .map(|w| w.signals.iter().any(|s| s == "backup"))
        .unwrap_or(false));

    let report = restore_from_backup(db.pool(), &backup.path).await.unwrap();
    assert_eq!(report.restored, 3);
    assert_eq!(report.skipped_existing, 0);
    assert_eq!(report.skipped_tombstone, 0);

    let after_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(after_count, 3);

    for id in &ids {
        let title: String = sqlx::query_scalar("SELECT title FROM memories WHERE id = ?")
            .bind(id)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert!(title.contains("UniquePhrase"));
        let cues: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_cues WHERE memory_id = ?")
            .bind(id)
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert!(cues > 0, "restored memory {id} must have cues");
    }

    // Backup files are never auto-deleted by restore.
    assert!(backup.path.exists());
    assert!(backup.manifest_path.exists());

    db.close().await;
    let _ = std::fs::remove_dir_all(&root);
}
