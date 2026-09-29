//! IDE install routes (I2, I4) from docs/specs/folder-picker-and-ide-install.md.
//! `HOME` is process-global, so every scenario runs in one test.

use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};

async fn open_app(home: &std::path::Path, readonly: bool) -> Router {
    std::env::set_var("HOME", home);
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

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Value,
) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method(method)
        .uri(path)
        .header("content-type", "application/json");
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req = req.header("host", "127.0.0.1:7070");
    }
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn cursor_configured(status_body: &str) -> bool {
    let v: Value = serde_json::from_str(status_body).unwrap();
    v["targets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["id"] == "cursor" && t["configured"] == true)
}

#[tokio::test]
async fn install_routes_are_local_only_and_report_what_they_changed() {
    let home = tempfile::tempdir().unwrap();
    let app = open_app(home.path(), false).await;
    let mcp = home.path().join(".cursor").join("mcp.json");
    let body = json!({ "targets": ["cursor"] });

    for path in [
        "/api/agent/install",
        "/api/agent/install/stream",
        "/api/agent/cli/install/stream",
        "/api/agent/uninstall",
    ] {
        for headers in [
            vec![("origin", "https://evil.example")],
            vec![("host", "ax.example.com")],
        ] {
            let (status, _) = send(&app, "POST", path, &headers, body.clone()).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "I4 {path} {headers:?}");
        }
    }
    assert!(
        !mcp.exists(),
        "I4: a refused install wrote {}",
        mcp.display()
    );

    let (status, text) = send(
        &app,
        "POST",
        "/api/agent/install",
        &[("origin", "http://127.0.0.1:7070")],
        body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["ok"], true, "{text}");
    assert!(mcp.is_file(), "I2: install did not write {}", mcp.display());
    let (_, status_body) = send(&app, "GET", "/api/agent/status", &[], Value::Null).await;
    assert!(cursor_configured(&status_body), "I5 status after install");
    let targets: Value = serde_json::from_str(&status_body).unwrap();
    let panel = |id: &str| {
        targets["targets"]
            .as_array()
            .and_then(|a| a.iter().find(|t| t["id"] == id))
            .map(|t| t["panel"].clone())
    };
    assert_eq!(panel("cursor"), Some(Value::Bool(false)), "C5 panel for an IDE: {status_body}");
    assert_eq!(panel("claude"), Some(Value::Null), "C5 no panel for a terminal agent: {status_body}");
    assert!(panel("jetbrains").is_some(), "D1 jetbrains target listed: {status_body}");

    let (status, text) = send(&app, "POST", "/api/agent/uninstall", &[], body.clone()).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["ok"], true, "{text}");
    assert_eq!(
        v["results"][0]["id"], "cursor",
        "I2 uninstall results: {text}"
    );
    assert!(
        v["results"][0]["files"]
            .as_array()
            .is_some_and(|f| !f.is_empty()),
        "{text}"
    );
    let (_, status_body) = send(&app, "GET", "/api/agent/status", &[], Value::Null).await;
    assert!(
        !cursor_configured(&status_body),
        "I5 status after uninstall"
    );

    let ro = open_app(home.path(), true).await;
    let (status, text) = send(&ro, "POST", "/api/agent/install", &[], body).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "I4 readonly");
    assert_eq!(serde_json::from_str::<Value>(&text).unwrap()["ok"], false);
}
