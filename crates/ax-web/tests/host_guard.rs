//! Command Center answers only for Host names it expects (stops DNS rebinding).
//! Separate process: these tests set environment variables.

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

async fn app(dir: &std::path::Path) -> Router {
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_GLOBAL_DB", dir.join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, false, 7070).await.unwrap();
    hub.nest_routers(CorsLayer::new())
}

fn request(uri: &str, host: &str) -> Request<Body> {
    let mut req = Request::builder()
        .uri(uri)
        .header("host", host)
        .body(Body::empty())
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    req
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn a_foreign_host_name_is_refused_everywhere() {
    let _lock = env_lock();
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path()).await;

    for uri in ["/api/version", "/api/memory", "/dav/", "/ax/", "/"] {
        let refused = app
            .clone()
            .oneshot(request(uri, "evil.example:7070"))
            .await
            .unwrap();
        assert_eq!(refused.status(), StatusCode::FORBIDDEN, "{uri}");
    }
    // A rebinding name that only looks like loopback is still foreign.
    let lookalike = app
        .clone()
        .oneshot(request("/api/version", "127.0.0.1.evil.example:7070"))
        .await
        .unwrap();
    assert_eq!(lookalike.status(), StatusCode::FORBIDDEN);

    for host in [
        "127.0.0.1:7070",
        "localhost:7070",
        "[::1]:7070",
        "LOCALHOST",
    ] {
        let ok = app
            .clone()
            .oneshot(request("/api/version", host))
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK, "{host}");
    }
    // A loopback bind does not answer for a LAN address.
    let lan = app
        .clone()
        .oneshot(request("/api/version", "192.168.1.5:7070"))
        .await
        .unwrap();
    assert_eq!(lan.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn configured_hosts_are_accepted() {
    let _lock = env_lock();
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    std::env::set_var("AX_WEB_AUTH_ORIGIN", "https://admin.example.com");
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".ax")).unwrap();
    std::fs::write(
        dir.path().join(".ax").join("config.json"),
        r#"{"web":{"allowed_hosts":["ax.internal.example"]}}"#,
    )
    .unwrap();
    let app = app(dir.path()).await;
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");

    for host in ["admin.example.com", "ax.internal.example:7070"] {
        let ok = app
            .clone()
            .oneshot(request("/api/version", host))
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK, "{host}");
    }
    let refused = app
        .oneshot(request("/api/version", "evil.example"))
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
}
