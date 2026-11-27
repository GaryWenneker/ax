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

#[cfg(unix)]
#[tokio::test]
async fn failed_forced_rebuild_keeps_the_previous_graph() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, mut ax) = indexed_project().await;
    assert!(has_symbol(&ax, "kept_symbol_survives").await);
    // A file that cannot be read makes the rebuild fail halfway.
    let unreadable = dir.path().join("unreadable.rs");
    std::fs::write(&unreadable, "pub fn locked() {}\n").unwrap();
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&unreadable).is_ok() {
        eprintln!("running with permissions that can read a mode 000 file; skipping");
        return;
    }
    let result = ToolHandler::call_tool(&mut ax, "ax_index", json!({"force": true})).await;
    std::fs::set_permissions(&unreadable, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(result.is_err(), "the rebuild must report the failure");
    assert!(
        has_symbol(&ax, "kept_symbol_survives").await,
        "a failed forced rebuild must not leave the graph empty"
    );
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
