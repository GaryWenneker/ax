//! WebHub smoke test against test-smoke fixture.

use std::path::PathBuf;

use ax_web::WebHub;

fn smoke_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test-smoke")
}

#[tokio::test]
async fn hub_open_and_hot_switch() {
    let root = smoke_root();
    let db = root.join(".ax").join("ax.db");
    if !db.exists() {
        eprintln!("skip hub_open_and_hot_switch: no test-smoke index at {}", db.display());
        return;
    }

    let hub = WebHub::open(root.clone(), false, 0)
        .await
        .expect("WebHub::open");

    {
        let ws = hub.read().await;
        assert!(ws.graph_pool.acquire().await.is_ok());
        assert_eq!(ws.project_root, root);
    }

    let target = root.canonicalize().unwrap_or(root.clone());
    let info = hub.switch(target.clone()).await.expect("hot switch");
    assert!(info.path.contains(
        target
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("test-smoke")
    ));

    let ws = hub.read().await;
    assert_eq!(
        ws.project_root.canonicalize().unwrap_or(ws.project_root.clone()),
        target
    );
}
