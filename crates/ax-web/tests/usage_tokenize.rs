//! `/api/usage/tokenize` takes a bounded body and still answers normal text.
//! `HOME` is process-global, so every scenario runs under one lock.

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

async fn app(home: &std::path::Path) -> Router {
    std::env::set_var("HOME", home);
    std::env::set_var("AX_HOME_DIR", home);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::set_var("AX_GLOBAL_DB", home.join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    let root = home.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    WebHub::open(root, false, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new())
}

async fn tokenize(app: &Router, text: &str) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/api/usage/tokenize")
        .header("host", "127.0.0.1:7070")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::json!({ "text": text }).to_string()))
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn tokenize_answers_normal_text_and_refuses_an_oversized_body() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path()).await;

    let (status, reply) = tokenize(&app, "hello world").await;
    assert_eq!(status, StatusCode::OK);
    assert!(reply["count"].as_u64().unwrap_or(0) > 0, "{reply}");

    // Text above the tokenizer's input cap is still accepted and marked truncated.
    let (status, reply) = tokenize(&app, &"word ".repeat(20_000)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reply["truncated"], true);

    // A body far beyond what the tokenizer reads is refused before parsing.
    let (status, _) = tokenize(&app, &"a".repeat(1024 * 1024)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
}
