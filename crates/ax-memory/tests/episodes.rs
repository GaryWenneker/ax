use ax_memory::{record_episode, EpisodeInput};

#[tokio::test]
async fn recording_an_episode_is_idempotent() {
    let root = std::env::temp_dir().join(format!("ax-ep-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let input = EpisodeInput {
        id: "ep-1".into(),
        session_id: "conv-1".into(),
        turn_id: "gen-1".into(),
        goal: "fix null".into(),
        outcome: "green".into(),
    };
    assert!(record_episode(db.pool(), &input).await.unwrap());
    assert!(!record_episode(db.pool(), &input).await.unwrap());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_episodes")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(count, 1);
}
