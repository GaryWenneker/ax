//! "Local owner" routes trust the socket peer, never request headers.
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

const LOOPBACK: &str = "127.0.0.1:50000";
const REMOTE: &str = "203.0.113.7:50000";

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

async fn hub(dir: &std::path::Path) -> WebHub {
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let global = dir.join("global.db");
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_GLOBAL_DB", &global);
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let pool = ax_global_db::open_and_init(&global).await.unwrap();
    ax_global_db::access::bootstrap_owner(&pool, "owner", "Owner", None)
        .await
        .unwrap();
    WebHub::open(root, false, 7070).await.unwrap()
}

fn request(uri: &str, peer: &str, headers: &[(&str, &str)]) -> Request<Body> {
    let mut builder = Request::builder().uri(uri);
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut req = builder.body(Body::empty()).unwrap();
    let peer: SocketAddr = peer.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    req
}

async fn status(app: &Router, req: Request<Body>) -> StatusCode {
    app.clone().oneshot(req).await.unwrap().status()
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn only_a_loopback_peer_without_a_proxy_is_the_local_owner() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = hub(dir.path()).await.nest_routers(CorsLayer::new());
    let session = "/api/access/session";

    assert_eq!(
        status(
            &app,
            request(session, LOOPBACK, &[("host", "127.0.0.1:7070")])
        )
        .await,
        StatusCode::OK
    );
    // A remote peer that claims a loopback Host.
    assert_eq!(
        status(
            &app,
            request(session, REMOTE, &[("host", "127.0.0.1:7070")])
        )
        .await,
        StatusCode::FORBIDDEN
    );
    // A proxy on this computer forwarding someone else.
    for forwarded in [
        ("x-forwarded-for", "203.0.113.7"),
        ("forwarded", "for=203.0.113.7"),
        ("x-real-ip", "203.0.113.7"),
    ] {
        assert_eq!(
            status(
                &app,
                request(session, LOOPBACK, &[("host", "127.0.0.1:7070"), forwarded])
            )
            .await,
            StatusCode::FORBIDDEN,
            "{forwarded:?}"
        );
    }
    // A client cannot plant the server's own marker.
    assert_eq!(
        status(
            &app,
            request(
                session,
                REMOTE,
                &[("host", "127.0.0.1:7070"), ("x-ax-local-peer", "1")]
            )
        )
        .await,
        StatusCode::FORBIDDEN
    );
    // Mount and installer routes follow the same rule.
    assert_eq!(
        status(
            &app,
            request("/api/dav/mount", REMOTE, &[("host", "127.0.0.1:7070")])
        )
        .await,
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn the_terminal_socket_needs_a_loopback_peer_even_on_a_lan_bind() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let mut hub = hub(dir.path()).await;
    hub.local_only = false;
    let app = hub.nest_routers(CorsLayer::new());
    let ws = |peer: &str, origin: &str, host: &str| {
        request(
            "/api/agent/pty/ws?agent=builtin",
            peer,
            &[
                ("host", host),
                ("origin", origin),
                ("connection", "Upgrade"),
                ("upgrade", "websocket"),
                ("sec-websocket-version", "13"),
                ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
            ],
        )
    };
    assert_eq!(
        status(
            &app,
            ws("10.0.0.9:50000", "http://10.0.0.9:7070", "10.0.0.5:7070")
        )
        .await,
        StatusCode::FORBIDDEN
    );
    // oneshot has no connection-upgrade extension, so an accepted request
    // reaches the socket extractor and stops there.
    assert_eq!(
        status(
            &app,
            ws(LOOPBACK, "http://127.0.0.1:7070", "127.0.0.1:7070")
        )
        .await,
        StatusCode::UPGRADE_REQUIRED
    );
}
