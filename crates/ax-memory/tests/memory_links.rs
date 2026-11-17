use ax_memory::{link_memories, remember, save_turn, RememberInput, TurnRecord, MEMORY_LINK_KINDS};

async fn open(name: &str) -> (std::path::PathBuf, ax_db::Database) {
    let root = std::env::temp_dir().join(format!("ax-mem-links-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax/turns")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    (root, db)
}

#[tokio::test]
async fn only_the_named_link_kinds_are_stored() {
    let (root, db) = open("kinds").await;
    let a = remember(
        db.pool(),
        RememberInput {
            title: "a".into(),
            body: "a".into(),
            kind: None,
            tags: vec![],
            files: vec![],
            source: Some("manual".into()),
        },
    )
    .await
    .unwrap();
    let b = remember(
        db.pool(),
        RememberInput {
            title: "b".into(),
            body: "b".into(),
            kind: None,
            tags: vec![],
            files: vec![],
            source: Some("manual".into()),
        },
    )
    .await
    .unwrap();

    link_memories(db.pool(), &a.id, &b.id, "follows")
        .await
        .unwrap();
    link_memories(db.pool(), &a.id, &b.id, "follows")
        .await
        .unwrap();
    let bad = link_memories(db.pool(), &a.id, &b.id, "nope").await;
    assert!(bad.is_err());

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_links WHERE from_id = ? AND to_id = ? AND kind = 'follows'",
    )
    .bind(&a.id)
    .bind(&b.id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    let nope: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memory_links WHERE kind = 'nope'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(count, 1);
    assert_eq!(nope, 0);
    assert_eq!(
        MEMORY_LINK_KINDS,
        &[
            "in_session",
            "follows",
            "produced",
            "changed",
            "touches",
            "committed_as",
            "derived_from",
            "supersedes",
            "source",
        ]
    );
}

#[tokio::test]
async fn a_newer_turn_follows_the_earlier_turn_in_the_same_session() {
    let (root, db) = open("follows").await;
    std::fs::write(
        root.join(".ax/turns/current.json"),
        r#"{"session_id":"conv-1","turn_id":"gen-1"}"#,
    )
    .unwrap();
    save_turn(
        db.pool(),
        &TurnRecord {
            id: "turn-old".into(),
            title: "old".into(),
            body: "old".into(),
            files: vec![],
        },
        10,
    )
    .await
    .unwrap();
    save_turn(
        db.pool(),
        &TurnRecord {
            id: "turn-new".into(),
            title: "new".into(),
            body: "new".into(),
            files: vec![],
        },
        20,
    )
    .await
    .unwrap();

    let linked: Option<String> = sqlx::query_scalar(
        "SELECT kind FROM memory_links WHERE from_id = 'turn-new' AND to_id = 'turn-old'",
    )
    .fetch_optional(db.pool())
    .await
    .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(linked.as_deref(), Some("follows"));
}
