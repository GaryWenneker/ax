//! Login, share-token, and terminal-origin checks. Separate process: these set env vars.

use std::sync::{Mutex, MutexGuard};

use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[tokio::test]
async fn login_covers_the_vault_and_a_share_token_still_opens_it() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let global = dir.path().join("global.db");
    std::env::set_var("AX_HOME_DIR", dir.path());
    std::env::set_var("AX_GLOBAL_DB", &global);
    std::env::remove_var("AX_SHARE_TOKEN");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let pool = ax_global_db::open_and_init(&global).await.unwrap();
    sqlx::query("INSERT INTO web_auth_settings(singleton,enabled) VALUES (1,1)")
        .execute(&pool)
        .await
        .unwrap();
    let hub = WebHub::open(root, false, 7070).await.unwrap();
    let app = hub.nest_routers(CorsLayer::new());

    let open_vault = app
        .clone()
        .oneshot(Request::builder().uri("/dav/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(open_vault.status(), StatusCode::UNAUTHORIZED);

    let ax_vault = app
        .clone()
        .oneshot(Request::builder().uri("/ax/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(ax_vault.status(), StatusCode::UNAUTHORIZED);

    let status = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/share/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status.status(), StatusCode::OK);

    std::env::set_var("AX_SHARE_TOKEN", "vault-share-token");
    let shared = app
        .oneshot(
            Request::builder()
                .uri("/dav/")
                .header("authorization", "Bearer vault-share-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    std::env::remove_var("AX_SHARE_TOKEN");
    assert_ne!(shared.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn foreign_origin_cannot_open_the_terminal_socket() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::env::set_var("AX_HOME_DIR", dir.path());
    std::env::set_var("AX_GLOBAL_DB", dir.path().join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, false, 7070).await.unwrap();
    let app = hub.nest_routers(CorsLayer::new());

    let denied = app
        .clone()
        .oneshot(ws_request("http://evil.example"))
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let body = axum::body::to_bytes(denied.into_body(), 1024)
        .await
        .unwrap();
    assert_eq!(body.as_ref(), b"Untrusted origin");

    // oneshot has no connection-upgrade extension, so an accepted origin
    // reaches the socket extractor and stops there.
    let allowed = app
        .oneshot(ws_request("http://127.0.0.1:7070"))
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::UPGRADE_REQUIRED);
}

fn ws_request(origin: &str) -> Request<Body> {
    Request::builder()
        .uri("/api/agent/pty/ws?agent=builtin")
        .header("origin", origin)
        .header("connection", "Upgrade")
        .header("upgrade", "websocket")
        .header("sec-websocket-version", "13")
        .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
        .body(Body::empty())
        .unwrap()
}
