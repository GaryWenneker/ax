//! Ship routes that change the repository, run work or make the server call
//! another host must refuse read-only (share) viewers, and the SonarQube login
//! check may only reach the configured SonarQube or a loopback address.

use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use ax_web::WebHub;
use axum::body::{to_bytes, Body};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

async fn app(dir: &std::path::Path, readonly: bool) -> Router {
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::fs::write(
        root.join(".ax").join("ship.toml"),
        "[sonar]\nenabled = false\nhost = \"http://127.0.0.1:1\"\nproject_key = \"t\"\n\
         token_env = \"SONAR_TOKEN\"\nscanner_path = \"sonar-scanner\"\n",
    )
    .unwrap();
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_GLOBAL_DB", dir.join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, readonly, 7070).await.unwrap();
    hub.nest_routers(CorsLayer::new())
}

async fn post_json(app: &Router, uri: &str, body: serde_json::Value) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method("POST")
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .header("origin", "http://127.0.0.1:7070")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let response = tokio::time::timeout(Duration::from_secs(5), app.clone().oneshot(req))
        .await
        .expect("the route must answer without reaching another host")
        .unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_viewers_cannot_create_a_draft_pull_request() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), true).await;
    let (status, _) = post_json(
        &app,
        "/api/ship/command",
        serde_json::json!({"cmd":"draft","title":"x"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_viewers_cannot_run_the_sonar_login_check() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), true).await;
    let (status, _) = post_json(
        &app,
        "/api/ship/sonar/validate-login",
        serde_json::json!({"host":"http://127.0.0.1:1","admin_user":"a","admin_password":"b"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn sonar_login_check_only_reaches_the_configured_or_a_loopback_host() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), false).await;
    for host in [
        "http://10.0.0.5:8080",
        "http://192.168.1.1",
        "https://sonar.example.com",
        "http://169.254.169.254",
        "not a url",
    ] {
        let (status, body) = post_json(
            &app,
            "/api/ship/sonar/validate-login",
            serde_json::json!({"host":host,"admin_user":"a","admin_password":"b"}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{host}: {body}");
    }
    // The configured host and loopback addresses are checked (nothing listens
    // on port 1, so SonarQube is reported unreachable).
    for host in [
        "http://127.0.0.1:1",
        "http://127.0.0.1:1/",
        "http://localhost:1",
    ] {
        let (status, body) = post_json(
            &app,
            "/api/ship/sonar/validate-login",
            serde_json::json!({"host":host,"admin_user":"a","admin_password":"b"}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{host}: {body}");
        assert!(body.contains("\"reachable\":false"), "{host}: {body}");
    }
}
