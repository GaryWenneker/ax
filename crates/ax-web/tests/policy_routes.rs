//! Policy routes: skill routes act on the skill when a rule shares its name,
//! relocation never leaves a copy in both places, and export stays inside
//! the project. `HOME` and `AX_GLOBAL_DB` are process-global, so every
//! scenario runs under one lock.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

/// A project with rule `dup`, skill `dup` and rule `solo` on disk.
async fn app(home: &Path) -> (Router, PathBuf) {
    std::env::set_var("HOME", home);
    std::env::set_var("AX_HOME_DIR", home);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::set_var("AX_GLOBAL_DB", home.join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    let root = home.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::fs::create_dir_all(root.join(".agents/rules")).unwrap();
    std::fs::create_dir_all(root.join(".agents/skills/dup")).unwrap();
    for id in ["dup", "solo"] {
        std::fs::write(
            root.join(format!(".agents/rules/{id}.mdc")),
            format!("---\nid: {id}\nlevel: INFO\nalwaysApply: true\n---\n# {id}\n\nRule body.\n"),
        )
        .unwrap();
    }
    std::fs::write(
        root.join(".agents/skills/dup/SKILL.md"),
        "---\nname: dup\ndescription: A skill named like a rule.\n---\n# dup\n\nSkill body.\n",
    )
    .unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root.clone(), false, 7070).await.unwrap();
    let app = hub.nest_routers(CorsLayer::new());
    let (status, body) = send(&app, "POST", "/api/policy/reindex", "{}").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (app, root)
}

async fn send(app: &Router, method: &str, uri: &str, body: &str) -> (StatusCode, Value) {
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
    let json = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into_owned()));
    (status, json)
}

fn enabled(item: &Value) -> bool {
    item.pointer("/frontmatter/enabled")
        .or_else(|| item.get("enabled"))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("no enabled flag in {item}"))
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn skill_routes_act_on_the_skill_when_a_rule_shares_the_name() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let (app, _root) = app(home.path()).await;

    let (status, body) = send(
        &app,
        "PATCH",
        "/api/policy/skills/dup/enabled",
        r#"{"enabled":false}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (_, skill) = send(&app, "GET", "/api/policy/skills/dup", "").await;
    let (_, rule) = send(&app, "GET", "/api/policy/rules/dup", "").await;
    assert!(!enabled(&skill), "skill must be disabled: {skill}");
    assert!(enabled(&rule), "rule must stay enabled: {rule}");

    let (status, body) = send(
        &app,
        "PATCH",
        "/api/policy/skills/dup/storage",
        r#"{"storage":"database","keepFile":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["kind"], "skill", "{body}");

    let (status, body) = send(
        &app,
        "PATCH",
        "/api/policy/rules/dup/storage",
        r#"{"storage":"database","keepFile":true}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["kind"], "rule", "{body}");

    // A skill route never falls back to a rule of the same name.
    let (status, body) = send(
        &app,
        "PATCH",
        "/api/policy/skills/solo/enabled",
        r#"{"enabled":false}"#,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    let (_, rule) = send(&app, "GET", "/api/policy/rules/solo", "").await;
    assert!(enabled(&rule), "rule must stay enabled: {rule}");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn export_refuses_folders_outside_the_project() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let (app, root) = app(home.path()).await;

    let outside = home.path().join("outside");
    let abs = serde_json::json!({ "out_dir": outside.to_string_lossy() }).to_string();
    let (status, body) = send(&app, "POST", "/api/policy/export", &abs).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        !outside.exists(),
        "absolute export wrote outside the project"
    );

    let (status, body) = send(
        &app,
        "POST",
        "/api/policy/export",
        r#"{"out_dir":"../escaped"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(!home.path().join("escaped").exists(), "`..` export escaped");

    #[cfg(unix)]
    {
        let link_target = home.path().join("linked");
        std::fs::create_dir_all(&link_target).unwrap();
        std::os::unix::fs::symlink(&link_target, root.join("link")).unwrap();
        let (status, body) = send(
            &app,
            "POST",
            "/api/policy/export",
            r#"{"out_dir":"link/out"}"#,
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
        assert!(
            !link_target.join("out").exists(),
            "export followed a link out"
        );
    }

    let (status, body) = send(&app, "POST", "/api/policy/export", "{}").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(root.join(".ax/policy/export").is_dir());
}
