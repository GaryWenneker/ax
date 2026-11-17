use ax_memory::{link_memories, purge_memory, remember, RememberInput};

#[tokio::test]
async fn purging_a_memory_removes_cues_links_and_writes_a_tombstone() {
    let root = std::env::temp_dir().join(format!("ax-purge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let kept = remember(
        db.pool(),
        RememberInput {
            title: "keep-me".into(),
            body: "body".into(),
            kind: Some("note".into()),
            tags: vec![],
            files: vec![],
            source: Some("test".into()),
        },
    )
    .await
    .unwrap();
    let other = remember(
        db.pool(),
        RememberInput {
            title: "other".into(),
            body: "other body".into(),
            kind: Some("note".into()),
            tags: vec![],
            files: vec![],
            source: Some("test".into()),
        },
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO memory_cues (cue, memory_id, weight, source) VALUES (?, ?, 1.0, ?)")
        .bind("keep")
        .bind(&kept.id)
        .bind("trigger")
        .execute(db.pool())
        .await
        .unwrap();
    link_memories(db.pool(), &kept.id, &other.id, "follows")
        .await
        .unwrap();

    let unknown = purge_memory(db.pool(), "missing-id").await.unwrap();
    assert!(!unknown);

    let purged = purge_memory(db.pool(), &kept.id).await.unwrap();
    assert!(purged);

    let again = purge_memory(db.pool(), &kept.id).await.unwrap();
    assert!(!again);

    let memories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE id = ?")
        .bind(&kept.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let cues: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_cues WHERE memory_id = ?")
        .bind(&kept.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_links WHERE from_id = ? OR to_id = ?")
            .bind(&kept.id)
            .bind(&kept.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    let tombs: Vec<String> =
        sqlx::query_scalar("SELECT memory_id FROM memory_tombstones ORDER BY memory_id")
            .fetch_all(db.pool())
            .await
            .unwrap();
    let other_still: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE id = ?")
        .bind(&other.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(memories, 0);
    assert_eq!(cues, 0);
    assert_eq!(links, 0);
    assert_eq!(tombs, vec![kept.id]);
    assert_eq!(other_still, 1);
}
