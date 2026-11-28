//! A workspace switch whose request is dropped half-way.
//! Separate process: this test sets environment variables.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ax_web::WebHub;

async fn project(dir: &Path, name: &str) -> PathBuf {
    let root = dir.join(name);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    root.canonicalize().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancelled_switch_does_not_block_later_switches() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();
    std::env::set_var("HOME", dir);
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_GLOBAL_DB", dir.join("global.db"));
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    let first = project(dir, "first").await;
    let second = project(dir, "second").await;
    let hub = WebHub::open(first.clone(), false, 0).await.unwrap();

    // A reader keeps the switch from taking the workspace, so the request is
    // dropped (as when the browser disconnects) while the switch waits.
    let reader = hub.read().await;
    let dropped = tokio::time::timeout(Duration::from_secs(3), hub.switch(second.clone())).await;
    assert!(dropped.is_err(), "the switch should still be waiting");
    drop(reader);

    // The switch that was under way finishes on its own, and the next one runs.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match hub.switch(first.clone()).await {
            Ok(_) => break,
            Err(error) if error.contains("already in progress") && Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Err(error) => panic!("later switch refused: {error}"),
        }
    }
    assert_eq!(hub.read().await.project_root, first);
}
