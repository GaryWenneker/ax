use ax_db::graph_history::{
    record_edge_version, record_node_version, GraphEdgeVersion, GraphNodeVersion,
};
use ax_db::Database;

#[tokio::test]
async fn recording_a_node_version_stores_the_body_once() {
    let root = std::env::temp_dir().join(format!("ax-graph-hist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = Database::open(&root.join(".ax/ax.db")).await.unwrap();

    let empty = record_node_version(
        db.pool(),
        &GraphNodeVersion {
            node_key: "".into(),
            body: "pub fn kept() {}".into(),
            signature: "kept".into(),
            kind: "function".into(),
            file_path: "src/kept.rs".into(),
            start_line: 1,
            end_line: 1,
            valid_from: 10,
            commit_id: "".into(),
            session_id: "conv-1".into(),
            turn_id: "gen-1".into(),
        },
    )
    .await
    .unwrap();
    assert!(!empty);

    let first = record_node_version(
        db.pool(),
        &GraphNodeVersion {
            node_key: "kept".into(),
            body: "pub fn kept() {}".into(),
            signature: "kept".into(),
            kind: "function".into(),
            file_path: "src/kept.rs".into(),
            start_line: 1,
            end_line: 1,
            valid_from: 10,
            commit_id: "".into(),
            session_id: "conv-1".into(),
            turn_id: "gen-1".into(),
        },
    )
    .await
    .unwrap();
    assert!(first);

    let second = record_node_version(
        db.pool(),
        &GraphNodeVersion {
            node_key: "kept".into(),
            body: "pub fn kept() {}".into(),
            signature: "kept".into(),
            kind: "function".into(),
            file_path: "src/kept.rs".into(),
            start_line: 1,
            end_line: 1,
            valid_from: 20,
            commit_id: "".into(),
            session_id: "conv-1".into(),
            turn_id: "gen-2".into(),
        },
    )
    .await
    .unwrap();
    assert!(second);

    let versions: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT version, body_hash, turn_id FROM graph_node_versions WHERE node_key = 'kept' ORDER BY version",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    let blobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM graph_body_blobs")
        .fetch_one(db.pool())
        .await
        .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0].0, 1);
    assert_eq!(versions[1].0, 2);
    assert_eq!(versions[0].1, versions[1].1);
    assert_eq!(versions[0].2, "gen-1");
    assert_eq!(versions[1].2, "gen-2");
    assert_eq!(blobs, 1);
}

#[tokio::test]
async fn recording_an_edge_version_closes_the_previous_row() {
    let root = std::env::temp_dir().join(format!("ax-graph-edge-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = Database::open(&root.join(".ax/ax.db")).await.unwrap();

    let empty = record_edge_version(
        db.pool(),
        &GraphEdgeVersion {
            source_key: "".into(),
            target_key: "b".into(),
            kind: "calls".into(),
            valid_from: 10,
            commit_id: "".into(),
            session_id: "conv-1".into(),
            turn_id: "gen-1".into(),
        },
    )
    .await
    .unwrap();
    assert!(!empty);

    let first = record_edge_version(
        db.pool(),
        &GraphEdgeVersion {
            source_key: "a".into(),
            target_key: "b".into(),
            kind: "calls".into(),
            valid_from: 10,
            commit_id: "".into(),
            session_id: "conv-1".into(),
            turn_id: "gen-1".into(),
        },
    )
    .await
    .unwrap();
    assert!(first);

    let second = record_edge_version(
        db.pool(),
        &GraphEdgeVersion {
            source_key: "a".into(),
            target_key: "b".into(),
            kind: "calls".into(),
            valid_from: 20,
            commit_id: "".into(),
            session_id: "conv-1".into(),
            turn_id: "gen-2".into(),
        },
    )
    .await
    .unwrap();
    assert!(second);

    let rows: Vec<(i64, Option<i64>, String)> = sqlx::query_as(
        "SELECT version, valid_to, turn_id FROM graph_edge_versions
         WHERE source_key = 'a' AND target_key = 'b' AND kind = 'calls'
         ORDER BY version",
    )
    .fetch_all(db.pool())
    .await
    .unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, 1);
    assert_eq!(rows[0].1, Some(20));
    assert_eq!(rows[0].2, "gen-1");
    assert_eq!(rows[1].0, 2);
    assert_eq!(rows[1].1, None);
    assert_eq!(rows[1].2, "gen-2");
}

#[tokio::test]
async fn node_as_of_returns_the_version_valid_at_that_time() {
    use ax_db::graph_history::{node_as_of, record_node_version, GraphNodeVersion};

    let root = std::env::temp_dir().join(format!("ax-asof-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = Database::open(&root.join(".ax/ax.db")).await.unwrap();

    record_node_version(
        db.pool(),
        &GraphNodeVersion {
            node_key: "kept".into(),
            body: "v1".into(),
            signature: "kept".into(),
            kind: "function".into(),
            file_path: "src/kept.rs".into(),
            start_line: 1,
            end_line: 1,
            valid_from: 10,
            commit_id: "".into(),
            session_id: "conv-1".into(),
            turn_id: "gen-1".into(),
        },
    )
    .await
    .unwrap();
    record_node_version(
        db.pool(),
        &GraphNodeVersion {
            node_key: "kept".into(),
            body: "v2".into(),
            signature: "kept".into(),
            kind: "function".into(),
            file_path: "src/kept.rs".into(),
            start_line: 1,
            end_line: 1,
            valid_from: 20,
            commit_id: "".into(),
            session_id: "conv-1".into(),
            turn_id: "gen-2".into(),
        },
    )
    .await
    .unwrap();

    let before = node_as_of(db.pool(), "kept", 5).await.unwrap();
    let mid = node_as_of(db.pool(), "kept", 15).await.unwrap().unwrap();
    let late = node_as_of(db.pool(), "kept", 25).await.unwrap().unwrap();
    db.close().await;
    let _ = std::fs::remove_dir_all(&root);

    assert!(before.is_none());
    assert_eq!(mid.version, 1);
    assert_eq!(late.version, 2);
}
