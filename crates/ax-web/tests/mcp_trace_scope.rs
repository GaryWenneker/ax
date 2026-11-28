//! The MCP trace follows another recent project's verbose log only for the
//! local owner's own writable Command Center, never for a read-only (share)
//! viewer. `HOME` is process-global, so every scenario runs under one lock.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

const SECRET: &str = "ax_explore prompt=confidential-project-b";

/// Project `a` (served, no verbose log) and a recent project `b` whose
/// verbose log holds [`SECRET`].
async fn app(home: &Path, readonly: bool) -> Router {
    std::env::set_var("HOME", home);
    std::env::set_var("AX_HOME_DIR", home);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::set_var("AX_GLOBAL_DB", home.join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    let a = home.join("a");
    let b = home.join("b");
    for root in [&a, &b] {
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        ax_db::Database::open(&root.join(".ax").join("ax.db"))
            .await
            .unwrap();
    }
    std::fs::write(
        b.join(".ax").join("mcp-verbose-2020-01-02.log"),
        format!("{SECRET}\n"),
    )
    .unwrap();
    ax_agent::config::touch_recent_project(&b, true).unwrap();
    let hub = WebHub::open(a, readonly, 7070).await.unwrap();
    hub.nest_routers(CorsLayer::new())
}

async fn get(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .body(Body::empty())
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_viewer_never_follows_another_project() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path(), true).await;

    let (status, meta) = get(&app, "/api/usage/mcp-trace/path").await;
    assert_eq!(status, StatusCode::OK, "{meta}");
    assert_eq!(meta["followLabel"], "a", "{meta}");
    assert!(meta["fallbackLabel"].is_null(), "{meta}");

    let (status, chunk) = get(&app, "/api/usage/mcp-trace/chunk?day=2030-01-01").await;
    assert_eq!(status, StatusCode::OK, "{chunk}");
    assert!(!chunk.to_string().contains(SECRET), "leaked: {chunk}");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn local_owner_still_follows_the_active_project() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path(), false).await;

    let (_, meta) = get(&app, "/api/usage/mcp-trace/path").await;
    assert_eq!(meta["followLabel"], "b", "{meta}");
    let (_, chunk) = get(&app, "/api/usage/mcp-trace/chunk?day=2030-01-01").await;
    assert!(chunk.to_string().contains(SECRET), "{chunk}");
}
