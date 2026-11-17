use ax_memory::{remember, trust_level_for, RememberInput};

#[test]
fn trust_levels_follow_source_and_kind() {
    assert_eq!(trust_level_for("mcp", "note"), "trusted");
    assert_eq!(trust_level_for("import", "note"), "untrusted");
    assert_eq!(trust_level_for("manual", "doc"), "untrusted");
    assert_eq!(trust_level_for("git", "git"), "untrusted");
}

#[tokio::test]
async fn remember_stores_origin_and_trust_level() {
    let root = std::env::temp_dir().join(format!("ax-prov-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let trusted = remember(
        db.pool(),
        RememberInput {
            title: "ok".into(),
            body: "body".into(),
            kind: Some("note".into()),
            tags: vec![],
            files: vec![],
            source: Some("mcp".into()),
        },
    )
    .await
    .unwrap();
    let untrusted = remember(
        db.pool(),
        RememberInput {
            title: "doc".into(),
            body: "imported".into(),
            kind: Some("doc".into()),
            tags: vec![],
            files: vec![],
            source: Some("import".into()),
        },
    )
    .await
    .unwrap();

    let trusted_row: (String, String) =
        sqlx::query_as("SELECT origin, trust_level FROM memories WHERE id = ?")
            .bind(&trusted.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    let untrusted_row: (String, String) =
        sqlx::query_as("SELECT origin, trust_level FROM memories WHERE id = ?")
            .bind(&untrusted.id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(trusted_row.0, "mcp");
    assert_eq!(trusted_row.1, "trusted");
    assert_eq!(untrusted_row.0, "import");
    assert_eq!(untrusted_row.1, "untrusted");
}
