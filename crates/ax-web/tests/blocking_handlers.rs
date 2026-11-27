//! Handlers that run git or other slow synchronous work must not block the
//! async runtime: other requests and timers keep running meanwhile. Each test
//! uses a single-threaded runtime and a fake `git` that sleeps, so blocking
//! work on the runtime thread would stall the timer below. `PATH` and `HOME`
//! are process-global, so every scenario runs under one lock.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

const GIT_DELAY: Duration = Duration::from_secs(3);

async fn app(home: &Path) -> (Router, WebHub) {
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
    let other = home.join("other");
    std::fs::create_dir_all(other.join(".ax")).unwrap();
    ax_db::Database::open(&other.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, false, 7070).await.unwrap();
    (hub.nest_routers(CorsLayer::new()), hub)
}

/// Puts a `git` first on `PATH` that sleeps and then fails.
#[cfg(unix)]
fn slow_git(home: &Path) -> String {
    use std::os::unix::fs::PermissionsExt;
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let git = bin.join("git");
    std::fs::write(
        &git,
        format!("#!/bin/sh\nsleep {}\nexit 1\n", GIT_DELAY.as_secs_f32()),
    )
    .unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let old = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{old}", bin.display()));
    old
}

async fn send(app: &Router, method: &str, uri: &str, body: &str) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, json)
}

/// Starts `uri` in the background and checks that a short timer and a
/// workspace switch (which takes the hub write lock) both complete while the
/// request runs.
async fn assert_does_not_block(
    app: &Router,
    hub: &WebHub,
    home: &Path,
    method: &str,
    uri: &str,
    body: &str,
) -> (StatusCode, Value) {
    let task = {
        let app = app.clone();
        let (method, uri, body) = (method.to_string(), uri.to_string(), body.to_string());
        tokio::spawn(async move { send(&app, &method, &uri, &body).await })
    };
    // Let the request start and reach its git call. Blocking work on this
    // single runtime thread would hold this short sleep up until git ends.
    let started = Instant::now();
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    let timer = started.elapsed();
    assert!(
        timer < GIT_DELAY / 2,
        "{uri} blocked the runtime for {timer:?}"
    );
    assert!(!task.is_finished(), "{uri} finished before its git call");
    let switched = tokio::time::timeout(GIT_DELAY / 2, hub.switch(home.join("other"))).await;
    let (status, response) = task.await.unwrap();
    if let Ok(result) = &switched {
        result.as_ref().expect("switch to the other project");
    }
    hub.switch(home.join("proj")).await.expect("switch back");
    assert!(
        switched.is_ok(),
        "{uri} held the workspace lock while git ran"
    );
    (status, response)
}

#[cfg(unix)]
#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn memory_git_routes_run_off_the_runtime() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let (app, hub) = app(home.path()).await;
    let (status, created) = send(
        &app,
        "POST",
        "/api/memory",
        r#"{"title":"t","body":"Did a thing\n\nCommits:\n- abcdef1 message"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id = created["memory"]["id"].as_str().unwrap().to_string();
    let old_path = slow_git(home.path());
    for route in ["diff", "file-changes", "file-links"] {
        let uri = format!("/api/memory/{id}/{route}");
        let (status, body) = assert_does_not_block(&app, &hub, home.path(), "GET", &uri, "").await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
    std::env::set_var("PATH", old_path);
}

#[cfg(unix)]
#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn wiki_publish_and_docs_sync_run_off_the_runtime() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let (app, hub) = app(home.path()).await;
    std::fs::write(
        home.path().join("proj/ax.json"),
        r#"{"okf":{"azdoWiki":{"enabled":true,"remote":"https://example.invalid/wiki.git"}},
            "docsCatalog":{"wiki_remote":"https://example.invalid/docs.wiki.git"}}"#,
    )
    .unwrap();
    let (status, body) = send(&app, "POST", "/api/okf/export", "{}").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let old_path = slow_git(home.path());

    // The fake git fails, so both report an error, but only after git ran.
    let (status, body) =
        assert_does_not_block(&app, &hub, home.path(), "POST", "/api/okf/publish", "{}").await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body.to_string().contains("git clone failed"), "{body}");
    let (status, body) = assert_does_not_block(
        &app,
        &hub,
        home.path(),
        "POST",
        "/api/docs-catalog/sync",
        "{}",
    )
    .await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{body}");
    assert!(body.to_string().contains("git clone failed"), "{body}");
    std::env::set_var("PATH", old_path);
}
