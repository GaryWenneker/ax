//! Web login through the router: sign-in, sessions, recovery codes, the
//! Origin check for changes and the role rules of the gate.
//! Separate process: these tests set environment variables.

use std::net::SocketAddr;
use std::sync::{Mutex, MutexGuard};

use ax_global_db::access::{self, MemberRole};
use ax_web::WebHub;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::Router;
use sqlx::SqlitePool;
use tower::ServiceExt;
use tower_http::cors::CorsLayer;

const LOOPBACK: &str = "127.0.0.1:50000";
const LAN: &str = "198.51.100.40:4000";
const HOST: &str = "ax.corp.example";
const PASSWORD: &str = "correct horse battery";

fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

struct Fixture {
    _dir: tempfile::TempDir,
    app: Router,
    pool: SqlitePool,
}

struct Reply {
    status: StatusCode,
    body: String,
    cookie: Option<String>,
}

/// A hub with web login on, an Owner, and one member per other role. Every
/// member has the password [`PASSWORD`].
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
    std::env::set_var("AX_WEB_ALLOWED_HOSTS", HOST);
    std::env::remove_var("AX_SHARE_TOKEN");
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    ax_db::Database::open(&root.join(".ax").join("ax.db"))
        .await
        .unwrap();
    let pool = ax_global_db::open_and_init(&global).await.unwrap();
    access::bootstrap_owner(&pool, "owner", "Owner", None)
        .await
        .unwrap();
    for (id, role) in [
        ("ada", MemberRole::Admin),
        ("mem", MemberRole::Member),
        ("ro", MemberRole::Readonly),
    ] {
        access::create_member(&pool, id, id, None, role)
            .await
            .unwrap();
    }
    let app = WebHub::open(root, false, 7070)
        .await
        .unwrap()
        .nest_routers(CorsLayer::new());
    let body = format!(r#"{{"password":"{PASSWORD}"}}"#);
    let local = [("host", "127.0.0.1:7070")];
    let reply = send(&app, "POST", "/api/web-auth/setup", LOOPBACK, &local, &body).await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    for id in ["ada", "mem", "ro"] {
        let uri = format!("/api/web-auth/users/{id}/password");
        let reply = send(&app, "POST", &uri, LOOPBACK, &local, &body).await;
        assert_eq!(reply.status, StatusCode::OK, "{id}: {}", reply.body);
    }
    sqlx::query("INSERT OR REPLACE INTO web_auth_settings(singleton,enabled) VALUES (1,1)")
        .execute(&pool)
        .await
        .unwrap();
    Fixture {
        _dir: tmp,
        app,
        pool,
    }
}

/// Sends one request from `peer`. `Host` is [`HOST`] unless `headers` names one.
async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    peer: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> Reply {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if !headers.iter().any(|(name, _)| *name == "host") {
        builder = builder.header("host", HOST);
    }
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let mut req = builder.body(Body::from(body.to_string())).unwrap();
    let peer: SocketAddr = peer.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(peer));
    let response = app.clone().oneshot(req).await.unwrap();
    let status = response.status();
    let cookie = response
        .headers()
        .get("set-cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .map(str::to_string);
    let bytes = axum::body::to_bytes(response.into_body(), 1 << 20)
        .await
        .unwrap();
    Reply {
        status,
        body: String::from_utf8_lossy(&bytes).into_owned(),
        cookie,
    }
}

