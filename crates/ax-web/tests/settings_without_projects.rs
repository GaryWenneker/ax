//! Node and remote host settings files that list no project are answered
//! with a status, never a handler panic (a dropped connection).
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

async fn app(dir: &std::path::Path) -> (Router, std::path::PathBuf) {
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let global = dir.join("global.db");
    std::env::set_var("HOME", dir);
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::set_var("AX_GLOBAL_DB", &global);
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    // Credentials from the environment keep the test away from the keychain.
    std::env::set_var("AX_COMMAND_CENTER_REPLICATION_TOKEN", "t".repeat(40));
    std::env::set_var("AX_COMMAND_CENTER_NODE_CONTROL", "c".repeat(40));
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let pool = ax_global_db::open_and_init(&global).await.unwrap();
    ax_global_db::access::bootstrap_owner(&pool, "owner", "Owner", None)
        .await
        .unwrap();
    let app = WebHub::open(root.clone(), false, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new());
    (app, root)
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: &str,
    csrf: Option<&str>,
) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .header("content-type", "application/json");
    if let Some(csrf) = csrf {
        req = req.header("x-ax-csrf", csrf);
    }
    let mut req = req.body(Body::from(body.to_string())).unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or_default())
}

async fn get(app: &Router, uri: &str) -> StatusCode {
    send(app, "GET", uri, "", None).await.0
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn node_status_answers_when_the_saved_node_lists_no_project() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let (app, root) = app(dir.path()).await;
    std::fs::write(
        root.join(".ax/command-center-node.json"),
        serde_json::json!({
            "config": {
                "schema_version": 1,
                "cluster_id": "team",
                "node_id": "node",
                "role": "primary",
                "primary_url": null,
                "projects": []
            },
            "port": 1,
            "advertise": null,
            "enabled": false
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(get(&app, "/api/access/nodes/local").await, StatusCode::OK);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn host_authorizations_answer_when_the_host_lists_no_project() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let (app, root) = app(dir.path()).await;
    std::fs::write(
        root.join(".ax/command-center-remote-host.json"),
        serde_json::json!({
            "config": {
                "public_url": "https://mcp.example.com",
                "oauth": {
                    "built_in": true,
                    "issuer": "https://mcp.example.com",
                    "jwks_uri": "https://mcp.example.com/jwks"
                },
                "projects": []
            },
            "port": 1,
            "enabled": false
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        get(&app, "/api/remote-mcp/publications/host/authorizations").await,
        StatusCode::CONFLICT
    );
    let (status, list) = send(&app, "GET", "/api/remote-mcp/publications", "", None).await;
    assert_eq!(status, StatusCode::OK, "{list}");
    let csrf = list["csrf"].as_str().unwrap().to_string();
    let decision = serde_json::json!({
        "workspaceId": list["workspaceId"],
        "decision": "approve"
    })
    .to_string();
    let (status, reply) = send(
        &app,
        "POST",
        "/api/remote-mcp/publications/host/authorizations/r1",
        &decision,
        Some(&csrf),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{reply}");
    assert!(
        reply.to_string().contains("no project"),
        "the error names the broken settings: {reply}"
    );
}
