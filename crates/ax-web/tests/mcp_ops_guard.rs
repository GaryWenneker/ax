//! MCP daemon reload and the quality audit change state, so read-only mode
//! and other websites cannot reach them, and the audit reads only agent
//! transcripts, never an arbitrary file named by the caller.
//! `HOME` is process-global, so every scenario runs under one lock.

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

async fn app(home: &Path, readonly: bool) -> Router {
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
    // Daemon metadata that a reload would delete.
    std::fs::write(daemon_info(home), "{}").unwrap();
    let hub = WebHub::open(root, readonly, 7070).await.unwrap();
    hub.nest_routers(CorsLayer::new())
}

fn daemon_info(home: &Path) -> std::path::PathBuf {
    home.join("proj")
        .join(".ax")
        .join(ax_mcp::daemon::DAEMON_INFO_FILE)
}

async fn post(
    app: &Router,
    uri: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("host", "127.0.0.1:7070");
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let mut req = req.body(Body::from(body.to_string())).unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

const RELOAD: &str = "/api/ops/mcp-reload";
const AUDIT: &str = "/api/usage/mcp-audit";
const JSON: (&str, &str) = ("content-type", "application/json");

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_mode_refuses_reload_and_audit() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path(), true).await;

    let (status, body) = post(&app, RELOAD, &[], "").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(daemon_info(home.path()).exists(), "reload ran");

    let (status, body) = post(&app, AUDIT, &[JSON], "{}").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn another_website_cannot_reload_the_daemon() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path(), false).await;

    // A plain HTML form on another site: a CORS-simple request with no preflight.
    let (status, body) = post(
        &app,
        RELOAD,
        &[
            ("origin", "https://evil.example"),
            ("content-type", "application/x-www-form-urlencoded"),
            ("sec-fetch-site", "cross-site"),
        ],
        "",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(daemon_info(home.path()).exists(), "reload ran");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn audit_refuses_arbitrary_file_paths() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path(), false).await;

    let secret = home.path().join("id_ed25519");
    std::fs::write(&secret, "-----BEGIN OPENSSH PRIVATE KEY-----\n").unwrap();
    for session in [secret.display().to_string(), "../../id_ed25519".to_string()] {
        let body = serde_json::json!({ "session": session }).to_string();
        let (status, text) = post(&app, AUDIT, &[JSON], &body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{session}: {text}");
    }

    // A plain session id is still accepted.
    let body = serde_json::json!({ "session": "abc-123" }).to_string();
    let (status, text) = post(&app, AUDIT, &[JSON], &body).await;
    assert_eq!(status, StatusCode::OK, "{text}");
}
