use ax_memory::{recall, reindex_memory_fts, remember, RememberInput};

#[tokio::test]
async fn reindex_restores_a_memory_missing_from_fts() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let row = remember(
        db.pool(),
        RememberInput {
            title: "Blossom Suno recipe".into(),
            body: "Conflicts remain after the vocal preset.".into(),
            kind: Some("bug_fix".into()),
            tags: vec!["Blossom".into(), "Suno".into()],
            files: vec![],
            source: Some("mcp".into()),
        },
    )
    .await
    .unwrap();

    let before = recall(db.pool(), "Blossom Suno", 5).await.unwrap();
    assert!(
        before.iter().any(|hit| hit.memory.id == row.id),
        "fresh write must be findable before the desync"
    );

    // Clear the hash vector first (memories UPDATE trigger refreshes FTS).
    // Then delete the FTS postings so only a rebuild can restore keyword recall.
    sqlx::query("UPDATE memories SET embedding = NULL WHERE id = ?")
        .bind(&row.id)
        .execute(db.pool())
        .await
        .unwrap();

    let rowid: i64 = sqlx::query_scalar("SELECT rowid FROM memories WHERE id = ?")
        .bind(&row.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let title: String = sqlx::query_scalar("SELECT title FROM memories WHERE id = ?")
        .bind(&row.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let body: String = sqlx::query_scalar("SELECT body FROM memories WHERE id = ?")
        .bind(&row.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let tags: String = sqlx::query_scalar("SELECT tags FROM memories WHERE id = ?")
        .bind(&row.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let search_text: String =
        sqlx::query_scalar("SELECT COALESCE(search_text, '') FROM memories WHERE id = ?")
            .bind(&row.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    sqlx::query(
        "INSERT INTO memories_fts(memories_fts, rowid, id, title, body, tags, search_text)
         VALUES ('delete', ?, ?, ?, ?, ?, ?)",
    )
    .bind(rowid)
    .bind(&row.id)
    .bind(&title)
    .bind(&body)
    .bind(&tags)
    .bind(&search_text)
    .execute(db.pool())
    .await
    .unwrap();

    let missing = recall(db.pool(), "Blossom Suno", 5).await.unwrap();
    assert!(
        missing.iter().all(|hit| hit.memory.id != row.id),
        "deleted FTS row (and cleared embedding) must hide the memory from recall"
    );

    let _updated = reindex_memory_fts(db.pool()).await.unwrap();

    let after = recall(db.pool(), "Blossom Suno", 5).await.unwrap();
    db.close().await;
    assert!(
        after.iter().any(|hit| hit.memory.id == row.id),
        "reindex must restore FTS for the memory"
    );
}
