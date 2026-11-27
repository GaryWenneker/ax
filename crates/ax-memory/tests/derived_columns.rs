//! Every path that inserts a memory fills the columns derived from its content
//! (search_text, origin, trust_level), as `remember` does.

mod common;

use ax_memory::{
    capture_git_history, capture_graph_episodes, save_file_change_with_hunk, save_file_changes,
    save_turn, trust_level_for, TurnRecord,
};

async fn derived(db: &ax_db::Database, kind: &str) -> Vec<(String, String, String, String)> {
    sqlx::query_as(
        "SELECT source, search_text, origin, trust_level FROM memories WHERE kind = ? ORDER BY id",
    )
    .bind(kind)
    .fetch_all(db.pool())
    .await
    .unwrap()
}

fn assert_derived(rows: &[(String, String, String, String)], kind: &str, word: &str) {
    assert!(!rows.is_empty(), "{kind}: no rows");
    for (source, search_text, origin, trust) in rows {
        assert!(
            search_text.contains(word),
            "{kind}: search_text {search_text:?} lacks {word:?}"
        );
        assert_eq!(origin, source, "{kind}: origin");
        assert_eq!(trust, trust_level_for(source, kind), "{kind}: trust_level");
    }
}

#[tokio::test]
async fn every_insert_path_fills_the_derived_columns() {
    let dir = tempfile::tempdir().unwrap();
    let db = ax_db::Database::open(&dir.path().join("ax.db"))
        .await
        .unwrap();

    let record = TurnRecord {
        id: "turn-1".into(),
        title: "Explain parseConfigFile".into(),
        body: "How does parseConfigFile load settings?".into(),
        files: vec!["src/config.rs".into()],
    };
    assert!(save_turn(db.pool(), &record, 1).await.unwrap());
    assert_derived(&derived(&db, "turn").await, "turn", "parse");

    save_file_changes(db.pool(), &["src/widgetStore.rs".into()], "c", "g", 1)
        .await
        .unwrap();
    save_file_change_with_hunk(
        db.pool(),
        "src/hunkFile.rs",
        Some("+x"),
        None,
        None,
        "c",
        "g2",
        1,
    )
    .await
    .unwrap();
    assert_derived(&derived(&db, "file_change").await, "file_change", "src");

    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |args: &[&str]| common::git(&repo, &[], args);
    git(&["init", "-q"]);
    std::fs::write(repo.join("a.txt"), "a").unwrap();
    git(&["add", "a.txt"]);
    git(&["commit", "-q", "-m", "Teach the loader about fallbackPaths"]);
    let captured = capture_git_history(db.pool(), &repo, 10).await.unwrap();
    assert_eq!(captured.captured, 1);
    assert_derived(&derived(&db, "git").await, "git", "fallback");

    for signature in ["fn old()", "fn new()"] {
        sqlx::query(
            "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
             VALUES ('sym-1', 'function', 'loadWidget', 'w::loadWidget', 'src/w.rs', 'rust', 1, 4, 0, 1, ?, 1)
             ON CONFLICT(id) DO UPDATE SET signature = excluded.signature",
        )
        .bind(signature)
        .execute(db.pool())
        .await
        .unwrap();
        capture_graph_episodes(db.pool()).await.unwrap();
    }
    assert_derived(&derived(&db, "graph_diff").await, "graph_diff", "widget");
    db.close().await;
}
