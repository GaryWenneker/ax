use ax_memory::{recall_filtered, remember, MemoryFilter, RememberInput};

async fn note(pool: &sqlx::SqlitePool, title: &str) -> String {
    remember(
        pool,
        RememberInput {
            title: title.into(),
            body: format!("{title} cache bug"),
            kind: None,
            tags: vec![],
            files: vec![],
            source: Some("manual".into()),
        },
    )
    .await
    .unwrap()
    .id
}

#[tokio::test]
async fn recall_keeps_only_the_named_session_and_turn() {
    let root = std::env::temp_dir().join(format!("ax-mem-filter-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax/turns")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let plain = note(db.pool(), "plain").await;
    std::fs::write(
        root.join(".ax/turns/current.json"),
        r#"{"session_id":"conv-1","turn_id":"gen-1"}"#,
    )
    .unwrap();
    let first = note(db.pool(), "first").await;
    std::fs::write(
        root.join(".ax/turns/current.json"),
        r#"{"session_id":"conv-1","turn_id":"gen-2"}"#,
    )
    .unwrap();
    let second = note(db.pool(), "second").await;

    let by_session = recall_filtered(
        db.pool(),
        "cache",
        10,
        &MemoryFilter {
            session: Some("conv-1".into()),
            ..MemoryFilter::default()
        },
    )
    .await
    .unwrap();
    let ids: Vec<_> = by_session.iter().map(|m| m.memory.id.as_str()).collect();
    assert!(ids.contains(&first.as_str()));
    assert!(ids.contains(&second.as_str()));
    assert!(!ids.contains(&plain.as_str()));

    let by_turn = recall_filtered(
        db.pool(),
        "cache",
        10,
        &MemoryFilter {
            turn: Some("gen-2".into()),
            ..MemoryFilter::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(by_turn.len(), 1);
    assert_eq!(by_turn[0].memory.id, second);

    let open = recall_filtered(db.pool(), "cache", 10, &MemoryFilter::default())
        .await
        .unwrap();
    assert!(open.len() >= 3);

    db.close().await;
    let _ = std::fs::remove_dir_all(&root);
}
