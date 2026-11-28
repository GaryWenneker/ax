//! WebHub open and hot switch between two real projects, under an isolated
//! home. Separate process: this test sets environment variables.

use std::path::{Path, PathBuf};

use ax_web::WebHub;

mod common;

async fn project(dir: &Path, name: &str) -> PathBuf {
    let root = dir.join(name);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    root.canonicalize().unwrap()
}

async fn served(hub: &WebHub) -> PathBuf {
    let ws = hub.read().await;
    assert!(
        ws.graph_pool.acquire().await.is_ok(),
        "graph pool is usable"
    );
    ws.project_root
        .canonicalize()
        .unwrap_or_else(|_| ws.project_root.clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hub_open_and_hot_switch() {
    let home = common::isolate_home();
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    let dir = tempfile::tempdir().unwrap();
    let first = project(dir.path(), "first").await;
    let second = project(dir.path(), "second").await;

    let hub = WebHub::open(first.clone(), false, 0)
        .await
        .expect("WebHub::open");
    assert_eq!(served(&hub).await, first);

    let info = hub.switch(second.clone()).await.expect("switch to second");
    assert!(info.path.contains("second"), "{}", info.path);
    assert_eq!(info.label, "second");
    assert_eq!(served(&hub).await, second);

    hub.switch(first.clone()).await.expect("switch back");
    assert_eq!(served(&hub).await, first);

    // The switch records the project in the isolated home's recent list.
    let recent = std::fs::read_to_string(home.join(".ax").join("config.json")).unwrap_or_default();
    assert!(recent.contains("second"), "{recent}");
}
