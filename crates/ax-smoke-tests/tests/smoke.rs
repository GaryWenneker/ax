//! Integration smoke tests against test-smoke fixture.

use std::path::PathBuf;

use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;
use ax_types::SearchOptions;

fn smoke_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-smoke")
}

#[tokio::test]
async fn smoke_init_index_and_search() {
    let root = smoke_root();
    let mut ax = if root.join(".ax").exists() {
        Ax::open(&root).await.expect("open test-smoke")
    } else {
        Ax::init(&root).await.expect("init test-smoke")
    };

    if ax.get_stats().await.map(|s| s.node_count).unwrap_or(0) == 0 {
        ax.sync(IndexOptions::default()).await.expect("sync index");
    }

    let stats = ax.get_stats().await.expect("stats");
    assert!(stats.node_count >= 1, "expected indexed nodes");

    let nodes = ax
        .search_nodes("greet", &SearchOptions::default())
        .await
        .expect("search");
    assert!(!nodes.is_empty(), "expected greet symbol in index");
}