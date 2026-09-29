//! `/ax/` alias and `/api/dav/mount` behaviors from docs/specs/webdav-mount-settings.md.

use std::path::Path;
use std::sync::OnceLock;

use ax_policy::parse_rule_file;
use ax_web::WebHub;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use tower_http::cors::{Any, CorsLayer};

const RULE: &str =
    "---\nid: english-only\nlevel: WARNING\nalwaysApply: true\n---\n\nWrite English.";

static HOME: OnceLock<TempDir> = OnceLock::new();

fn isolate_home() -> &'static Path {
    HOME.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("HOME", dir.path());
        std::env::set_var("AX_DAV_MOUNT_DRY_RUN", "1");
        dir
    })
    .path()
}

async fn fixture(readonly: bool) -> (TempDir, Router) {
    isolate_home();
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    {
        let seed = WebHub::open(root.clone(), false, 0).await.unwrap();
        let ws = seed.read().await;
        let doc = parse_rule_file(Path::new("x.mdc"), RULE).unwrap();
        ws.policy
            .store
            .save_rule(doc.frontmatter, doc.body)
            .await
            .unwrap();
    }
    let hub = WebHub::open(root, readonly, 7070).await.unwrap();
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);
    (dir, hub.nest_routers(cors))
}

async fn send(
    app: &Router,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, String) {
    let mut req = Request::builder().method(method).uri(path);
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

const LOCAL: (&str, &str) = ("host", "127.0.0.1:7070");
const JSON: (&str, &str) = ("content-type", "application/json");

fn json(body: &str) -> Value {
    serde_json::from_str(body).unwrap_or_else(|e| panic!("not JSON ({e}): {body}"))
}

#[tokio::test]
async fn b1_ax_alias_lists_and_serves_pages() {
    let (_d, app) = fixture(false).await;
    let (status, body) = send(&app, "PROPFIND", "/ax/", &[("Depth", "1")], "").await;
    assert_eq!(status, StatusCode::MULTI_STATUS, "{body}");
    assert!(body.contains("/ax/rules/"), "{body}");
    assert!(!body.contains("/dav/"), "{body}");
    let (status, body) = send(&app, "GET", "/ax/rules/english-only.md", &[], "").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("Write English."), "{body}");
    let (status, _) = send(&app, "PROPFIND", "/dav/", &[("Depth", "1")], "").await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
}

#[tokio::test]
async fn b3_status_shape() {
    let (_d, app) = fixture(false).await;
    let (status, body) = send(&app, "GET", "/api/dav/mount", &[LOCAL], "").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let v = json(&body);
    assert_eq!(v["url"], "http://127.0.0.1:7070/ax/");
    assert!(v["os"].is_string());
    assert!(v["mounted"].is_boolean());
    assert!(v["autostart"].is_boolean());
}

#[tokio::test]
async fn b4_b5_mount_then_unmount_dry_run() {
    let (_d, app) = fixture(false).await;
    let (status, body) = send(
        &app,
        "POST",
        "/api/dav/mount",
        &[LOCAL, JSON],
        r#"{"autostart":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let v = json(&body);
    assert_eq!(v["name"], "ax");
    assert_eq!(v["autostart"], true);
    let cmds = v["commands"].as_array().expect("commands");
    assert!(!cmds.is_empty());
    assert!(body.contains("/ax/"), "{body}");

    let (_, body) = send(&app, "GET", "/api/dav/mount", &[LOCAL], "").await;
    assert_eq!(json(&body)["name"], "ax");

    let (status, body) = send(&app, "DELETE", "/api/dav/mount", &[LOCAL], "").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!json(&body)["commands"].as_array().unwrap().is_empty());
    let (_, body) = send(&app, "GET", "/api/dav/mount", &[LOCAL], "").await;
    let v = json(&body);
    assert_eq!(v["mounted"], false);
    assert_eq!(v["autostart"], false);
}

#[tokio::test]
async fn b6_bad_name_is_400() {
    let (_d, app) = fixture(false).await;
    let (status, body) = send(
        &app,
        "POST",
        "/api/dav/mount",
        &[LOCAL, JSON],
        r#"{"name":"ax; rm -rf ~"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(json(&body).get("commands").is_none());
}

#[tokio::test]
async fn n1_foreign_host_origin_and_readonly_are_403() {
    let (_d, app) = fixture(false).await;
    for h in [
        vec![("host", "192.168.1.5:7070")],
        vec![LOCAL, ("origin", "https://evil.com")],
    ] {
        let mut hs = h.clone();
        hs.push(JSON);
        let (status, _) = send(&app, "POST", "/api/dav/mount", &hs, "{}").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{h:?}");
        let (status, _) = send(&app, "DELETE", "/api/dav/mount", &h, "").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{h:?}");
    }
    let (_d2, ro) = fixture(true).await;
    let (status, _) = send(&ro, "POST", "/api/dav/mount", &[LOCAL, JSON], "{}").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
