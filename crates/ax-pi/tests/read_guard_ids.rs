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
