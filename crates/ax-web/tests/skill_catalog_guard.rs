//! Skill catalog routes that write obey read-only mode, and the publish
//! repository must be a remote URL or a folder inside the project.

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

async fn app(dir: &std::path::Path, readonly: bool) -> (Router, std::path::PathBuf) {
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_GLOBAL_DB", dir.join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let hub = WebHub::open(root.clone(), readonly, 7070).await.unwrap();
    (hub.nest_routers(CorsLayer::new()), root)
}

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
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_mode_refuses_every_catalog_write() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let (app, root) = app(dir.path(), true).await;
    for (method, uri, body) in [
        (
            "POST",
            "/api/policy/skills/catalog/install",
            r#"{"source":"::not-a-source::","yes":true}"#,
        ),
        (
            "PUT",
            "/api/policy/skills/catalog/settings",
            r#"{"reportInstalls":true,"publishRemote":"https://github.com/attacker/x.git"}"#,
        ),
        ("POST", "/api/policy/skills/demo/publish", "{}"),
        ("POST", "/api/policy/skills/demo/audit", "{}"),
        ("POST", "/api/policy/rules/demo/audit", "{}"),
        (
            "POST",
            "/api/policy/skills/catalog/audit",
            r#"{"source":"::not-a-source::","skill":"demo"}"#,
        ),
    ] {
        let (status, text) = send(&app, method, uri, body).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {text}");
    }
    let settings =
        std::fs::read_to_string(root.join(".ax").join("skill-catalog.json")).unwrap_or_default();
    assert!(!settings.contains("attacker"), "{settings}");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn the_publish_repository_must_be_a_remote_url_or_inside_the_project() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let (app, root) = app(dir.path(), false).await;
    std::fs::create_dir_all(root.join("published")).unwrap();
    let outside = dir.path().join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let put = |remote: String| {
        serde_json::json!({ "reportInstalls": false, "publishRemote": remote }).to_string()
    };
    let uri = "/api/policy/skills/catalog/settings";
    for remote in [
        "--upload-pack=touch /tmp/ax-pwned".to_string(),
        "-u".to_string(),
        "file:///etc".to_string(),
        "ext::sh -c touch% /tmp/ax-pwned".to_string(),
        outside.display().to_string(),
        format!("well-known:{}", outside.display()),
        "../outside".to_string(),
    ] {
        let (status, text) = send(&app, "PUT", uri, &put(remote.clone())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{remote}: {text}");
    }
    for remote in [
        String::new(),
        "https://github.com/me/skills.git".to_string(),
        "ssh://git@github.com/me/skills.git".to_string(),
        "git@github.com:me/skills.git".to_string(),
        root.join("published").display().to_string(),
        "published".to_string(),
    ] {
        let (status, text) = send(&app, "PUT", uri, &put(remote.clone())).await;
        assert_eq!(status, StatusCode::OK, "{remote}: {text}");
    }

    // A relative folder is inside the project, not the server's working
    // directory.
    let skill = root.join(".agents").join("skills").join("demo");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: demo\ndescription: Demo skill.\n---\n\nDo the demo.\n",
    )
    .unwrap();
    let (status, text) = send(&app, "POST", "/api/policy/skills/demo/publish", "{}").await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(root
        .join("published")
        .join("skills")
        .join("demo")
        .join("SKILL.md")
        .is_file());
}
