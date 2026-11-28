//! Agent profile routes must not change anything in read-only mode and must
//! refuse ids that leave `~/.ax/agent-profiles`.
//! `HOME` is process-global, so every scenario runs in one test.

use ax_web::WebHub;
use axum::body::Body;
use axum::http::Request;
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};

async fn open_app(home: &std::path::Path, readonly: bool) -> Router {
    std::env::set_var("HOME", home);
    std::env::set_var("AX_HOME_DIR", home);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::set_var("AX_GLOBAL_DB", home.join("global.db"));
    let root = home.join(if readonly { "ro" } else { "proj" });
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, readonly, 0).await.unwrap();
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    hub.nest_routers(cors)
}

async fn send(app: &Router, method: &str, path: &str, body: Value) -> Value {
    let req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json")
        .header("host", "127.0.0.1:7070")
        .body(Body::from(body.to_string()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

#[tokio::test]
async fn profile_routes_respect_readonly_and_refuse_escaping_ids() {
    let home = tempfile::tempdir().unwrap();
    let app = open_app(home.path(), false).await;

    let victim = home.path().join("Documents");
    std::fs::create_dir_all(&victim).unwrap();
    std::fs::write(victim.join("keep.txt"), b"keep").unwrap();
    let r = send(
        &app,
        "POST",
        "/api/agent/profiles",
        json!({ "agent": "../..", "id": "Documents", "label": "x" }),
    )
    .await;
    assert_eq!(r["ok"], false, "{r}");
    let r = send(
        &app,
        "POST",
        "/api/agent/profiles",
        json!({ "agent": "claude", "id": "", "label": "x" }),
    )
    .await;
    assert_eq!(r["ok"], false, "{r}");

    let r = send(
        &app,
        "POST",
        "/api/agent/profiles",
        json!({ "agent": "claude", "id": "work", "label": "Work" }),
    )
    .await;
    assert_eq!(r["ok"], true, "{r}");
    let dir = home.path().join(".ax/agent-profiles/claude/work");
    assert!(dir.exists());

    let ro = open_app(home.path(), true).await;
    let r = send(&ro, "DELETE", "/api/agent/profiles/claude/work", json!({})).await;
    assert_eq!(r["ok"], false, "read-only delete must be refused: {r}");
    assert!(dir.exists(), "read-only delete removed the profile folder");

    let r = send(&app, "DELETE", "/api/agent/profiles/claude/work", json!({})).await;
    assert_eq!(r["ok"], true, "{r}");
    assert!(!dir.exists());
    assert!(victim.join("keep.txt").exists());
}
