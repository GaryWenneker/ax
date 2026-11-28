//! Who may call `/api/access`: client scopes per route, the local owner's
//! CSRF token, member role rules and read-only mode.
//! Separate process: these tests set environment variables.

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
const LOOPBACK: &str = "127.0.0.1:50000";
const LAN: &str = "198.51.100.50:4000";

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

struct Fixture {
    _dir: tempfile::TempDir,
    app: Router,
    pool: SqlitePool,
}

async fn client_token(pool: &SqlitePool, id: &str, member: &str, scopes: &[&str]) -> String {
    let scopes: Vec<String> = scopes.iter().map(|s| s.to_string()).collect();
    access::create_client(pool, id, member, id, "cli", &scopes)
        .await
        .unwrap();
    access::issue_client_credential(pool, id, None)
        .await
        .unwrap()
        .token
}

async fn fixture(readonly: bool) -> Fixture {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
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
    access::bootstrap_owner(&pool, OWNER, "Owner", None)
        .await
        .unwrap();
    access::create_member(&pool, "admin-1", "Admin", None, MemberRole::Admin)
        .await
        .unwrap();
    let app = ax_web::WebHub::open(root, readonly, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new());
    Fixture {
        _dir: tmp,
        app,
        pool,
    }
}

async fn call(
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
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn client_scopes_follow_the_route_not_a_substring_of_its_id() {
    let _lock = env_lock();
    let f = fixture(false).await;
    let team = client_token(&f.pool, "team-cli", "admin-1", &["admin:team"]).await;
    let clients = client_token(&f.pool, "clients-cli", "admin-1", &["admin:clients"]).await;
    let team_auth = format!("Bearer {team}");
    let clients_auth = format!("Bearer {clients}");
    let as_team = [("authorization", team_auth.as_str())];
    let as_clients = [("authorization", clients_auth.as_str())];

    type Headers<'a> = &'a [(&'a str, &'a str)];
    let cases: &[(&str, &str, Headers, StatusCode)] = &[
        ("GET", "/api/access/team", &as_team, StatusCode::OK),
        (
            "GET",
            "/api/access/clients",
            &as_team,
            StatusCode::FORBIDDEN,
        ),
        ("GET", "/api/access/nodes", &as_team, StatusCode::FORBIDDEN),
        ("GET", "/api/access/clients", &as_clients, StatusCode::OK),
        (
            "GET",
            "/api/access/team",
            &as_clients,
            StatusCode::FORBIDDEN,
        ),
        (
            "GET",
            "/api/access/nodes",
            &as_clients,
            StatusCode::FORBIDDEN,
        ),
        // An id that happens to contain "/team" or "/nodes" does not change
        // which scope the route needs.
        (
            "POST",
            "/api/access/nodes/team-node/revoke",
            &as_team,
            StatusCode::FORBIDDEN,
        ),
        (
            "POST",
            "/api/access/clients/team-cli/revoke",
            &as_team,
            StatusCode::FORBIDDEN,
        ),
        (
            "GET",
            "/api/access/team",
            &[("authorization", "Bearer axc_not-a-token")],
            StatusCode::UNAUTHORIZED,
        ),
    ];
    for (method, uri, headers, expected) in cases {
        let (status, text) = call(&f.app, method, uri, LOOPBACK, headers, "{}").await;
        assert_eq!(status, *expected, "{method} {uri} {headers:?}: {text}");
    }
    // The refused revoke left the client alone.
    let all = access::list_clients(&f.pool).await.unwrap();
    assert!(all
        .iter()
        .any(|c| c.id == "team-cli" && c.revoked_at.is_none()));
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn the_local_owner_needs_the_csrf_token_to_write() {
    let _lock = env_lock();
    let f = fixture(false).await;
    let member = r#"{"display_name":"New","role":"member"}"#;

    // Someone on the network without a credential gets nothing.
    let (status, _) = call(&f.app, "GET", "/api/access/team", LAN, &[], "").await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let (status, text) = call(&f.app, "GET", "/api/access/session", LOOPBACK, &[], "").await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let csrf = serde_json::from_str::<serde_json::Value>(&text).unwrap()["csrf"]
        .as_str()
        .unwrap()
        .to_string();

    for headers in [vec![], vec![("x-ax-access-csrf", "wrong")]] {
        let (status, text) = call(
            &f.app,
            "POST",
            "/api/access/team",
            LOOPBACK,
            &headers,
            member,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{headers:?}: {text}");
    }
    let (status, text) = call(
        &f.app,
        "POST",
        "/api/access/team",
        LOOPBACK,
        &[("x-ax-access-csrf", csrf.as_str())],
        member,
    )
    .await;
    assert!(status.is_success(), "{status}: {text}");
    let members = access::list_members(&f.pool).await.unwrap();
    assert!(members.iter().any(|m| m.display_name == "New"));
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn an_admin_cannot_create_change_or_remove_an_owner() {
    let _lock = env_lock();
    let f = fixture(false).await;
    let admin = client_token(&f.pool, "admin-team", "admin-1", &["admin:team"]).await;
    let auth = format!("Bearer {admin}");
    let as_admin = [("authorization", auth.as_str())];

    let (status, text) = call(
        &f.app,
        "POST",
        "/api/access/team",
        LOOPBACK,
        &as_admin,
        r#"{"display_name":"Boss","role":"owner"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "create owner: {text}");
    let (status, text) = call(
        &f.app,
        "PATCH",
        "/api/access/team/admin-1",
        LOOPBACK,
        &as_admin,
        r#"{"roles":["owner"],"status":"active"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "promote: {text}");
    let (status, text) = call(
        &f.app,
        "PATCH",
        &format!("/api/access/team/{OWNER}"),
        LOOPBACK,
        &as_admin,
        r#"{"roles":["member"],"status":"active"}"#,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "demote owner: {text}");
    let (status, text) = call(
        &f.app,
        "DELETE",
        &format!("/api/access/team/{OWNER}"),
        LOOPBACK,
        &as_admin,
        "",
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "remove owner: {text}");

    // A plain member is still created.
    let (status, text) = call(
        &f.app,
        "POST",
        "/api/access/team",
        LOOPBACK,
        &as_admin,
        r#"{"display_name":"Dev","role":"member"}"#,
    )
    .await;
    assert!(status.is_success(), "{status}: {text}");
    let members = access::list_members(&f.pool).await.unwrap();
    let owners = members
        .iter()
        .filter(|m| m.roles.contains(&MemberRole::Owner))
        .count();
    assert_eq!(owners, 1);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn read_only_mode_refuses_every_access_write() {
    let _lock = env_lock();
    let f = fixture(true).await;
    let admin = client_token(
        &f.pool,
        "ro-cli",
        OWNER,
        &["admin:team", "admin:clients", "admin:nodes"],
    )
    .await;
    let auth = format!("Bearer {admin}");
    let as_owner = [("authorization", auth.as_str())];
    let (status, _) = call(&f.app, "GET", "/api/access/team", LOOPBACK, &as_owner, "").await;
    assert_eq!(status, StatusCode::OK);
    for (method, uri) in [
        ("POST", "/api/access/team"),
        ("PATCH", "/api/access/team/admin-1"),
        ("DELETE", "/api/access/team/admin-1"),
        ("POST", "/api/access/clients"),
        ("POST", "/api/access/clients/ro-cli/rotate"),
        ("POST", "/api/access/nodes/x/revoke"),
    ] {
        let (status, text) = call(&f.app, method, uri, LOOPBACK, &as_owner, "{}").await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}: {text}");
    }
}
