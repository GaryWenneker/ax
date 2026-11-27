//! Purging the database of the project that is currently open.
//! Separate process: this test sets environment variables.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

async fn project(dir: &Path, name: &str) -> PathBuf {
    let root = dir.join(name);
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    root.canonicalize().unwrap()
}

async fn send(app: &Router, method: &str, uri: &str, body: &str) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn purging_the_open_project_closes_it_and_says_so() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    std::env::set_var("HOME", &dir);
    std::env::set_var("AX_HOME_DIR", &dir);
    std::env::set_var("AX_GLOBAL_DB", dir.join("global.db"));
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    let first = project(&dir, "first").await;
    let second = project(&dir, "second").await;
    let hub = WebHub::open(first.clone(), false, 7070).await.unwrap();
    let app = hub.nest_routers(CorsLayer::new());

    let (status, _) = send(&app, "GET", "/api/stats", "").await;
    assert_eq!(status, StatusCode::OK);

    let body = serde_json::json!({ "path": first.to_string_lossy(), "groups": ["database"] });
    let (status, text) = send(&app, "POST", "/api/workspace/purge", &body.to_string()).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let reply: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(reply["disconnected"], true, "{text}");

    // Every handle on ax.db is gone, not only the graph pool.
    {
        let bundle = hub.read().await;
        assert!(bundle.graph_pool.is_closed());
        assert!(
            bundle.policy.store.pool().is_closed(),
            "the policy pool still holds ax.db open"
        );
    }

    // Project routes answer with a clear error instead of a closed-pool failure.
    let (status, text) = send(&app, "GET", "/api/stats", "").await;
    assert_eq!(status, StatusCode::CONFLICT, "{text}");
    assert!(text.contains("No project open"), "{text}");
    let (status, text) = send(&app, "GET", "/api/memory/", "").await;
    assert_eq!(status, StatusCode::CONFLICT, "{text}");

    // The workspace routes still work, so the browser can open another project.
    let (status, _) = send(&app, "GET", "/api/workspace/current", "").await;
    assert_eq!(status, StatusCode::OK);
    let body = serde_json::json!({ "path": second.to_string_lossy() });
    let (status, text) = send(&app, "POST", "/api/workspace/switch", &body.to_string()).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let (status, text) = send(&app, "GET", "/api/stats", "").await;
    assert_eq!(status, StatusCode::OK, "{text}");
}
