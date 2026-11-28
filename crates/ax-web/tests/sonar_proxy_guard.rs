//! The SonarQube UI proxy forwards requests with the configured admin
//! credentials, so it must refuse writes in read-only mode and writes that do
//! not come from a page served by this Command Center.

use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard};

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

const PROXY: &str = "/api/ship/sonar/ui/api/users/change_password";

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

async fn app(dir: &std::path::Path, readonly: bool) -> Router {
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    // Point the proxy at a closed port so no request can reach a real
    // SonarQube on this computer.
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

async fn send(app: &Router, method: &str, headers: &[(&str, &str)]) -> StatusCode {
    let mut builder = Request::builder()
        .method(method)
        .uri(PROXY)
        .header("host", "127.0.0.1:7070")
        .header("content-type", "application/x-www-form-urlencoded");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut req = builder
        .body(Body::from("login=admin&password=attacker-pass"))
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    app.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_mode_refuses_proxied_sonar_writes() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), true).await;
    for method in ["POST", "PUT", "PATCH", "DELETE"] {
        let status = send(&app, method, &[("origin", "http://127.0.0.1:7070")]).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method}");
    }
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn cross_site_or_origin_less_sonar_writes_are_refused() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path(), false).await;
    // A form posted from another website.
    assert_eq!(
        send(
            &app,
            "POST",
            &[
                ("origin", "https://evil.example"),
                ("sec-fetch-site", "cross-site")
            ]
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(&app, "POST", &[("origin", "https://evil.example")]).await,
        StatusCode::FORBIDDEN
    );
    // No Origin at all: not provably from this Command Center.
    assert_eq!(send(&app, "POST", &[]).await, StatusCode::FORBIDDEN);
    assert_eq!(
        send(&app, "POST", &[("origin", "null")]).await,
        StatusCode::FORBIDDEN
    );
    // Same site but not same origin.
    assert_eq!(
        send(
            &app,
            "POST",
            &[
                ("origin", "http://127.0.0.1:7070"),
                ("sec-fetch-site", "same-site")
            ]
        )
        .await,
        StatusCode::FORBIDDEN
    );
    // The Command Center's own page passes the guard (the upstream is not
    // running in this test, so the proxy answers 502).
    let status = send(
        &app,
        "POST",
        &[
            ("origin", "http://127.0.0.1:7070"),
            ("sec-fetch-site", "same-origin"),
        ],
    )
    .await;
    assert_ne!(status, StatusCode::FORBIDDEN);
    // Reads keep working without an Origin.
    let mut req = Request::builder()
        .uri("/api/ship/sonar/ui/api/server/version")
        .header("host", "127.0.0.1:7070")
        .body(Body::empty())
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    assert_ne!(
        app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}
