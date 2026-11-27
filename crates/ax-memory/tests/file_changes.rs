use ax_memory::save_file_changes;

#[tokio::test]
async fn saving_file_changes_writes_one_memory_per_path() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
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

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "file_change");
    assert_eq!(rows[0].1, "[\"src/a.rs\"]");
    assert_eq!(rows[0].2, "conv-1");
    assert_eq!(rows[0].3, "gen-1");
    assert_eq!(rows[1].1, "[\"src/b.rs\"]");
}

async fn open_db() -> (tempfile::TempDir, std::path::PathBuf, ax_db::Database) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    (tmp, root, db)
}

async fn set_content_hash(pool: &sqlx::SqlitePool, path: &str, hash: &str) {
    sqlx::query(
        "INSERT INTO file_contents (path, content_hash, content, byte_len, updated_at)
         VALUES (?, ?, '', 0, 1)
         ON CONFLICT(path) DO UPDATE SET content_hash = excluded.content_hash",
    )
    .bind(path)
    .bind(hash)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn sync_file_changes_are_recent_tier_and_record_every_new_content() {
    let (_tmp, root, db) = open_db().await;
    let pool = db.pool();
    let paths = vec!["src/a.rs".to_string()];
    set_content_hash(pool, "src/a.rs", "h1").await;
    assert_eq!(
        ax_memory::capture_affected_paths(pool, &root, &paths, 10)
            .await
            .unwrap(),
        1
    );
    // The same content synced again is not a new change.
    assert_eq!(
        ax_memory::capture_affected_paths(pool, &root, &paths, 20)
            .await
            .unwrap(),
        0
    );
    set_content_hash(pool, "src/a.rs", "h2").await;
    assert_eq!(
        ax_memory::capture_affected_paths(pool, &root, &paths, 30)
            .await
            .unwrap(),
        1
    );
    let tiers: Vec<String> =
        sqlx::query_scalar("SELECT tier FROM memories WHERE kind = 'file_change'")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(tiers, vec!["t1".to_string(), "t1".to_string()]);
    db.close().await;
}

#[tokio::test]
async fn turn_retention_archives_old_file_changes() {
    let (_tmp, root, db) = open_db().await;
    let pool = db.pool();
    save_file_changes(pool, &["src/old.rs".into()], "c", "t", 1_000)
        .await
        .unwrap();
    let now = 1_000 + 40 * 86_400_000;
    ax_memory::prune_turns(pool, now, 30, &root.join("backups"))
        .await
        .unwrap();
    let tier: String = sqlx::query_scalar("SELECT tier FROM memories WHERE kind = 'file_change'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(tier, "t3");
    db.close().await;
}

#[tokio::test]
async fn file_change_cues_do_not_crowd_out_prompt_cue_hits() {
    let (_tmp, _, db) = open_db().await;
    let pool = db.pool();
    let decision = ax_memory::remember(
        pool,
        ax_memory::RememberInput {
            title: "Storage layout".into(),
            body: "Keep rows compact.".into(),
            kind: Some("decision".into()),
            tags: vec![],
            files: vec!["src/zebrafile.rs".into()],
            source: Some("manual".into()),
        },
    )
    .await
    .unwrap();
    save_file_changes(pool, &["src/zebrafile.rs".into()], "c", "t", 1)
        .await
        .unwrap();
    let hits = ax_memory::brain::prompt_cues(pool, "zebrafile", 1)
        .await
        .unwrap();
    let ids: Vec<&str> = hits.iter().map(|hit| hit.memory.id.as_str()).collect();
    assert_eq!(ids, vec![decision.id.as_str()]);
    db.close().await;
}

#[tokio::test]
async fn turn_end_hook_does_not_duplicate_a_synced_file_change() {
    let (_tmp, root, db) = open_db().await;
    let pool = db.pool();
    std::fs::create_dir_all(root.join(".ax/turns")).unwrap();
    std::fs::write(
        root.join(".ax/turns/current.json"),
        r#"{"session_id":"conv-1","turn_id":"gen-1"}"#,
    )
    .unwrap();
    set_content_hash(pool, "src/a.rs", "h1").await;
    let paths = vec!["src/a.rs".to_string()];
    assert_eq!(
        ax_memory::capture_affected_paths(pool, &root, &paths, 10)
            .await
            .unwrap(),
        1
    );
    // The turn-end hook reports the same edit for the same session and turn.
    assert_eq!(
        save_file_changes(pool, &paths, "conv-1", "gen-1", 20)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ax_memory::save_file_change_with_hunk(
            pool,
            "src/a.rs",
            Some("+x"),
            None,
            None,
            "conv-1",
            "gen-1",
            30
        )
        .await
        .unwrap(),
        0
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memories WHERE kind = 'file_change' AND files = '[\"src/a.rs\"]'",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    // Another turn still records the path.
    assert_eq!(
        save_file_changes(pool, &paths, "conv-1", "gen-2", 40)
            .await
            .unwrap(),
        1
    );
    db.close().await;
}
