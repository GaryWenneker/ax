use ax_memory::{link_memories, remember, save_turn, RememberInput, TurnRecord, MEMORY_LINK_KINDS};

async fn open() -> (tempfile::TempDir, std::path::PathBuf, ax_db::Database) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax/turns")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    (tmp, root, db)
}

#[tokio::test]
async fn only_the_named_link_kinds_are_stored() {
    let (_tmp, _, db) = open().await;
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
            "renamed",
            "corroborates",
        ]
    );
}

async fn lesson(db: &ax_db::Database, title: &str) -> String {
    remember(
        db.pool(),
        RememberInput {
            title: title.into(),
            body: title.into(),
            kind: Some("lesson".into()),
            tags: vec![],
            files: vec![],
            source: Some("extractor".into()),
        },
    )
    .await
    .unwrap()
    .id
}

async fn confidence(db: &ax_db::Database, id: &str) -> f64 {
    sqlx::query_scalar("SELECT confidence FROM memories WHERE id = ?")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn corroborating_two_lessons_links_them_and_counts_once() {
    let (_tmp, _, db) = open().await;
    let a = lesson(&db, "null panic in parse").await;
    let b = lesson(&db, "null panic in parse again").await;
    sqlx::query("UPDATE memories SET confidence = 0.5")
        .execute(db.pool())
        .await
        .unwrap();
    let before = confidence(&db, &a).await;

    assert!(ax_memory::extract::corroborate_lessons(db.pool(), &a, &b)
        .await
        .unwrap());
    let once = confidence(&db, &a).await;
    let once_b = confidence(&db, &b).await;
    assert!(once > before, "{before} -> {once}");
    let links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_links WHERE kind = 'corroborates'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(links, 1);

    // The same pair, in either order, does not raise confidence again.
    assert!(!ax_memory::extract::corroborate_lessons(db.pool(), &a, &b)
        .await
        .unwrap());
    assert!(!ax_memory::extract::corroborate_lessons(db.pool(), &b, &a)
        .await
        .unwrap());
    assert_eq!(confidence(&db, &a).await, once);
    assert_eq!(confidence(&db, &b).await, once_b);
    db.close().await;
}

#[tokio::test]
async fn a_newer_turn_follows_the_earlier_turn_in_the_same_session() {
    let (_tmp, root, db) = open().await;
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
    assert_eq!(linked.as_deref(), Some("follows"));
}
