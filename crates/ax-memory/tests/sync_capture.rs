use ax_memory::capture_sync_file_change;

#[tokio::test]
async fn sync_capture_writes_only_when_the_hash_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();

    let same = capture_sync_file_change(
        db.pool(),
        "src/a.rs",
        Some("abc"),
        "abc",
        "conv-1",
        "gen-1",
        1,
    )
    .await
    .unwrap();
    assert_eq!(same, 0);

    let blank = capture_sync_file_change(db.pool(), "", None, "abc", "conv-1", "gen-1", 1)
        .await
        .unwrap();
    assert_eq!(blank, 0);

    let written = capture_sync_file_change(
        db.pool(),
        "src/a.rs",
        Some("old"),
        "new",
        "conv-1",
        "gen-1",
        1,
    )
    .await
    .unwrap();
    assert_eq!(written, 1);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories WHERE kind = 'file_change'")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    assert_eq!(count, 1);
}

async fn set_source(pool: &sqlx::SqlitePool, content: &str) {
    sqlx::query(
        "INSERT INTO file_contents (path, content_hash, content, byte_len, updated_at)
         VALUES ('src/p.rs', ?, ?, ?, 1)
         ON CONFLICT(path) DO UPDATE SET content_hash = excluded.content_hash,
             content = excluded.content, byte_len = excluded.byte_len",
    )
    .bind(blake3::hash(content.as_bytes()).to_hex().to_string())
    .bind(content)
    .bind(content.len() as i64)
    .execute(pool)
    .await
    .unwrap();
}

async fn anchor_states(pool: &sqlx::SqlitePool) -> Vec<(i64, i64)> {
    sqlx::query_as("SELECT version, stale FROM memory_anchors WHERE node_key = 'np'")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_body_rewrite_with_the_same_signature_makes_the_anchor_stale() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    let pool = db.pool();
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('np', 'function', 'parse', 'parse', 'src/p.rs', 'rust', 2, 4, 0, 1, 'fn parse()', 1)",
    )
    .execute(pool)
    .await
    .unwrap();
    let paths = vec!["src/p.rs".to_string()];
    set_source(pool, "// header\nfn parse() {\n    no_alloc()\n}\n").await;
    assert_eq!(
        ax_memory::record_versions_for_paths(pool, &root, &paths, 10)
            .await
            .unwrap(),
        1
    );
    let memory = ax_memory::remember(
        pool,
        ax_memory::RememberInput {
            title: "parse() must not allocate".into(),
            body: "Keep parse allocation free.".into(),
            kind: Some("convention".into()),
            tags: vec![],
            files: vec![],
            source: Some("manual".into()),
        },
    )
    .await
    .unwrap();
    assert!(ax_memory::anchor_memory(pool, &memory.id, "np", 1)
        .await
        .unwrap());

    // An edit outside the function body records nothing new.
    set_source(
        pool,
        "// header, reworded\nfn parse() {\n    no_alloc()\n}\n",
    )
    .await;
    assert_eq!(
        ax_memory::record_versions_for_paths(pool, &root, &paths, 20)
            .await
            .unwrap(),
        0
    );

    // Same signature, new body: a new version, and the anchor goes stale.
    set_source(
        pool,
        "// header, reworded\nfn parse() {\n    Vec::new()\n}\n",
    )
    .await;
    assert_eq!(
        ax_memory::record_versions_for_paths(pool, &root, &paths, 30)
            .await
            .unwrap(),
        1
    );
    ax_memory::refresh_anchor_flags(pool).await.unwrap();
    assert_eq!(anchor_states(pool).await, vec![(1, 1)]);
    db.close().await;
}

#[tokio::test]
async fn versions_recorded_with_the_old_signature_hash_are_upgraded_not_flagged() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    let pool = db.pool();
    sqlx::query(
        "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language, start_line, end_line, start_column, end_column, signature, updated_at)
         VALUES ('np', 'function', 'parse', 'parse', 'src/p.rs', 'rust', 1, 3, 0, 1, 'fn parse()', 1)",
    )
    .execute(pool)
    .await
    .unwrap();
    // A version stored by an older build: body = name + signature.
    ax_db::graph_history::record_node_version(
        pool,
        &ax_db::graph_history::GraphNodeVersion {
            node_key: "np".into(),
            body: "parse\nfn parse()".into(),
            signature: "fn parse()".into(),
            kind: "function".into(),
            file_path: "src/p.rs".into(),
            start_line: 1,
            end_line: 3,
            valid_from: 1,
            commit_id: String::new(),
            session_id: String::new(),
            turn_id: String::new(),
        },
    )
    .await
    .unwrap();
    let memory = ax_memory::remember(
        pool,
        ax_memory::RememberInput {
            title: "parse note".into(),
            body: "About parse.".into(),
            kind: Some("note".into()),
            tags: vec![],
            files: vec![],
            source: Some("manual".into()),
        },
    )
    .await
    .unwrap();
    ax_memory::anchor_memory(pool, &memory.id, "np", 1)
        .await
        .unwrap();
    set_source(pool, "fn parse() {\n    no_alloc()\n}\n").await;
    let paths = vec!["src/p.rs".to_string()];
    assert_eq!(
        ax_memory::record_versions_for_paths(pool, &root, &paths, 10)
            .await
            .unwrap(),
        0
    );
    ax_memory::refresh_anchor_flags(pool).await.unwrap();
    assert_eq!(anchor_states(pool).await, vec![(1, 0)]);

    set_source(pool, "fn parse() {\n    Vec::new()\n}\n").await;
    assert_eq!(
        ax_memory::record_versions_for_paths(pool, &root, &paths, 20)
            .await
            .unwrap(),
        1
    );
    ax_memory::refresh_anchor_flags(pool).await.unwrap();
    assert_eq!(anchor_states(pool).await, vec![(1, 1)]);
    db.close().await;
}
