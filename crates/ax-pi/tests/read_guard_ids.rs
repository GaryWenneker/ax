use ax_pi::read_guard_ids;

#[test]
fn the_read_guard_uses_the_current_turn_and_keeps_the_placeholder_without_one() {
    let root = std::env::temp_dir().join(format!("ax-guard-ids-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = root.join(".ax/ax.db");
    std::fs::write(&db, "").unwrap();

    assert_eq!(
        read_guard_ids(&db, "caller"),
        ("caller".to_string(), "read-guard".to_string())
    );

    std::fs::create_dir_all(root.join(".ax/turns")).unwrap();
    std::fs::write(
        root.join(".ax/turns/current.json"),
        r#"{"session_id":"conv-1","turn_id":"gen-1"}"#,
    )
    .unwrap();
    assert_eq!(
        read_guard_ids(&db, "caller"),
        ("conv-1".to_string(), "gen-1".to_string())
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn note_read_guard_records_advice_under_the_current_turn() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join(".ax/ax.db");

    // No database: nothing is created.
    ax_pi::note_read_guard(
        &db,
        "caller",
        "rg",
        serde_json::json!({"pattern": "UserService", "path": "src/"}),
        vec!["UserService".into()],
        Vec::new(),
    )
    .await
    .unwrap();
    assert!(!db.exists());

    std::fs::create_dir_all(dir.path().join(".ax/turns")).unwrap();
    std::fs::write(&db, "").unwrap();
    std::fs::write(
        dir.path().join(".ax/turns/current.json"),
        r#"{"session_id":"conv-1","turn_id":"gen-1"}"#,
    )
    .unwrap();
    ax_pi::note_read_guard(
        &db,
        "caller",
        "rg",
        serde_json::json!({"pattern": "UserService", "path": "src/"}),
        vec!["UserService".into()],
        Vec::new(),
    )
    .await
    .unwrap();

    let (text, json) = ax_pi::read_optimization(&db).await.unwrap();
    let items = json["items"].as_array().expect("items");
    assert_eq!(items.len(), 1, "{json}");
    assert!(text.contains("ax_explore"), "{text}");
    let (_, economics) = ax_pi::read_economics(&db, Some("conv-1"), "USD", None)
        .await
        .unwrap();
    assert_eq!(economics["session_id"], "conv-1", "{economics}");
}
