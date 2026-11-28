//! The Specs routes in read-only mode and the plan import behind the list.
//! `HOME` is process-global, so every scenario runs under one lock.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn set_env(home: &Path) {
    std::env::set_var("HOME", home);
    std::env::set_var("AX_HOME_DIR", home);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::set_var("AX_GLOBAL_DB", home.join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::remove_var("HERMES_HOME");
}

async fn app(root: &Path, readonly: bool) -> Router {
    let hub = WebHub::open(root.to_path_buf(), readonly, 7070)
        .await
        .unwrap();
    hub.nest_routers(CorsLayer::new())
}

async fn call(app: &Router, method: &str, uri: &str, body: Value) -> Value {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .header("content-type", "application/json")
        .body(if method == "GET" {
            Body::empty()
        } else {
            Body::from(body.to_string())
        })
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let response = app.clone().oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

fn write_plan(root: &Path, name: &str) {
    let dir = root.join(".cursor").join("plans");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join(name),
        format!(
            "# Plan {name}\n\nsession: axs_{}\n\n## Steps\n\n- do the thing\n",
            name.len()
        ),
    )
    .unwrap();
}

async fn project(home: &Path) -> std::path::PathBuf {
    let root = home.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    root
}

fn spec_count(listing: &Value) -> usize {
    listing["specs"].as_array().map(Vec::len).unwrap_or(0)
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_list_does_not_import_and_still_lists() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    set_env(home.path());
    let root = project(home.path()).await;
    write_plan(&root, "first.md");
    {
        let rw = app(&root, false).await;
        let listing = call(&rw, "GET", "/api/specs", Value::Null).await;
        assert!(listing.get("error").is_none(), "{listing}");
        assert_eq!(spec_count(&listing), 1, "{listing}");
    }
    write_plan(&root, "second.md");
    let ro = app(&root, true).await;
    let listing = call(&ro, "GET", "/api/specs", Value::Null).await;
    assert!(listing.get("error").is_none(), "{listing}");
    assert_eq!(
        spec_count(&listing),
        1,
        "read-only list must not import: {listing}"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_viewers_can_read_the_hierarchy_tree() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    set_env(home.path());
    let root = project(home.path()).await;
    {
        let rw = app(&root, false).await;
        let flag = call(&rw, "POST", "/api/specs/flag", json!({ "enabled": true })).await;
        assert_ne!(flag["ok"], json!(false), "{flag}");
        let tree = call(
            &rw,
            "POST",
            "/api/specs/tree",
            json!({ "projectId": "local" }),
        )
        .await;
        assert_eq!(tree["ok"], json!(true), "{tree}");
    }
    let ro = app(&root, true).await;
    let tree = call(
        &ro,
        "POST",
        "/api/specs/tree",
        json!({ "projectId": "local" }),
    )
    .await;
    assert_eq!(tree["ok"], json!(true), "{tree}");
    let provenance = call(
        &ro,
        "POST",
        "/api/specs/provenance",
        json!({ "planId": "missing", "version": 1 }),
    )
    .await;
    assert_ne!(provenance["error"], json!("readonly"), "{provenance}");
    let compare = call(
        &ro,
        "POST",
        "/api/specs/plan-compare",
        json!({ "planId": "missing", "left": 1, "right": 2 }),
    )
    .await;
    assert_ne!(compare["error"], json!("readonly"), "{compare}");
    let create = call(&ro, "POST", "/api/specs/with-plan", json!({})).await;
    assert_eq!(create["error"], json!("readonly"), "{create}");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn list_rescans_plan_folders_only_after_the_interval_or_on_refresh() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    set_env(home.path());
    let root = project(home.path()).await;
    write_plan(&root, "first.md");
    let rw = app(&root, false).await;
    assert_eq!(
        spec_count(&call(&rw, "GET", "/api/specs", Value::Null).await),
        1
    );
    write_plan(&root, "second.md");
    assert_eq!(
        spec_count(&call(&rw, "GET", "/api/specs", Value::Null).await),
        1,
        "a poll right after an import must not rescan the plan folders"
    );
    assert_eq!(
        spec_count(&call(&rw, "GET", "/api/specs?refresh=1", Value::Null).await),
        2,
        "an explicit refresh rescans"
    );
}
