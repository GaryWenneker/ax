//! Who may use the workspace routes (browse, switch, create, init, purge,
//! recent list) with web sign-in off.
//! Separate process: these tests set environment variables.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

const LOOPBACK: &str = "127.0.0.1:50000";
const LAN: &str = "198.51.100.50:4000";

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

struct Fixture {
    _dir: tempfile::TempDir,
    dir: PathBuf,
    app: Router,
}

async fn fixture(readonly: bool) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    std::env::set_var("HOME", &dir);
    std::env::set_var("AX_HOME_DIR", &dir);
    std::env::set_var("AX_GLOBAL_DB", dir.join("global.db"));
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let app = WebHub::open(root, readonly, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new());
    Fixture {
        _dir: tmp,
        dir,
        app,
    }
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    peer: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (StatusCode, String) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .header("content-type", "application/json");
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut req = builder.body(Body::from(body.to_string())).unwrap();
    let peer: SocketAddr = peer.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

fn recent_list(dir: &std::path::Path) -> String {
    std::fs::read_to_string(dir.join(".ax").join("config.json")).unwrap_or_default()
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn only_the_local_browser_uses_workspace_routes_without_sign_in() {
    let _lock = env_lock();
    let fx = fixture(false).await;
    let browse = format!("/api/workspace/browse?path={}", fx.dir.display());
    let mkdir = serde_json::json!({"parent": fx.dir, "name": "made"}).to_string();
    let add = serde_json::json!({"path": fx.dir.join("elsewhere")}).to_string();
    let foreign = [("origin", "http://evil.example")];
    let proxied = [("x-forwarded-for", "203.0.113.9")];
    for (peer, headers) in [
        (LAN, &[][..]),
        (LOOPBACK, &foreign[..]),
        (LOOPBACK, &proxied[..]),
    ] {
        for (method, uri, body) in [
            ("GET", browse.as_str(), ""),
            ("GET", "/api/workspace/current", ""),
            ("POST", "/api/workspace/mkdir", mkdir.as_str()),
            ("POST", "/api/workspace/recent/add", add.as_str()),
        ] {
            let (status, text) = send(&fx.app, method, uri, peer, headers, body).await;
            assert_eq!(
                status,
                StatusCode::FORBIDDEN,
                "{peer} {headers:?} {method} {uri}: {text}"
            );
        }
    }
    assert!(!fx.dir.join("made").exists());
    assert!(!recent_list(&fx.dir).contains("elsewhere"));

    // The person at this computer, in the Command Center page.
    let own = [("origin", "http://127.0.0.1:7070")];
    let (status, text) = send(&fx.app, "GET", &browse, LOOPBACK, &[], "").await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(text.contains("\"ok\":true"), "{text}");
    let (status, text) = send(
        &fx.app,
        "POST",
        "/api/workspace/mkdir",
        LOOPBACK,
        &own,
        &mkdir,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(fx.dir.join("made").is_dir());

    // Any folder may be browsed: the picker walks the whole disk. The root is
    // `/` on Unix and the drive (`C:\`) on Windows.
    let root = fx
        .dir
        .ancestors()
        .last()
        .expect("a root")
        .display()
        .to_string();
    let (_, text) = send(
        &fx.app,
        "GET",
        &format!("/api/workspace/browse?path={root}"),
        LOOPBACK,
        &[],
        "",
    )
    .await;
    assert!(text.contains("\"ok\":true"), "{text}");
    // A relative path would resolve against the server's working directory.
    let relative = serde_json::json!({"parent": "relative-parent", "name": "x"}).to_string();
    let (_, text) = send(
        &fx.app,
        "POST",
        "/api/workspace/mkdir",
        LOOPBACK,
        &own,
        &relative,
    )
    .await;
    assert!(text.contains("must be absolute"), "{text}");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_mode_does_not_change_the_recent_list() {
    let _lock = env_lock();
    let fx = fixture(true).await;
    let add = serde_json::json!({"path": fx.dir.join("elsewhere")}).to_string();
    let (_, text) = send(
        &fx.app,
        "POST",
        "/api/workspace/recent/add",
        LOOPBACK,
        &[],
        &add,
    )
    .await;
    assert!(text.contains("Read-only mode"), "{text}");
    assert!(!recent_list(&fx.dir).contains("elsewhere"));
}

async fn post_json(fx: &Fixture, uri: &str, body: serde_json::Value) -> String {
    send(&fx.app, "POST", uri, LOOPBACK, &[], &body.to_string())
        .await
        .1
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn reading_the_current_project_does_not_write_the_recent_list() {
    let _lock = env_lock();
    let fx = fixture(false).await;
    let (status, text) = send(&fx.app, "GET", "/api/workspace/current", LOOPBACK, &[], "").await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(
        !recent_list(&fx.dir).contains("proj"),
        "GET /current wrote the recent list: {}",
        recent_list(&fx.dir)
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn mkdir_takes_one_plain_folder_name() {
    let _lock = env_lock();
    let fx = fixture(false).await;
    let parent = fx.dir.to_string_lossy().to_string();
    for name in ["..", ".", "a/b", "a\\b", "", "x..y"] {
        let text = post_json(
            &fx,
            "/api/workspace/mkdir",
            serde_json::json!({"parent": parent, "name": name}),
        )
        .await;
        assert!(text.contains("Invalid directory name"), "{name}: {text}");
    }
    let text = post_json(
        &fx,
        "/api/workspace/mkdir",
        serde_json::json!({"parent": parent, "name": "fresh"}),
    )
    .await;
    assert!(text.contains("\"created\":true"), "{text}");
    assert!(fx.dir.join("fresh").is_dir());
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_mode_refuses_every_folder_change() {
    let _lock = env_lock();
    let fx = fixture(true).await;
    let path = fx.dir.join("proj").to_string_lossy().to_string();
    let parent = fx.dir.to_string_lossy().to_string();
    for (uri, body) in [
        ("/api/workspace/switch", serde_json::json!({"path": path})),
        (
            "/api/workspace/mkdir",
            serde_json::json!({"parent": parent, "name": "made"}),
        ),
        (
            "/api/workspace/init/stream",
            serde_json::json!({"path": parent}),
        ),
        (
            "/api/workspace/purge",
            serde_json::json!({"path": path, "groups": ["database"]}),
        ),
    ] {
        let text = post_json(&fx, uri, body).await;
        assert!(text.contains("Read-only mode"), "{uri}: {text}");
    }
    assert!(!fx.dir.join("made").exists());
    assert!(fx.dir.join("proj/.ax/ax.db").is_file());
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn purging_a_missing_folder_removes_nothing_on_disk() {
    let _lock = env_lock();
    let fx = fixture(false).await;
    let gone = fx.dir.join("gone").to_string_lossy().to_string();
    let text = post_json(
        &fx,
        "/api/workspace/purge",
        serde_json::json!({"path": gone, "groups": ["database", "recent"]}),
    )
    .await;
    let reply: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(reply["ok"], true, "{text}");
    assert_eq!(reply["removed"], serde_json::json!([]), "{text}");
    assert!(fx.dir.join("proj/.ax/ax.db").is_file());
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn purge_forgets_the_project_in_the_configured_global_database() {
    let _lock = env_lock();
    let fx = fixture(false).await;
    // AX_GLOBAL_DB (set by the fixture) is not HOME/.ax/global.db.
    let global_path = fx.dir.join("global.db");
    let other = fx.dir.join("other");
    std::fs::create_dir_all(&other).unwrap();
    let global = ax_global_db::open_and_init(&global_path).await.unwrap();
    ax_global_db::policy::ensure_project(&global, &other)
        .await
        .unwrap();
    let count = || async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM projects")
            .fetch_one(&global)
            .await
            .unwrap()
    };
    assert_eq!(count().await, 1);
    let text = post_json(
        &fx,
        "/api/workspace/purge",
        serde_json::json!({"path": other.to_string_lossy(), "groups": ["globalIndex"]}),
    )
    .await;
    assert!(text.contains("\"ok\":true"), "{text}");
    assert_eq!(count().await, 0, "the project row is still in AX_GLOBAL_DB");
    global.close().await;
}
