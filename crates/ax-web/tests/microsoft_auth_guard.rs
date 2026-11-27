//! Microsoft sign-in setup needs the share token, a writable server and the
//! local owner's browser. Separate process: these tests set environment
//! variables.

use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard};

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

const LOOPBACK: &str = "127.0.0.1:50000";
const REMOTE: &str = "203.0.113.7:50000";

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

async fn app(dir: &std::path::Path, readonly: bool) -> Router {
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_GLOBAL_DB", dir.join("global.db"));
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    WebHub::open(root, readonly, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new())
}

fn request(method: &str, uri: &str, peer: &str, token: Option<&str>) -> Request<Body> {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .header("content-type", "application/json");
    if let Some(token) = token {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let body = if method == "PUT" {
        Body::from(r#"{"clientId":"attacker-client"}"#)
    } else {
        Body::empty()
    };
    let mut req = builder.body(body).unwrap();
    let peer: SocketAddr = peer.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    req
}

async fn status(app: &Router, req: Request<Body>) -> StatusCode {
    app.clone().oneshot(req).await.unwrap().status()
}

fn config_written(dir: &std::path::Path) -> bool {
    std::fs::read_to_string(dir.join(".ax").join("config.json"))
        .is_ok_and(|text| text.contains("attacker-client"))
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn a_share_session_cannot_change_microsoft_sign_in() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("AX_SHARE_TOKEN", "share-secret");
    let app = app(dir.path(), true).await;

    for (method, uri) in [
        ("DELETE", "/api/auth/microsoft"),
        ("PUT", "/api/auth/microsoft/config"),
        ("GET", "/api/auth/microsoft/status"),
        ("POST", "/api/auth/microsoft/device/start"),
    ] {
        assert_eq!(
            status(&app, request(method, uri, REMOTE, None)).await,
            StatusCode::UNAUTHORIZED,
            "{method} {uri} without the share token"
        );
    }
    // A viewer with the token is read-only.
    for (method, uri) in [
        ("DELETE", "/api/auth/microsoft"),
        ("PUT", "/api/auth/microsoft/config"),
        ("POST", "/api/auth/microsoft/device/start"),
        ("POST", "/api/auth/microsoft/device/poll"),
    ] {
        assert_eq!(
            status(&app, request(method, uri, REMOTE, Some("share-secret"))).await,
            StatusCode::FORBIDDEN,
            "{method} {uri} in a read-only share"
        );
    }
    std::env::remove_var("AX_SHARE_TOKEN");
    assert!(!config_written(dir.path()));
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn only_the_local_owner_changes_microsoft_sign_in() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    std::env::remove_var("AX_SHARE_TOKEN");
    let app = app(dir.path(), false).await;

    assert_eq!(
        status(
            &app,
            request("PUT", "/api/auth/microsoft/config", REMOTE, None)
        )
        .await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        status(&app, request("DELETE", "/api/auth/microsoft", REMOTE, None)).await,
        StatusCode::FORBIDDEN
    );
    assert!(!config_written(dir.path()));

    assert_eq!(
        status(
            &app,
            request("PUT", "/api/auth/microsoft/config", LOOPBACK, None)
        )
        .await,
        StatusCode::OK
    );
    assert!(config_written(dir.path()));
    std::env::remove_var("AX_MS_CLIENT_ID");
}

/// Changing the process environment while other request threads read it is a
/// data race; the saved client id is read from the config file instead.
#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn saving_the_client_id_leaves_the_process_environment_alone() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_MS_CLIENT_ID");
    let app = app(dir.path(), false).await;

    assert_eq!(
        status(
            &app,
            request("PUT", "/api/auth/microsoft/config", LOOPBACK, None)
        )
        .await,
        StatusCode::OK
    );
    let leaked = std::env::var("AX_MS_CLIENT_ID");
    std::env::remove_var("AX_MS_CLIENT_ID");
    assert!(leaked.is_err(), "environment changed: {leaked:?}");
    assert_eq!(
        ax_share::auth::microsoft::ms_client_id().unwrap(),
        "attacker-client"
    );
    assert!(ax_share::microsoft_auth_status().custom_client_id);
}
