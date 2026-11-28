//! `ax_index force=true` rebuilds the graph without emptying it first.

use ax_extraction::orchestrator::IndexOptions;
use ax_mcp::tools::ToolHandler;
use serde_json::json;

async fn indexed_project() -> (tempfile::TempDir, ax_core::Ax) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("kept.rs"),
        "pub fn kept_symbol_survives() -> u32 { 1 }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("gone.rs"),
        "pub fn deleted_file_symbol() -> u32 { 2 }\n",
    )
    .unwrap();
    let mut ax = ax_core::Ax::init(root).await.unwrap();
    ax.index_all(
        IndexOptions {
            quiet: true,
            ..Default::default()
        },
        None,
    )
    .await
    .unwrap();
    (dir, ax)
}

async fn has_symbol(ax: &ax_core::Ax, name: &str) -> bool {
    !ax.queries()
        .get_nodes_by_lower_name(name)
        .await
        .unwrap()
        .is_empty()
}

/// An unreadable file is skipped (index-core review fix), so it no longer makes a
/// forced rebuild fail; the rebuild still keeps every other symbol. A rebuild that does
/// fail drops nothing, because stale rows are removed only after a successful scan.
#[cfg(unix)]
#[tokio::test]
async fn forced_rebuild_with_an_unreadable_file_keeps_the_graph() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, mut ax) = indexed_project().await;
    assert!(has_symbol(&ax, "kept_symbol_survives").await);
    let unreadable = dir.path().join("unreadable.rs");
    std::fs::write(&unreadable, "pub fn locked() {}\n").unwrap();
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&unreadable).is_ok() {
        eprintln!("running with permissions that can read a mode 000 file; skipping");
        return;
    }
    let result = ToolHandler::call_tool(&mut ax, "ax_index", json!({"force": true})).await;
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(result.is_ok(), "an unreadable file is skipped: {result:?}");
    assert!(
        has_symbol(&ax, "kept_symbol_survives").await,
        "a forced rebuild must not leave the graph empty"
    );
    assert!(!has_symbol(&ax, "locked").await);
}

#[tokio::test]
async fn forced_rebuild_drops_files_that_are_gone() {
    let (dir, mut ax) = indexed_project().await;
    assert!(has_symbol(&ax, "deleted_file_symbol").await);
    std::fs::remove_file(dir.path().join("gone.rs")).unwrap();
    ToolHandler::call_tool(&mut ax, "ax_index", json!({"force": true}))
        .await
        .unwrap();
    assert!(has_symbol(&ax, "kept_symbol_survives").await);
    assert!(
        !has_symbol(&ax, "deleted_file_symbol").await,
        "a forced rebuild must not keep symbols of deleted files"
    );
    let paths: Vec<String> = ax
        .queries()
        .get_all_files()
        .await
        .unwrap()
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert!(!paths.iter().any(|p| p == "gone.rs"), "{paths:?}");
}

async fn seed_replica_file(ax: &ax_core::Ax) -> String {
    let path = "ax-replica/a_1/src/remote.rs".to_string();
    ax.queries()
        .upsert_file(&ax_types::FileRecord {
            path: path.clone(),
            content_hash: "replica".into(),
            language: ax_types::Language::Rust,
            size: 1,
            modified_at: 0,
            indexed_at: 0,
            node_count: 0,
            errors: None,
        })
        .await
        .unwrap();
    path
}

async fn has_file(ax: &ax_core::Ax, path: &str) -> bool {
    ax.queries()
        .get_all_files()
        .await
        .unwrap()
        .iter()
        .any(|f| f.path == path)
}

#[tokio::test]
async fn forced_rebuild_keeps_replicated_files() {
    let (_dir, mut ax) = indexed_project().await;
    let replica = seed_replica_file(&ax).await;
    ToolHandler::call_tool(&mut ax, "ax_index", json!({"force": true}))
        .await
        .unwrap();
    assert!(
        has_file(&ax, &replica).await,
        "a forced rebuild must keep replicated rows"
    );
}

#[tokio::test]
async fn full_index_keeps_replicated_files() {
    let (_dir, mut ax) = indexed_project().await;
    let replica = seed_replica_file(&ax).await;
    ax.index_all(
        IndexOptions {
            quiet: true,
            ..Default::default()
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        has_file(&ax, &replica).await,
        "a full index must keep replicated rows"
    );
    assert!(has_symbol(&ax, "kept_symbol_survives").await);
}
