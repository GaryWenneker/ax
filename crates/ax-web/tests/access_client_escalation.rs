//! An Admin must not create, issue, rotate or widen a client credential for
//! an Owner: such a token would authenticate as the Owner.

use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard};

use ax_global_db::access::{self, MemberRole};
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use sqlx::SqlitePool;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

const OWNER: &str = "local-owner";

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn scopes(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// An app, the pool, an Admin bearer token, and a client that belongs to the
/// Owner.
async fn setup(dir: &std::path::Path) -> (Router, SqlitePool, String, String) {
    let root = dir.join("proj");
    std::fs::create_dir_all(root.join(".ax")).unwrap();
    let global = dir.join("global.db");
    std::env::set_var("AX_HOME_DIR", dir);
    std::env::set_var("AX_GLOBAL_DB", &global);
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let pool = ax_global_db::open_and_init(&global).await.unwrap();
    access::bootstrap_owner(&pool, OWNER, "Owner", None)
        .await
        .unwrap();
    access::create_member(&pool, "admin-1", "Admin", None, MemberRole::Admin)
        .await
        .unwrap();
    access::create_client(
        &pool,
        "admin-cli",
        "admin-1",
        "Admin CLI",
        "cli",
        &scopes(&["admin:clients", "admin:team"]),
    )
    .await
    .unwrap();
    let token = access::issue_client_credential(&pool, "admin-cli", None)
        .await
        .unwrap()
        .token;
    access::create_client(
        &pool,
        "owner-cli",
        OWNER,
        "Owner CLI",
        "cli",
        &scopes(&["memory:read"]),
    )
    .await
    .unwrap();
    let hub = ax_web::WebHub::open(root, false, 7070).await.unwrap();
    (
        hub.nest_routers(CorsLayer::new()),
        pool,
        token,
        "owner-cli".into(),
    )
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    token: &str,
    body: &str,
) -> (StatusCode, String) {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header("host", "127.0.0.1:7070")
        .header("authorization", format!("Bearer {token}"))
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
async fn an_admin_cannot_mint_owner_credentials() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let (app, pool, admin, owner_client) = setup(dir.path()).await;

    let (status, text) = call(
        &app,
        "POST",
        "/api/access/clients",
        &admin,
        &format!(
            r#"{{"member_id":"{OWNER}","display_name":"x","client_type":"cli","scopes":["admin:team"]}}"#
        ),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "create: {text}");

    let (status, text) = call(
        &app,
        "POST",
        &format!("/api/access/clients/{owner_client}/token"),
        &admin,
        "{}",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "issue: {text}");

    let (status, text) = call(
        &app,
        "POST",
        &format!("/api/access/clients/{owner_client}/rotate"),
        &admin,
        "{}",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "rotate: {text}");

    let (status, text) = call(
        &app,
        "PATCH",
        &format!("/api/access/clients/{owner_client}/scopes"),
        &admin,
        r#"{"scopes":["admin:team","admin:clients"]}"#,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "scopes: {text}");

    // Nothing was created or changed for the Owner.
    let clients = access::list_clients(&pool).await.unwrap();
    assert_eq!(clients.iter().filter(|c| c.member_id == OWNER).count(), 1);
    let credentials = serde_json::to_value(access::list_client_credentials(&pool).await.unwrap())
        .unwrap()
        .to_string();
    assert!(!credentials.contains(&owner_client), "{credentials}");
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn an_admin_still_manages_its_own_and_lower_credentials() {
    let _lock = env_lock();
    let dir = tempfile::tempdir().unwrap();
    let (app, pool, admin, _) = setup(dir.path()).await;
    access::create_member(&pool, "member-1", "Member", None, MemberRole::Member)
        .await
        .unwrap();
    for member in ["admin-1", "member-1"] {
        let (status, text) = call(
            &app,
            "POST",
            "/api/access/clients",
            &admin,
            &format!(
                r#"{{"member_id":"{member}","display_name":"x","client_type":"cli","scopes":["memory:read"]}}"#
            ),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{member}: {text}");
        let id = serde_json::from_str::<serde_json::Value>(&text).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let (status, text) = call(
            &app,
            "POST",
            &format!("/api/access/clients/{id}/token"),
            &admin,
            "{}",
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{member}: {text}");
    }
}
