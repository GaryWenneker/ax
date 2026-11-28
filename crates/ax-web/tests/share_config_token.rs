//! `GET /api/policy/share/config` reports `tokenSet` and never the git API
//! token. A read writes nothing (it is allowed in read-only mode); a token
//! left in `ax.json` moves to the credential store on the next save.
//! The credential store is the in-process memory store
//! (`AX_SHARE_SECRET_STORE=memory`), so no OS keychain is touched.

use std::net::SocketAddr;

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

async fn send(app: &Router, method: &str, uri: &str, body: &str) -> (StatusCode, String) {
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
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
async fn share_config_reports_token_set_and_never_the_token() {
    let home = tempfile::tempdir().unwrap();
    let home = home.path();
    std::env::set_var("HOME", home);
    std::env::set_var("AX_HOME_DIR", home);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::set_var("AX_GLOBAL_DB", home.join("global.db"));
    std::env::set_var("AX_SHARE_SECRET_STORE", "memory");
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    let root = home.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::fs::write(
        root.join("ax.json"),
        r#"{"share":{"provider":"github","github":{"repoUrl":"https://gitlab.example.com/t/p.git","token":"glpat-in-ax-json"}}}"#,
    )
    .unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let app = WebHub::open(root.clone(), false, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new());

    let (status, body) = send(&app, "GET", "/api/policy/share/config", "").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("glpat-in-ax-json"), "{body}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["github"]["tokenSet"], true, "{body}");
    assert!(json["github"].get("token").is_none(), "{body}");
    let on_disk = std::fs::read_to_string(root.join("ax.json")).unwrap();
    assert!(
        on_disk.contains("glpat-in-ax-json"),
        "a GET rewrote ax.json: {on_disk}"
    );

    // A save without a token moves the old one out of ax.json.
    let (status, body) = send(
        &app,
        "PUT",
        "/api/policy/share/config",
        r#"{"provider":"github","github":{"repoUrl":"https://gitlab.example.com/t/p.git"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let on_disk = std::fs::read_to_string(root.join("ax.json")).unwrap();
    assert!(!on_disk.contains("glpat-in-ax-json"), "{on_disk}");
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["github"]["tokenSet"], true, "{body}");

    // A new token goes to the credential store and never reaches a file.
    let (status, body) = send(
        &app,
        "PUT",
        "/api/policy/share/config",
        r#"{"provider":"github","github":{"repoUrl":"https://gitlab.example.com/t/p.git","token":"glpat-new"}}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.contains("glpat-new"), "{body}");
    let on_disk = std::fs::read_to_string(root.join("ax.json")).unwrap();
    assert!(!on_disk.contains("glpat-new"), "{on_disk}");
    assert!(!home.join(".ax/auth/git-share.json").exists());
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(json["github"]["tokenSet"], true, "{body}");
}
