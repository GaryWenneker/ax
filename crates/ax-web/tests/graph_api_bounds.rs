//! Graph read routes keep their page-size caps for any query value, and the
//! source route serves only indexed files of a bounded size.
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

/// A project with three indexed files (`a.rs`, `b.rs`, `c.rs`) on disk plus
/// an unindexed `.env` and `notes.txt`.
async fn app(home: &std::path::Path) -> Router {
    std::env::set_var("HOME", home);
    std::env::set_var("AX_HOME_DIR", home);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::set_var("AX_GLOBAL_DB", home.join("global.db"));
    std::env::remove_var("AX_SHARE_TOKEN");
    let root = home.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let db = ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    for name in ["a.rs", "b.rs", "c.rs"] {
        std::fs::write(root.join(name), "fn main() {}\nfn two() {}\n").unwrap();
        sqlx::query("INSERT INTO files VALUES (?,'hash','rust',1,1,1,0,NULL)")
            .bind(name)
            .execute(db.pool())
            .await
            .unwrap();
    }
    std::fs::write(root.join(".env"), "SECRET=hunter2\n").unwrap();
    std::fs::write(root.join("notes.txt"), "private\n").unwrap();
    let hub = WebHub::open(root, true, 7070).await.unwrap();
    hub.nest_routers(CorsLayer::new())
}

async fn get(app: &Router, uri: &str) -> (StatusCode, serde_json::Value) {
    let mut req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .body(Body::empty())
        .unwrap();
    let peer: SocketAddr = "127.0.0.1:50000".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn negative_limits_and_offsets_stay_bounded() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path()).await;

    // SQLite reads LIMIT -1 as "no limit"; the route must clamp to at least 1.
    let (status, body) = get(&app, "/api/files?limit=-1").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["files"].as_array().unwrap().len(), 1, "{body}");
    assert_eq!(body["total"], 3);

    let (status, body) = get(&app, "/api/files?limit=2&offset=-5").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["files"][0]["path"], "a.rs", "{body}");

    for uri in [
        "/api/nodes?limit=-1",
        "/api/search?q=a&limit=-1",
        "/api/unresolved?limit=-1",
    ] {
        let (status, body) = get(&app, uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn source_line_math_does_not_overflow() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path()).await;

    let max = i64::MAX;
    for uri in [
        format!("/api/source?path=a.rs&start={max}"),
        format!("/api/source?path=a.rs&start=1&end={max}&context={max}"),
        format!("/api/source?path=a.rs&start={}", i64::MIN),
    ] {
        let (status, body) = get(&app, &uri).await;
        assert_eq!(status, StatusCode::OK, "{uri}: {body}");
    }
    let (_, body) = get(&app, "/api/source?path=a.rs&start=1&end=1&context=0").await;
    assert_eq!(body["lines"].as_array().unwrap().len(), 1, "{body}");
    assert_eq!(body["lines"][0]["text"], "fn main() {}");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn source_serves_only_indexed_files() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path()).await;

    let (status, body) = get(&app, "/api/source?path=a.rs").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, _) = get(&app, "/api/source?path=./a.rs").await;
    assert_eq!(status, StatusCode::OK);

    for path in [".env", "notes.txt", ".ax/ax.db"] {
        let (status, body) = get(&app, &format!("/api/source?path={path}")).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}: {body}");
        assert!(
            !body.to_string().contains("hunter2"),
            "{path} leaked: {body}"
        );
    }
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn source_refuses_files_over_the_size_cap() {
    let _lock = env_lock();
    let home = tempfile::tempdir().unwrap();
    let app = app(home.path()).await;

    // An indexed file that grew past the cap after indexing.
    let big = "x".repeat(1024) + "\n";
    let content = big.repeat(9 * 1024);
    std::fs::write(home.path().join("proj").join("a.rs"), content).unwrap();
    let (status, body) = get(&app, "/api/source?path=a.rs").await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "{}", body["error"]);
}
