//! Native folder picker route (P1, P3–P5) from docs/specs/folder-picker-and-ide-install.md.
//! `AX_FOLDER_PICKER_CMD` is process-global, so every scenario runs in one test.
//! The picker commands are `#!/bin/sh` scripts, so this file only runs on Unix.
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};

async fn open_app(readonly: bool) -> (tempfile::TempDir, Router) {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("HOME", dir.path());
    std::env::set_var("AX_HOME_DIR", dir.path());
    std::env::set_var("AX_GLOBAL_DB", dir.path().join("global.db"));
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root, readonly, 0).await.unwrap();
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    (dir, hub.nest_routers(cors))
}

async fn pick(app: &Router, headers: &[(&str, &str)]) -> (StatusCode, Value) {
    let mut req = Request::builder()
        .method("POST")
        .uri("/api/vault/folder-picker");
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req = req.header("host", "127.0.0.1:7070");
    }
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    let res = app
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn script(dir: &Path, name: &str, body: &str) -> String {
    let p = dir.join(name);
    std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    p.to_string_lossy().into_owned()
}

#[tokio::test]
async fn picker_route_scenarios() {
    let (dir, app) = open_app(false).await;

    std::env::set_var(
        "AX_FOLDER_PICKER_CMD",
        script(dir.path(), "ok", "printf '/tmp/notes/\\n'"),
    );
    assert_eq!(
        pick(&app, &[]).await,
        (StatusCode::OK, json!({ "path": "/tmp/notes" })),
        "P1 chosen"
    );

    std::env::set_var(
        "AX_FOLDER_PICKER_CMD",
        script(dir.path(), "cancel", "exit 1"),
    );
    assert_eq!(
        pick(&app, &[]).await,
        (StatusCode::OK, json!({ "path": null })),
        "P1 cancel"
    );

    std::env::set_var(
        "AX_FOLDER_PICKER_CMD",
        script(dir.path(), "fail", "printf '/tmp/partial\\n'; exit 1"),
    );
    assert_eq!(
        pick(&app, &[]).await,
        (StatusCode::OK, json!({ "path": null })),
        "P1 failed dialog output is ignored"
    );

    std::env::set_var(
        "AX_FOLDER_PICKER_CMD",
        script(dir.path(), "blank", "printf '\\n'"),
    );
    assert_eq!(
        pick(&app, &[]).await,
        (StatusCode::OK, json!({ "path": null })),
        "P1 empty output"
    );

    std::env::set_var(
        "AX_FOLDER_PICKER_CMD",
        dir.path().join("missing").to_string_lossy().into_owned(),
    );
    let (status, body) = pick(&app, &[]).await;
    assert_eq!(status, StatusCode::NOT_IMPLEMENTED, "P3");
    assert!(
        body["error"].as_str().is_some_and(|e| !e.is_empty()),
        "{body}"
    );

    std::env::set_var(
        "AX_FOLDER_PICKER_CMD",
        script(dir.path(), "ok2", "printf '/tmp/x\\n'"),
    );
    for headers in [
        vec![("origin", "https://evil.example")],
        vec![("host", "ax.example.com")],
    ] {
        assert_eq!(
            pick(&app, &headers).await.0,
            StatusCode::FORBIDDEN,
            "P5 {headers:?}"
        );
    }
    assert_eq!(
        pick(&app, &[("origin", "http://127.0.0.1:7070")]).await.0,
        StatusCode::OK,
        "P5 local origin"
    );

    std::env::set_var(
        "AX_FOLDER_PICKER_CMD",
        script(dir.path(), "slow", "sleep 1; printf '/tmp/slow\\n'"),
    );
    let (a, b) = tokio::join!(pick(&app, &[]), async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        pick(&app, &[]).await
    });
    assert_eq!(
        a,
        (StatusCode::OK, json!({ "path": "/tmp/slow" })),
        "P5 first"
    );
    assert_eq!(b.0, StatusCode::CONFLICT, "P5 second while open");

    let (_ro_dir, ro) = open_app(true).await;
    assert_eq!(pick(&ro, &[]).await.0, StatusCode::FORBIDDEN, "P5 readonly");
}
