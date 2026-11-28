//! The web-login gate and the password login limits.
//! Separate process: these tests set environment variables.

use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard};

use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use sqlx::SqlitePool;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

const LOOPBACK: &str = "127.0.0.1:50000";

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

struct Fixture {
    _dir: tempfile::TempDir,
    dir: std::path::PathBuf,
    app: Router,
    pool: SqlitePool,
}

/// A hub with an Owner whose password is set and web login turned on.
async fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().to_path_buf();
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let global = dir.join("global.db");
    std::env::set_var("HOME", &dir);
    std::env::set_var("AX_HOME_DIR", &dir);
    std::env::set_var("AX_GLOBAL_DB", &global);
    std::env::set_var("AX_NO_IDE_PANEL", "1");
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    std::env::remove_var("AX_WEB_ALLOWED_HOSTS");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let pool = ax_global_db::open_and_init(&global).await.unwrap();
    ax_global_db::access::bootstrap_owner(&pool, "owner", "Owner", None)
        .await
        .unwrap();
    let app = WebHub::open(root, false, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new());
    let (status, body) = send(
        &app,
        "POST",
        "/api/web-auth/setup",
        LOOPBACK,
        &[],
        r#"{"password":"correct horse battery"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    sqlx::query("INSERT OR REPLACE INTO web_auth_settings(singleton,enabled) VALUES (1,1)")
        .execute(&pool)
        .await
        .unwrap();
    Fixture {
        _dir: tmp,
        dir,
        app,
        pool,
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

/// Sends every login at once and returns the statuses.
async fn burst(app: &Router, peer: &'static str, identities: Vec<String>) -> Vec<StatusCode> {
    let mut tasks = Vec::new();
    for identity in identities {
        let app = app.clone();
        tasks.push(tokio::spawn(async move {
            let body = serde_json::json!({ "identity": identity, "password": "wrong password!" });
            send(
                &app,
                "POST",
                "/api/web-auth/login",
                peer,
                &[],
                &body.to_string(),
            )
            .await
            .0
        }));
    }
    let mut out = Vec::new();
    for task in tasks {
        out.push(task.await.unwrap());
    }
    out
}

fn count(statuses: &[StatusCode], wanted: StatusCode) -> usize {
    statuses.iter().filter(|status| **status == wanted).count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn a_parallel_burst_gets_no_more_than_five_guesses() {
    let _lock = env_lock();
    let fx = fixture().await;
    let statuses = burst(&fx.app, "198.51.100.9:4000", vec!["owner".into(); 12]).await;
    assert_eq!(
        count(&statuses, StatusCode::UNAUTHORIZED),
        5,
        "{statuses:?}"
    );
    assert_eq!(
        count(&statuses, StatusCode::TOO_MANY_REQUESTS),
        7,
        "{statuses:?}"
    );
    // The correct password from another address still signs in.
    let (status, body) = send(
        &fx.app,
        "POST",
        "/api/web-auth/login",
        "198.51.100.10:4000",
        &[],
        r#"{"identity":"owner","password":"correct horse battery"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn one_address_cannot_spread_guesses_over_many_identities() {
    let _lock = env_lock();
    let fx = fixture().await;
    let identities: Vec<String> = (0..36).map(|n| format!("nobody-{n}")).collect();
    let statuses = burst(&fx.app, "198.51.100.20:4000", identities).await;
    assert_eq!(
        count(&statuses, StatusCode::UNAUTHORIZED),
        30,
        "{statuses:?}"
    );
    assert_eq!(
        count(&statuses, StatusCode::TOO_MANY_REQUESTS),
        6,
        "{statuses:?}"
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn static_assets_load_while_the_global_database_cannot_open() {
    let _lock = env_lock();
    let fx = fixture().await;
    // A path below a regular file can never be opened.
    let blocker = fx.dir.join("not-a-dir");
    std::fs::write(&blocker, "x").unwrap();
    std::env::set_var("AX_GLOBAL_DB", blocker.join("global.db"));
    let (shell, _) = send(&fx.app, "GET", "/", LOOPBACK, &[], "").await;
    let (asset, _) = send(&fx.app, "GET", "/assets/missing.js", LOOPBACK, &[], "").await;
    let (api, _) = send(&fx.app, "GET", "/api/stats", LOOPBACK, &[], "").await;
    std::env::set_var("AX_GLOBAL_DB", fx.dir.join("global.db"));
    assert_ne!(shell, StatusCode::INTERNAL_SERVER_ERROR, "SPA shell");
    assert_ne!(asset, StatusCode::INTERNAL_SERVER_ERROR, "static asset");
    // Data APIs still fail closed.
    assert_eq!(api, StatusCode::INTERNAL_SERVER_ERROR, "data API");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn scoped_client_tokens_keep_working_with_web_login_on() {
    let _lock = env_lock();
    let fx = fixture().await;
    ax_global_db::access::create_client(
        &fx.pool,
        "cli",
        "owner",
        "CLI",
        "cli",
        &["memory:read".to_string(), "context:read".to_string()],
    )
    .await
    .unwrap();
    let issued = ax_global_db::access::issue_client_credential(&fx.pool, "cli", None)
        .await
        .unwrap();
    let bearer = format!("Bearer {}", issued.token);
    let auth = [("authorization", bearer.as_str())];

    let (memory, body) = send(&fx.app, "GET", "/api/memory", LOOPBACK, &auth, "").await;
    assert_eq!(memory, StatusCode::OK, "memory:read {body}");
    let (stats, body) = send(&fx.app, "GET", "/api/stats", LOOPBACK, &auth, "").await;
    assert_eq!(stats, StatusCode::OK, "context:read {body}");
    // Scopes still apply: this token cannot write memory.
    let (write, body) = send(&fx.app, "POST", "/api/memory", LOOPBACK, &auth, "{}").await;
    assert_eq!(write, StatusCode::FORBIDDEN, "memory:write {body}");
    // A token that is not valid is refused.
    let fake = [(
        "authorization",
        "Bearer axc_0000000000000000000000000000000000000000000000",
    )];
    let (bad, body) = send(&fx.app, "GET", "/api/memory", LOOPBACK, &fake, "").await;
    assert_eq!(bad, StatusCode::UNAUTHORIZED, "{body}");
    // Without a token, the session is still required.
    let (anon, body) = send(&fx.app, "GET", "/api/memory", LOOPBACK, &[], "").await;
    assert_eq!(anon, StatusCode::UNAUTHORIZED, "{body}");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn password_hashing_does_not_stall_other_requests() {
    let _lock = env_lock();
    let fx = fixture().await;
    // A current-thread runtime: anything hashed inline stops the ticker.
    let started = std::time::Instant::now();
    let login = {
        let app = fx.app.clone();
        tokio::spawn(async move {
            send(
                &app,
                "POST",
                "/api/web-auth/login",
                "198.51.100.30:4000",
                &[],
                r#"{"identity":"owner","password":"wrong password!"}"#,
            )
            .await
            .0
        })
    };
    let mut longest = std::time::Duration::ZERO;
    let mut last = std::time::Instant::now();
    while !login.is_finished() {
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        longest = longest.max(last.elapsed());
        last = std::time::Instant::now();
    }
    let total = started.elapsed();
    assert_eq!(login.await.unwrap(), StatusCode::UNAUTHORIZED);
    assert!(
        longest * 2 < total,
        "the runtime stalled for {longest:?} of a {total:?} login"
    );
}