/// Signs `identity` in from the LAN and returns the `Cookie` header value.
async fn sign_in(app: &Router, identity: &str) -> String {
    let body = format!(r#"{{"identity":"{identity}","password":"{PASSWORD}"}}"#);
    let reply = send(app, "POST", "/api/web-auth/login", LAN, &[], &body).await;
    assert_eq!(reply.status, StatusCode::OK, "{identity}: {}", reply.body);
    reply.cookie.expect("session cookie")
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn a_same_site_page_cannot_change_data_with_a_valid_session() {
    let _lock = env_lock();
    let fx = fixture().await;
    let cookie = sign_in(&fx.app, "owner").await;
    for origin in [
        "https://wiki.corp.example",
        "https://evil.example",
        "http://localhost:7070",
    ] {
        let reply = send(
            &fx.app,
            "POST",
            "/api/memory",
            LAN,
            &[("cookie", &cookie), ("origin", origin)],
            "{}",
        )
        .await;
        assert_eq!(reply.status, StatusCode::FORBIDDEN, "{origin}");
        assert!(reply.body.contains("Untrusted request origin"), "{origin}");
    }
    // A change without any Origin is refused too.
    let reply = send(
        &fx.app,
        "POST",
        "/api/memory",
        LAN,
        &[("cookie", &cookie)],
        "{}",
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    // The page this server serves passes the Origin check.
    for origin in ["https://ax.corp.example", "http://ax.corp.example"] {
        let reply = send(
            &fx.app,
            "POST",
            "/api/memory",
            LAN,
            &[("cookie", &cookie), ("origin", origin)],
            "{}",
        )
        .await;
        assert!(
            !reply.body.contains("Untrusted request origin"),
            "{origin}: {} {}",
            reply.status,
            reply.body
        );
    }
    // Behind an HTTPS proxy, only the HTTPS origin is the page.
    let reply = send(
        &fx.app,
        "POST",
        "/api/memory",
        LAN,
        &[
            ("cookie", &cookie),
            ("origin", "http://ax.corp.example"),
            ("x-forwarded-proto", "https"),
        ],
        "{}",
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn the_configured_origin_is_the_only_trusted_one() {
    let _lock = env_lock();
    let fx = fixture().await;
    std::env::set_var("AX_WEB_AUTH_ORIGIN", "https://ax.corp.example");
    let cookie = sign_in(&fx.app, "owner").await;
    let foreign = send(
        &fx.app,
        "POST",
        "/api/memory",
        LAN,
        &[("cookie", &cookie), ("origin", "https://wiki.corp.example")],
        "{}",
    )
    .await;
    let own = send(
        &fx.app,
        "POST",
        "/api/memory",
        LAN,
        &[("cookie", &cookie), ("origin", "https://ax.corp.example")],
        "{}",
    )
    .await;
    std::env::remove_var("AX_WEB_AUTH_ORIGIN");
    assert_eq!(foreign.status, StatusCode::FORBIDDEN, "{}", foreign.body);
    assert!(
        !own.body.contains("Untrusted request origin"),
        "{} {}",
        own.status,
        own.body
    );
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn roles_decide_what_a_signed_in_member_may_reach() {
    let _lock = env_lock();
    let fx = fixture().await;
    let origin = "http://ax.corp.example";
    let ws = [
        ("connection", "upgrade"),
        ("upgrade", "websocket"),
        ("sec-websocket-version", "13"),
        ("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ=="),
        ("origin", origin),
    ];
    // Without a session nothing is reachable.
    let reply = send(&fx.app, "GET", "/api/agent/status", LAN, &[], "").await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);

    for id in ["mem", "ro"] {
        let cookie = sign_in(&fx.app, id).await;
        let c = ("cookie", cookie.as_str());
        let o = ("origin", origin);
        for (method, uri) in [
            ("GET", "/api/agent/status"),
            ("POST", "/api/agent/install"),
            ("POST", "/api/agent/chat/stream"),
            ("GET", "/api/workspace/current"),
            ("POST", "/api/lsp/enrich"),
            ("POST", "/api/dav/mount"),
            ("GET", "/api/access/team"),
        ] {
            let reply = send(&fx.app, method, uri, LAN, &[c, o], "{}").await;
            assert_eq!(
                reply.status,
                StatusCode::FORBIDDEN,
                "{id} {method} {uri}: {}",
                reply.body
            );
        }
        // The agent terminal is a WebSocket opened with GET.
        let mut headers = vec![c];
        headers.extend(ws);
        let reply = send(&fx.app, "GET", "/api/agent/pty/ws", LAN, &headers, "").await;
        assert_eq!(
            reply.status,
            StatusCode::FORBIDDEN,
            "{id} pty: {}",
            reply.body
        );
        // Reading project data is allowed for every role.
        let reply = send(&fx.app, "GET", "/api/memory", LAN, &[c], "").await;
        assert_eq!(reply.status, StatusCode::OK, "{id} memory: {}", reply.body);
    }

    // A read-only member cannot write; a member can.
    let ro = sign_in(&fx.app, "ro").await;
    let reply = send(
        &fx.app,
        "POST",
        "/api/memory",
        LAN,
        &[("cookie", &ro), ("origin", origin)],
        "{}",
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
    assert!(reply.body.contains("Write permission required"));
    let mem = sign_in(&fx.app, "mem").await;
    let reply = send(
        &fx.app,
        "POST",
        "/api/memory",
        LAN,
        &[("cookie", &mem), ("origin", origin)],
        "{}",
    )
    .await;
    assert_ne!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);

    // An Admin reaches the management routes.
    let ada = sign_in(&fx.app, "ada").await;
    for uri in ["/api/agent/status", "/api/workspace/current"] {
        let reply = send(&fx.app, "GET", uri, LAN, &[("cookie", &ada)], "").await;
        assert_eq!(reply.status, StatusCode::OK, "admin {uri}: {}", reply.body);
    }
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn five_wrong_passwords_lock_the_account_for_that_address() {
    let _lock = env_lock();
    let fx = fixture().await;
    // Its own address: the limiter is shared by every test in this process.
    let peer = "198.51.100.77:4000";
    let wrong = r#"{"identity":"mem","password":"wrong password!"}"#;
    for attempt in 1..=5 {
        let reply = send(&fx.app, "POST", "/api/web-auth/login", peer, &[], wrong).await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "attempt {attempt}");
    }
    let right = format!(r#"{{"identity":"mem","password":"{PASSWORD}"}}"#);
    let reply = send(&fx.app, "POST", "/api/web-auth/login", peer, &[], &right).await;
    assert_eq!(
        reply.status,
        StatusCode::TOO_MANY_REQUESTS,
        "{}",
        reply.body
    );
    assert!(reply.cookie.is_none());
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn expired_and_signed_out_sessions_are_refused() {
    let _lock = env_lock();
    let fx = fixture().await;
    let cookie = sign_in(&fx.app, "mem").await;
    let reply = send(
        &fx.app,
        "GET",
        "/api/memory",
        LAN,
        &[("cookie", &cookie)],
        "",
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    sqlx::query("UPDATE web_sessions SET expires_at=unixepoch()-1 WHERE member_id='mem'")
        .execute(&fx.pool)
        .await
        .unwrap();
    let reply = send(
        &fx.app,
        "GET",
        "/api/memory",
        LAN,
        &[("cookie", &cookie)],
        "",
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);

    let cookie = sign_in(&fx.app, "mem").await;
    let reply = send(
        &fx.app,
        "POST",
        "/api/web-auth/logout",
        LAN,
        &[("cookie", &cookie)],
        "",
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let reply = send(
        &fx.app,
        "GET",
        "/api/memory",
        LAN,
        &[("cookie", &cookie)],
        "",
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn a_recovery_code_signs_in_once() {
    let _lock = env_lock();
    let fx = fixture().await;
    let cookie = sign_in(&fx.app, "mem").await;
    let reply = send(
        &fx.app,
        "POST",
        "/api/web-auth/recovery/rotate",
        LAN,
        &[("cookie", &cookie), ("origin", "http://ax.corp.example")],
        "",
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    let codes: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    let code = codes["codes"][0].as_str().unwrap().to_string();
    let body = format!(r#"{{"identity":"mem","code":"{code}"}}"#);
    let first = send(
        &fx.app,
        "POST",
        "/api/web-auth/recovery/login",
        LAN,
        &[],
        &body,
    )
    .await;
    assert_eq!(first.status, StatusCode::OK, "{}", first.body);
    assert!(first.cookie.is_some());
    let again = send(
        &fx.app,
        "POST",
        "/api/web-auth/recovery/login",
        LAN,
        &[],
        &body,
    )
    .await;
    assert_eq!(again.status, StatusCode::UNAUTHORIZED, "{}", again.body);
    // A code belongs to one member only.
    let other = codes["codes"][1].as_str().unwrap();
    let body = format!(r#"{{"identity":"ada","code":"{other}"}}"#);
    let reply = send(
        &fx.app,
        "POST",
        "/api/web-auth/recovery/login",
        LAN,
        &[],
        &body,
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // process-global env; the guard must cover the awaits
async fn only_the_local_owner_manages_without_a_session() {
    let _lock = env_lock();
    let fx = fixture().await;
    let body = r#"{"password":"another long password"}"#;
    let local = [("host", "127.0.0.1:7070")];
    // A loopback peer that a proxy forwards is someone else.
    let proxied = [
        ("host", "127.0.0.1:7070"),
        ("x-forwarded-for", "203.0.113.9"),
    ];
    let reply = send(
        &fx.app,
        "POST",
        "/api/web-auth/users/mem/password",
        LOOPBACK,
        &proxied,
        body,
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
    // A LAN peer without a session is refused.
    let reply = send(
        &fx.app,
        "POST",
        "/api/web-auth/users/mem/password",
        LAN,
        &local,
        body,
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
    // The person at this computer may.
    let reply = send(
        &fx.app,
        "POST",
        "/api/web-auth/users/mem/password",
        LOOPBACK,
        &local,
        body,
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    // A non-owner session cannot set another member's password.
    let ada = sign_in(&fx.app, "ada").await;
    let reply = send(
        &fx.app,
        "POST",
        "/api/web-auth/users/mem/password",
        LAN,
        &[("cookie", &ada), ("origin", "http://ax.corp.example")],
        body,
    )
    .await;
    assert_eq!(reply.status, StatusCode::FORBIDDEN, "{}", reply.body);
}
