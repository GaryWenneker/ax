use ax_memory::{remember, save_turn, RememberInput, TurnRecord};

async fn ids(pool: &sqlx::SqlitePool, id: &str) -> (String, String) {
    sqlx::query_as("SELECT session_id, turn_id FROM memories WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

fn note(title: &str) -> RememberInput {
    RememberInput {
        title: title.into(),
        body: title.into(),
        kind: None,
        tags: vec![],
        files: vec![],
        source: Some("manual".into()),
    }
}

#[tokio::test]
async fn a_saved_memory_and_turn_keep_the_current_ids() {
    let root = std::env::temp_dir().join(format!("ax-mem-turn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax/turns")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let plain = remember(db.pool(), note("plain")).await.unwrap();
    std::fs::write(
        root.join(".ax/turns/current.json"),
        r#"{"session_id":"conv-1","turn_id":"gen-1"}"#,
    )
    .unwrap();
    let stamped = remember(db.pool(), note("stamped")).await.unwrap();
    save_turn(
        db.pool(),
        &TurnRecord {
            id: "turn-1".into(),
            title: "t".into(),
            body: "b".into(),
            files: vec![],
        },
        1,
    )
    .await
    .unwrap();

    let plain_ids = ids(db.pool(), &plain.id).await;
    let stamped_ids = ids(db.pool(), &stamped.id).await;
    let turn_ids = ids(db.pool(), "turn-1").await;
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(plain_ids, (String::new(), String::new()));
    assert_eq!(stamped_ids, ("conv-1".to_string(), "gen-1".to_string()));
    assert_eq!(turn_ids, ("conv-1".to_string(), "gen-1".to_string()));
}
