//! Credential failures at the share-token gate: node credentials get a clear
//! answer, failed client credentials are audited, and a peer that keeps
//! presenting wrong tokens is slowed down with 429.
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
    std::env::set_var("HOME", dir);
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::set_var("AX_GLOBAL_DB", dir.join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    WebHub::open(root, true, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new())
}

async fn get(app: &Router, peer: &str, bearer: Option<&str>) -> (StatusCode, String) {
    let mut req = Request::builder()
        .uri("/api/stats")
        .header("host", "127.0.0.1:7070");
    if let Some(token) = bearer {
        req = req.header("authorization", format!("Bearer {token}"));
    }
    let mut req = req.body(Body::empty()).unwrap();
    let peer: SocketAddr = peer.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn node_credentials_get_a_clear_answer_and_client_failures_are_audited() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path()).await;

    let node = format!("axn_{}", "n".repeat(40));
    let (status, body) = get(&app, "127.0.0.4:50000", Some(&node)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(body.contains("replication server"), "{body}");

    let client = format!("axc_{}", "c".repeat(40));
    let (status, _) = get(&app, "127.0.0.4:50000", Some(&client)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    let pool = ax_global_db::open_and_init(&dir.path().join("global.db"))
        .await
        .unwrap();
    let failures: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM security_audit_log WHERE event_type='authentication.failed'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(failures, 2, "both failed credentials are audited");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn a_peer_that_keeps_failing_the_share_token_is_refused_for_a_while() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let app = app(dir.path()).await;
    std::env::set_var("AX_SHARE_TOKEN", "s3cret");

    let attacker = "127.0.0.2:50000";
    let mut refused = None;
    for attempt in 0..40 {
        let (status, _) = get(&app, attacker, Some("guess")).await;
        if status == StatusCode::TOO_MANY_REQUESTS {
            refused = Some(attempt);
            break;
        }
        assert_ne!(status, StatusCode::OK);
    }
    let refused = refused.expect("repeated failures are refused with 429");
    assert!(
        refused >= 10,
        "a few mistakes are allowed: refused at {refused}"
    );
    // Locked out: even the right token waits until the window ends.
    assert_eq!(
        get(&app, attacker, Some("s3cret")).await.0,
        StatusCode::TOO_MANY_REQUESTS
    );
    // Another peer is not affected.
    assert_eq!(
        get(&app, "127.0.0.3:50000", Some("s3cret")).await.0,
        StatusCode::OK
    );
    // A request without any token still gets the share gate, not 429.
    assert_eq!(get(&app, attacker, None).await.0, StatusCode::UNAUTHORIZED);
    std::env::remove_var("AX_SHARE_TOKEN");
}
