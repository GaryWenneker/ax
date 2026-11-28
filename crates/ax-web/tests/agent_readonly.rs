//! Agent routes that change state or run an agent obey read-only mode, and a
//! chat run (which starts an agent with tool access) needs the local browser.
//! `HOME` is process-global, so every scenario runs under one lock.

use std::net::SocketAddr;
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

async fn app(home: &std::path::Path, readonly: bool) -> Router {
    std::env::set_var("HOME", home);
    std::env::set_var("AX_HOME_DIR", home);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::set_var("AX_GLOBAL_DB", home.join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    let root = home.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, readonly, 7070).await.unwrap();
    hub.nest_routers(CorsLayer::new())
}

async fn send(app: &Router, method: &str, uri: &str, peer: &str, body: &str) -> StatusCode {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let peer: SocketAddr = peer.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    app.clone().oneshot(req).await.unwrap().status()
}

const LOOPBACK: &str = "127.0.0.1:50000";
const CHAT: &str = "/api/agent/chat/stream";
const CHAT_BODY: &str = r#"{"prompt":"hello","agent":"builtin"}"#;

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_mode_refuses_profile_delete_and_chat() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    std::env::set_var("HOME", home.path());
    std::env::set_var("AX_HOME_DIR", home.path());
    ax_agent::profiles::create_profile("claude", "work", "Work", None, None, None).unwrap();
    let app = app(home.path(), true).await;

    assert_eq!(
        send(
            &app,
            "DELETE",
            "/api/agent/profiles/claude/work",
            LOOPBACK,
            ""
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert!(ax_agent::profiles::list_profiles("claude")
        .iter()
        .any(|p| p.id == "work"));
    assert_eq!(
        send(&app, "POST", CHAT, LOOPBACK, CHAT_BODY).await,
        StatusCode::FORBIDDEN
    );
    // Reads still work.
    assert_eq!(
        send(&app, "GET", "/api/agent/profiles", LOOPBACK, "").await,
        StatusCode::OK
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn chat_needs_the_local_browser() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path(), false).await;
    assert_eq!(
        send(&app, "POST", CHAT, "203.0.113.7:50000", CHAT_BODY).await,
        StatusCode::FORBIDDEN
    );
}
