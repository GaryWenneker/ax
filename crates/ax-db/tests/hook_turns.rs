use ax_db::hook_turns::{record_hook_turn, HookTurn};
use ax_db::Database;

fn turn(session: &str, id: &str, prompt: &str) -> HookTurn {
    HookTurn {
        project_id: "project".into(),
        session_id: session.into(),
        turn_id: id.into(),
        prompt: prompt.into(),
    }
}

#[tokio::test]
async fn a_prompt_records_one_session_and_numbered_turns() {
    let root = std::env::temp_dir().join(format!("ax-hook-turn-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = Database::open(&root.join(".ax/ax.db")).await.unwrap();

    record_hook_turn(db.pool(), &turn("conv-1", "gen-1", "hello"))
        .await
        .unwrap();
    record_hook_turn(db.pool(), &turn("conv-1", "gen-1", "replaced"))
        .await
        .unwrap();
    record_hook_turn(db.pool(), &turn("conv-1", "gen-2", &"y".repeat(8_001)))
        .await
        .unwrap();
    record_hook_turn(db.pool(), &turn("conv-2", "gen-9", "other"))
        .await
        .unwrap();
    record_hook_turn(db.pool(), &turn("conv-1", "  ", "blank"))
        .await
        .unwrap();

    let sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM agent_sessions WHERE id = 'conv-1'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    let rows: Vec<(String, i64, String)> = sqlx::query_as(
        "SELECT id, sequence, prompt FROM agent_turns WHERE session_id = 'conv-1' ORDER BY sequence",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    let other: i64 = sqlx::query_scalar("SELECT sequence FROM agent_turns WHERE id = 'gen-9'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(sessions, 1);
    assert_eq!(
        rows,
        vec![
            ("gen-1".to_string(), 1, "hello".to_string()),
            ("gen-2".to_string(), 2, "y".repeat(8_000)),
        ]
    );
    assert_eq!(other, 1);
}

#[tokio::test]
async fn finishing_a_turn_stores_outcome_files_and_commits() {
    let root = std::env::temp_dir().join(format!("ax-hook-finish-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = Database::open(&root.join(".ax/ax.db")).await.unwrap();
    record_hook_turn(db.pool(), &turn("conv-1", "gen-1", "hello"))
        .await
        .unwrap();

    let updated = ax_db::hook_turns::finish_hook_turn(
        db.pool(),
        &ax_db::hook_turns::HookTurnEnd {
            turn_id: "gen-1".into(),
            outcome: Some("done".into()),
            files: vec!["src/a.rs".into()],
            commits: vec!["abc".into()],
        },
    )
    .await
    .unwrap();
    assert!(updated);

    let missing = ax_db::hook_turns::finish_hook_turn(
        db.pool(),
        &ax_db::hook_turns::HookTurnEnd {
            turn_id: "missing".into(),
            outcome: Some("nope".into()),
            files: vec![],
            commits: vec![],
        },
    )
    .await
    .unwrap();
    assert!(!missing);

    ax_db::hook_turns::finish_hook_turn(
        db.pool(),
        &ax_db::hook_turns::HookTurnEnd {
            turn_id: "gen-1".into(),
            outcome: None,
            files: vec!["src/b.rs".into()],
            commits: vec![],
        },
    )
    .await
    .unwrap();
    let kept: (String, String, String) =
        sqlx::query_as("SELECT outcome, files, commits FROM agent_turns WHERE id = 'gen-1'")
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(kept.0, "done");
    assert_eq!(kept.1, "[\"src/b.rs\"]");
    assert_eq!(kept.2, "[]");

    let long = "z".repeat(20_001);
    ax_db::hook_turns::finish_hook_turn(
        db.pool(),
        &ax_db::hook_turns::HookTurnEnd {
            turn_id: "gen-1".into(),
            outcome: Some(long),
            files: vec!["src/a.rs".into()],
            commits: vec!["abc".into()],
        },
    )
    .await
    .unwrap();

    let row: (String, String, String, Option<i64>) = sqlx::query_as(
        "SELECT outcome, files, commits, ended_at FROM agent_turns WHERE id = 'gen-1'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(row.0, "z".repeat(20_000));
    assert_eq!(row.1, "[\"src/a.rs\"]");
    assert_eq!(row.2, "[\"abc\"]");
    assert!(row.3.is_some());
}
