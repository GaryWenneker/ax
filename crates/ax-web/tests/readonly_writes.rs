//! A read-only Command Center refuses every route that writes, in policy,
//! policy share, Microsoft sign-in, ship, specs, the skill catalog, usage
//! (savings and pricing), remote MCP publications and vault folders.
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
    std::fs::create_dir_all(root.join(".agents/rules")).unwrap();
    std::fs::write(
        root.join(".agents/rules/solo.mdc"),
        "---\nid: solo\nlevel: INFO\nalwaysApply: true\n---\n# solo\n\nRule body.\n",
    )
    .unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    WebHub::open(root, true, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new())
}

/// The spec routes answer a refused write with 200 and `{"ok":false,"error":"readonly"}`.
fn refused(status: StatusCode, body: &serde_json::Value) -> bool {
    status == StatusCode::FORBIDDEN
        || (status == StatusCode::OK && body["ok"] == false && body["error"] == "readonly")
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: &str,
) -> (StatusCode, serde_json::Value) {
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
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

/// Routes that change files, databases or settings, with a body that parses.
const WRITES: &[(&str, &str, &str)] = &[
    (
        "POST",
        "/api/policy/rules",
        r#"{"frontmatter":{"id":"x","level":"INFO"},"body":"b"}"#,
    ),
    (
        "PUT",
        "/api/policy/rules/solo",
        r#"{"frontmatter":{"id":"solo","level":"INFO"},"body":"b"}"#,
    ),
    ("DELETE", "/api/policy/rules/solo", ""),
    (
        "PATCH",
        "/api/policy/rules/solo/enabled",
        r#"{"enabled":false}"#,
    ),
    (
        "PATCH",
        "/api/policy/rules/solo/storage",
        r#"{"storage":"database"}"#,
    ),
    ("POST", "/api/policy/rules/solo/revisions/1/restore", "{}"),
    (
        "POST",
        "/api/policy/skills",
        r#"{"frontmatter":{"name":"x","description":"d"},"body":"b"}"#,
    ),
    ("DELETE", "/api/policy/skills/x", ""),
    (
        "PATCH",
        "/api/policy/skills/x/enabled",
        r#"{"enabled":false}"#,
    ),
    (
        "POST",
        "/api/policy/relocate",
        r#"{"kind":"rule","id":"solo","to":"global"}"#,
    ),
    (
        "POST",
        "/api/policy/copies/delete",
        r#"{"kind":"rule","id":"solo","layer":"global"}"#,
    ),
    (
        "POST",
        "/api/policy/capture",
        r#"{"prompt":"p","action":"save","rule":{"frontmatter":{"id":"x","level":"INFO"},"body":"b"}}"#,
    ),
    ("POST", "/api/policy/reindex", "{}"),
    ("POST", "/api/policy/export", "{}"),
    ("POST", "/api/policy/pack/export", "{}"),
    ("POST", "/api/policy/pack/import", "{}"),
    ("POST", "/api/policy/review/x/approve", "{}"),
    ("POST", "/api/policy/review/x/reject", "{}"),
    ("PUT", "/api/policy/settings", r#"{"requireReview":true}"#),
    (
        "POST",
        "/api/policy/skills/catalog/install",
        r#"{"source":"x","name":"x"}"#,
    ),
    ("PUT", "/api/policy/skills/catalog/settings", "{}"),
    ("POST", "/api/policy/skills/x/publish", "{}"),
    ("PUT", "/api/policy/share/config", "{}"),
    ("POST", "/api/policy/share/sync", "{}"),
    ("PUT", "/api/auth/microsoft/config", r#"{"clientId":"c"}"#),
    ("DELETE", "/api/auth/microsoft", ""),
    ("POST", "/api/ship/command", r#"{"cmd":"evaluate"}"#),
    ("POST", "/api/ship/command", r#"{"cmd":"draft"}"#),
    ("PUT", "/api/ship/config", "{}"),
    ("PUT", "/api/ship/review-language", r#"{"code":"en"}"#),
    ("POST", "/api/ship/sonar/install", "{}"),
    ("POST", "/api/ship/sonar/start", "{}"),
    ("POST", "/api/ship/sonar/stop", "{}"),
    ("POST", "/api/ship/sonar/scan", "{}"),
    (
        "POST",
        "/api/ship/sonar/exclude",
        r#"{"repo":"r","excluded":true}"#,
    ),
    ("POST", "/api/specs/with-plan", "{}"),
    ("POST", "/api/specs/import", "{}"),
    (
        "POST",
        "/api/specs/x/revisions",
        r#"{"basedOn":1,"body":"b"}"#,
    ),
    ("POST", "/api/specs/x/approve", r#"{"version":1}"#),
    ("POST", "/api/usage/savings/import", "{}"),
    ("POST", "/api/usage/pricing/sync", "{}"),
    ("POST", "/api/remote-mcp/publications/import", "{}"),
    ("POST", "/api/remote-mcp/publications/host/setup", "{}"),
    ("DELETE", "/api/remote-mcp/publications/x", ""),
    ("PUT", "/api/vault/folders", "[]"),
    ("DELETE", "/api/vault/folders/x", ""),
];

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn a_read_only_server_refuses_every_write() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path()).await;
    let mut allowed = Vec::new();
    for (method, uri, body) in WRITES {
        let (status, reply) = send(&app, method, uri, body).await;
        if !refused(status, &reply) {
            allowed.push(format!("{method} {uri} -> {status}"));
        }
    }
    assert!(allowed.is_empty(), "not refused:\n{}", allowed.join("\n"));
    // Reads still answer.
    assert_eq!(
        send(&app, "GET", "/api/policy/rules", "").await.0,
        StatusCode::OK
    );
}
