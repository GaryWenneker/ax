use ax_memory::anchor_memory;

#[tokio::test]
async fn anchoring_a_memory_is_idempotent() {
    let root = std::env::temp_dir().join(format!("ax-anchor-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    assert!(!anchor_memory(db.pool(), "", "kept", 1).await.unwrap());
    assert!(!anchor_memory(db.pool(), "m1", "", 1).await.unwrap());

    let first = anchor_memory(db.pool(), "m1", "kept", 1).await.unwrap();
    let second = anchor_memory(db.pool(), "m1", "kept", 1).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_anchors")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert!(first);
    assert!(!second);
    assert_eq!(count, 1);
}
