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

async fn fresh_db() -> (tempfile::TempDir, ax_db::Database) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".ax")).unwrap();
    let db = ax_db::Database::open(&dir.path().join(".ax/ax.db"))
        .await
        .unwrap();
    (dir, db)
}

async fn note(pool: &sqlx::SqlitePool, title: &str) -> String {
    remember(
        pool,
        RememberInput {
            title: title.into(),
            body: format!("{title} body"),
            kind: Some("note".into()),
            tags: vec![],
            files: vec![],
            source: Some("manual".into()),
        },
    )
    .await
    .unwrap()
    .id
}

async fn memory_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn restore_keeps_timestamps_confidence_enabled_and_tier() {
    let (dir, db) = fresh_db().await;
    let pool = db.pool();
    let id = note(pool, "old turn").await;
    sqlx::query(
        "UPDATE memories SET created_at = 1000, updated_at = 2000, confidence = 0.4,
         enabled = 0, tier = 't3' WHERE id = ?",
    )
    .bind(&id)
    .execute(pool)
    .await
    .unwrap();
    let backup = write_full_backup(pool, dir.path(), 5).await.unwrap();
    sqlx::query("DELETE FROM memories")
        .execute(pool)
        .await
        .unwrap();

    let report = restore_from_backup(pool, &backup.path).await.unwrap();
    assert_eq!(report.restored, 1);
    let (created, updated, confidence, enabled, tier): (i64, i64, f64, i64, String) =
        sqlx::query_as(
            "SELECT created_at, updated_at, confidence, enabled, tier FROM memories WHERE id = ?",
        )
        .bind(&id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!((created, updated), (1000, 2000));
    assert!((confidence - 0.4).abs() < 1e-9);
    assert_eq!(enabled, 0, "a disabled memory must stay disabled");
    assert_eq!(tier, "t3");
    db.close().await;
}

#[tokio::test]
async fn restore_refuses_a_backup_whose_checksum_does_not_match() {
    let (dir, db) = fresh_db().await;
    let pool = db.pool();
    note(pool, "first").await;
    note(pool, "second").await;
    let backup = write_full_backup(pool, dir.path(), 5).await.unwrap();
    sqlx::query("DELETE FROM memories")
        .execute(pool)
        .await
        .unwrap();

    // Truncate the payload to one row, as a crash during the write would.
    let plain = zstd::decode_all(std::fs::read(&backup.path).unwrap().as_slice()).unwrap();
    let text = String::from_utf8(plain).unwrap();
    let first_line = format!("{}\n", text.lines().next().unwrap());
    std::fs::write(
        &backup.path,
        zstd::encode_all(first_line.as_bytes(), 3).unwrap(),
    )
    .unwrap();

    let err = restore_from_backup(pool, &backup.path).await;
    assert!(err.is_err(), "checksum mismatch must be an error");
    assert_eq!(memory_count(pool).await, 0);
    db.close().await;
}

#[tokio::test]
async fn restore_writes_nothing_when_a_line_is_malformed() {
    let (dir, db) = fresh_db().await;
    let pool = db.pool();
    note(pool, "first").await;
    let backup = write_full_backup(pool, dir.path(), 5).await.unwrap();
    sqlx::query("DELETE FROM memories")
        .execute(pool)
        .await
        .unwrap();

    // A matching manifest around a payload with one broken line.
    let plain = zstd::decode_all(std::fs::read(&backup.path).unwrap().as_slice()).unwrap();
    let mut text = String::from_utf8(plain).unwrap();
    text.push_str("{not json\n");
    std::fs::write(&backup.path, zstd::encode_all(text.as_bytes(), 3).unwrap()).unwrap();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&backup.manifest_path).unwrap()).unwrap();
    manifest["checksum"] = blake3::hash(text.as_bytes()).to_hex().to_string().into();
    manifest["count"] = 2.into();
    std::fs::write(&backup.manifest_path, manifest.to_string()).unwrap();

    assert!(restore_from_backup(pool, &backup.path).await.is_err());
    assert_eq!(memory_count(pool).await, 0, "no row may be written");
    db.close().await;
}

#[tokio::test]
async fn restore_refuses_a_backup_without_manifest() {
    let (dir, db) = fresh_db().await;
    let pool = db.pool();
    note(pool, "first").await;
    let backup = write_full_backup(pool, dir.path(), 5).await.unwrap();
    sqlx::query("DELETE FROM memories")
        .execute(pool)
        .await
        .unwrap();
    std::fs::remove_file(&backup.manifest_path).unwrap();

    assert!(restore_from_backup(pool, &backup.path).await.is_err());
    assert_eq!(memory_count(pool).await, 0);
    db.close().await;
}
