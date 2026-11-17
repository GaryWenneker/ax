use ax_memory::{backfill_session_ids, remember, RememberInput};

#[tokio::test]
async fn backfill_links_conversation_lines_and_marks_the_rest_unlinked() {
    let root = std::env::temp_dir().join(format!("ax-backfill-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let linked = remember(
        db.pool(),
        RememberInput {
            title: "turn".into(),
            body: "Conversation: deadbeef0012\nhello".into(),
            kind: Some("turn".into()),
            tags: vec![],
            files: vec![],
            source: Some("turn".into()),
        },
    )
    .await
    .unwrap();
    let bare = remember(
        db.pool(),
        RememberInput {
            title: "note".into(),
            body: "no conversation".into(),
            kind: Some("note".into()),
            tags: vec![],
            files: vec![],
            source: Some("manual".into()),
        },
    )
    .await
    .unwrap();
    // Clear stamps so backfill owns them.
    sqlx::query("UPDATE memories SET session_id = '', turn_id = ''")
        .execute(db.pool())
        .await
        .unwrap();

    let report = backfill_session_ids(db.pool()).await.unwrap();
    assert_eq!(report.updated, 2);
    assert_eq!(report.linked, 1);
    assert_eq!(report.unlinked, 1);

    let linked_session: String = sqlx::query_scalar("SELECT session_id FROM memories WHERE id = ?")
        .bind(&linked.id)
        .fetch_one(db.pool())
        .await
        .unwrap();
    let bare_ids: (String, String) =
        sqlx::query_as("SELECT session_id, turn_id FROM memories WHERE id = ?")
            .bind(&bare.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    let again = backfill_session_ids(db.pool()).await.unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(linked_session, "deadbeef0012");
    assert_eq!(bare_ids.0, "Unlinked");
    assert_eq!(bare_ids.1, "Unlinked");
    assert_eq!(again.updated, 0);
}
