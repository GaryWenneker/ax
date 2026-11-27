//! Who may use the workspace routes (browse, switch, create, init, purge,
//! recent list) with web sign-in off.
//! Separate process: these tests set environment variables.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

const LOOPBACK: &str = "127.0.0.1:50000";
const LAN: &str = "198.51.100.50:4000";

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

struct Fixture {
    _dir: tempfile::TempDir,
    dir: PathBuf,
    app: Router,
}

async fn fixture(readonly: bool) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::env::set_var("HOME", &dir);
    std::env::set_var("AX_HOME_DIR", &dir);
    std::env::set_var("AX_GLOBAL_DB", dir.join("global.db"));
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let app = WebHub::open(root, readonly, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new());
    Fixture {
        _dir: tmp,
        dir,
        app,
    }
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    peer: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut req = builder.body(Body::from(body.to_string())).unwrap();
    let peer: SocketAddr = peer.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn recent_list(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join(".ax").join("config.json")).unwrap_or_default()
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn only_the_local_browser_uses_workspace_routes_without_sign_in() {
    let _lock = env_lock();
    let fx = fixture(false).await;
    let browse = format!("/api/workspace/browse?path={}", fx.dir.display());
    let mkdir = serde_json::json!({"parent": fx.dir, "name": "made"}).to_string();
    let add = serde_json::json!({"path": fx.dir.join("elsewhere")}).to_string();
    let foreign = [("origin", "http://evil.example")];
    let proxied = [("x-forwarded-for", "203.0.113.9")];
    for (peer, headers) in [
        (LAN, &[][..]),
        (LOOPBACK, &foreign[..]),
        (LOOPBACK, &proxied[..]),
    ] {
        for (method, uri, body) in [
            ("GET", browse.as_str(), ""),
            ("GET", "/api/workspace/current", ""),
            ("POST", "/api/workspace/mkdir", mkdir.as_str()),
            ("POST", "/api/workspace/recent/add", add.as_str()),
        ] {
            let (status, text) = send(&fx.app, method, uri, peer, headers, body).await;
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "{peer} {headers:?} {method} {uri}: {text}"
            );
        }
    }
    assert!(!fx.dir.join("made").exists());
    assert!(!recent_list(&fx.dir).contains("elsewhere"));

    // The person at this computer, in the Command Center page.
    let own = [("origin", "http://127.0.0.1:7070")];
    let (status, text) = send(&fx.app, "GET", &browse, LOOPBACK, &[], "").await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(text.contains("\"ok\":true"), "{text}");
    let (status, text) = send(
        &fx.app,
        "POST",
        "/api/workspace/mkdir",
        LOOPBACK,
        &own,
        &mkdir,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(fx.dir.join("made").is_dir());

    // Any folder may be browsed: the picker walks the whole disk.
    let (_, text) = send(
        &fx.app,
        "GET",
        "/api/workspace/browse?path=/",
        LOOPBACK,
        &[],
        "",
    )
    .await;
    assert!(text.contains("\"ok\":true"), "{text}");
    // A relative path would resolve against the server's working directory.
    let relative = serde_json::json!({"parent": "relative-parent", "name": "x"}).to_string();
    let (_, text) = send(
        &fx.app,
        "POST",
        "/api/workspace/mkdir",
        LOOPBACK,
        &own,
        &relative,
    )
    .await;
    assert!(text.contains("must be absolute"), "{text}");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_mode_does_not_change_the_recent_list() {
    let _lock = env_lock();
    let fx = fixture(true).await;
    let add = serde_json::json!({"path": fx.dir.join("elsewhere")}).to_string();
    let (_, text) = send(
        &fx.app,
        "POST",
        "/api/workspace/recent/add",
        LOOPBACK,
        &[],
        &add,
    )
    .await;
    assert!(text.contains("Read-only mode"), "{text}");
    assert!(!recent_list(&fx.dir).contains("elsewhere"));
}
