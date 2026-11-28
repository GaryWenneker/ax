use ax_memory::{prune_excluded_file_changes, save_file_changes};

async fn count(db: &ax_db::Database, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(db.pool()).await.unwrap()
}

#[tokio::test]
async fn prune_removes_only_file_changes_whose_paths_are_all_excluded() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    let paths: Vec<String> = [
        "src/keep.rs",
        "target-dev/release/build/x/out",
        "site/node_modules/pkg/index.js",
        "web/test-results/a.png",
    ]
    .iter()
    .map(|p| p.to_string())
    .collect();
    save_file_changes(db.pool(), &paths, "conv", "turn", 1)
        .await
        .unwrap();
    // A burst-style memory naming a kept and an excluded path stays.
    sqlx::query(
        "INSERT INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, enabled)
         VALUES ('mixed', 'file_change', 'changed 2 files in .', '', '[]', '[\"src/a.rs\",\"target/x\"]', 1.0, 'turn', 1, 1, 1)",
    )
    .execute(db.pool())
    .await
    .unwrap();
    // Another kind with an excluded path is never touched.
    ax_memory::remember(
        db.pool(),
        ax_memory::RememberInput {
            title: "note about target".into(),
            body: "keep me".into(),
            kind: Some("note".into()),
            tags: vec![],
            files: vec!["target/x".into()],
            source: None,
        },
    )
    .await
    .unwrap();

    // A link from a kept note to a junk file change must go with it.
    let junk_id: String = sqlx::query_scalar(
        "SELECT id FROM memories WHERE kind = 'file_change' AND files LIKE '%target-dev%'",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    sqlx::query("INSERT INTO memory_links (from_id, to_id, kind) VALUES ('mixed', ?, 'related')")
        .bind(&junk_id)
        .execute(db.pool())
        .await
        .unwrap();
    let cues_before = count(&db, "SELECT COUNT(*) FROM memory_cues").await;
    assert!(cues_before > 0, "file changes register cues");

    let mut excluded = |path: &str| {
        ["target", "target-dev", "node_modules", "test-results"]
            .iter()
            .any(|seg| path.split('/').any(|part| part == *seg))
    };
    let mut progress = Vec::new();
    let dry = prune_excluded_file_changes(db.pool(), &mut excluded, false, &mut |d, t| {
        progress.push((d, t))
    })
    .await
    .unwrap();
    assert!(dry.dry_run);
    assert_eq!(
        (dry.scanned, dry.matched, dry.purged, dry.kept),
        (5, 3, 0, 2)
    );
    assert!(progress.is_empty());
    assert_eq!(
        count(
            &db,
            "SELECT COUNT(*) FROM memories WHERE kind = 'file_change'"
        )
        .await,
        5,
        "a dry run deletes nothing"
    );

    let applied = prune_excluded_file_changes(db.pool(), &mut excluded, true, &mut |d, t| {
        progress.push((d, t))
    })
    .await
    .unwrap();
    assert_eq!((applied.matched, applied.purged, applied.kept), (3, 3, 2));
    assert_eq!(progress.last(), Some(&(3, 3)));

    let left: Vec<String> =
        sqlx::query_scalar("SELECT id FROM memories WHERE kind = 'file_change' ORDER BY id")
            .fetch_all(db.pool())
            .await
            .unwrap();
    let notes = count(
        &db,
        "SELECT COUNT(*) FROM memories WHERE kind != 'file_change'",
    )
    .await;
    let tombstones = count(&db, "SELECT COUNT(*) FROM memory_tombstones").await;
    let links = count(&db, "SELECT COUNT(*) FROM memory_links").await;
    let tombstoned_junk = count(
        &db,
        &format!("SELECT COUNT(*) FROM memory_tombstones WHERE memory_id = '{junk_id}'"),
    )
    .await;
    let orphan_cues = count(
        &db,
        "SELECT COUNT(*) FROM memory_cues WHERE memory_id NOT IN (SELECT id FROM memories)",
    )
    .await;
    db.close().await;

    assert_eq!(left.len(), 2);
    assert!(left.contains(&"mixed".to_string()));
    assert_eq!(notes, 1);
    assert_eq!(tombstones, 3);
    assert_eq!(orphan_cues, 0);
    assert_eq!(links, 0);
    assert_eq!(tombstoned_junk, 1);
}

#[tokio::test]
async fn purge_waits_for_another_writer_instead_of_failing() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax/ax.db"))
        .await
        .unwrap();
    let paths: Vec<String> = (0..1200).map(|i| format!("target/x{i}")).collect();
    // Above the burst limit save_file_changes still writes one row per path.
    save_file_changes(db.pool(), &paths, "conv", "turn", 1)
        .await
        .unwrap();
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM memories WHERE kind = 'file_change'")
        .fetch_all(db.pool())
        .await
        .unwrap();
    assert_eq!(ids.len(), 1200);

    // Another writer (a live MCP server, the watcher) holds the write lock.
    let mut other = db.pool().begin_with("BEGIN IMMEDIATE").await.unwrap();
    sqlx::query("UPDATE memories SET confidence = 0.9 WHERE id = ?")
        .bind(&ids[0])
        .execute(&mut *other)
        .await
        .unwrap();
    let mut on_batch = |_: usize, _: usize| {};
    let release = async {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        other.commit().await.unwrap();
    };
    let (purged, ()) = tokio::join!(
        ax_memory::purge_memories(db.pool(), &ids, &mut on_batch),
        release
    );
    let purged = purged.unwrap();

    let left = count(
        &db,
        "SELECT COUNT(*) FROM memories WHERE kind = 'file_change'",
    )
    .await;
    db.close().await;
    assert_eq!(purged, 1200);
    assert_eq!(left, 0);
}
