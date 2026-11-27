//! Optional share-token gate for `ax share` / LAN exposure.
//!
//! Tokens are compared in constant time. A socket peer that presents wrong
//! credentials [`MAX_FAILURES`] times within [`FAILURE_WINDOW`] gets 429 for
//! every credential it presents until the window ends, the correct one
//! included, and none of them is checked. A reverse proxy or tunnel on this computer is not
//! limited this way, because all its clients share its address. Failed client
//! and node credentials are written to the security audit log.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::IntoResponse;
use axum::response::Response;
use subtle::ConstantTimeEq;

/// Wrong credentials one socket peer may present within [`FAILURE_WINDOW`].
const MAX_FAILURES: u32 = 20;
const FAILURE_WINDOW: Duration = Duration::from_secs(60);
/// Peers tracked at once; expired entries are dropped when this is reached.
const MAX_TRACKED_PEERS: usize = 4096;

struct Failures {
    count: u32,
    since: Instant,
}

static FAILURES: Mutex<Option<HashMap<IpAddr, Failures>>> = Mutex::new(None);

/// True while `peer` has used up its failed attempts in the current window.
fn peer_locked(peer: Option<IpAddr>) -> bool {
    let Some(peer) = peer else { return false };
    let mut guard = FAILURES.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    match map.get(&peer) {
        Some(f) if f.since.elapsed() >= FAILURE_WINDOW => {
            map.remove(&peer);
            false
        }
        Some(f) => f.count >= MAX_FAILURES,
        None => false,
    }
}

fn record_failure(peer: Option<IpAddr>) {
    let Some(peer) = peer else { return };
    let mut guard = FAILURES.lock().unwrap_or_else(|p| p.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if map.len() >= MAX_TRACKED_PEERS && !map.contains_key(&peer) {
        map.retain(|_, f| f.since.elapsed() < FAILURE_WINDOW);
        if map.len() >= MAX_TRACKED_PEERS {
            return;
        }
    }
    let entry = map.entry(peer).or_insert(Failures {
        count: 0,
        since: Instant::now(),
    });
    if entry.since.elapsed() >= FAILURE_WINDOW {
        *entry = Failures {
            count: 0,
            since: Instant::now(),
        };
    }
    entry.count = entry.count.saturating_add(1);
}

fn too_many_failures() -> Response {
    (
        StatusCode::TOO_MANY_REQUESTS,
        [(header::RETRY_AFTER, FAILURE_WINDOW.as_secs().to_string())],
        axum::Json(serde_json::json!({"error":"Too many failed attempts; try again later"})),
    )
        .into_response()
}

/// Constant-time token comparison (the length is not secret).
fn token_eq(provided: &str, expected: &str) -> bool {
    provided.len() == expected.len() && bool::from(provided.as_bytes().ct_eq(expected.as_bytes()))
}

/// The address failures are counted against: the socket peer, or `None` for
/// a reverse proxy or tunnel on this computer, whose clients all share it.
fn failure_peer(req: &Request) -> Option<IpAddr> {
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    if crate::request_guard::peer_is_local_proxy(peer, req.headers()) {
        return None;
    }
    peer.map(|addr| addr.ip().to_canonical())
}

async fn audit_failed_credential(
    db: &sqlx::SqlitePool,
    kind: &str,
    path: &str,
    peer: Option<IpAddr>,
) {
    let _ = ax_global_db::access::audit(
        db,
        None,
        None,
        "authentication.failed",
        "http",
        path,
        &serde_json::json!({"credential": kind, "peer": peer.map(|p| p.to_string())}),
    )
    .await;
}

pub async fn share_token_middleware(req: Request, next: Next) -> Response {
    enforce_share_token(req, next, std::env::var("AX_SHARE_TOKEN").ok()).await
}

pub(crate) async fn enforce_share_token(
    req: Request,
    next: Next,
    configured: Option<String>,
) -> Response {
    let peer = failure_peer(&req);
    let client_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    if let Some(token) =
        client_token.filter(|token| token.starts_with("axc_") || token.starts_with("axn_"))
    {
        // A locked peer gets 429 before its credential is checked, so the
        // answer never tells a guesser whether this guess was right.
        if peer_locked(peer) {
            return too_many_failures();
        }
        let db = match crate::global_pool::global_pool().await {
            Ok(db) => db,
            Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        };
        if token.starts_with("axn_") {
            // Node credentials authenticate replication traffic only; the
            // Command Center HTTP API has no node scopes.
            record_failure(peer);
            audit_failed_credential(&db, "node", req.uri().path(), peer).await;
            return (
                StatusCode::UNAUTHORIZED,
                axum::Json(serde_json::json!({
                    "error": "Node credentials are accepted only by the replication server; use a client credential (axc_) for this API"
                })),
            )
                .into_response();
        }
        let principal = match ax_global_db::access::authenticate_client_token(&db, token).await {
            Ok(Some(p)) => p,
            _ => {
                record_failure(peer);
                audit_failed_credential(&db, "client", req.uri().path(), peer).await;
                return (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({"error":"Invalid client credential"})),
                )
                    .into_response();
            }
        };
        // Only explicitly scoped APIs are available to client credentials.
        // Access management performs its more specific role checks inside its router.
        let path = req.uri().path();
        let scope = http_scope(path, req.method() == axum::http::Method::GET);
        if scope.is_none_or(|scope| !principal.allows(scope)) {
            let _ = ax_global_db::access::audit(
                &db,
                Some(&principal.member_id),
                Some(&principal.client_id),
                "authorization.denied",
                "http",
                path,
                &serde_json::json!({}),
            )
            .await;
            return (
                StatusCode::FORBIDDEN,
                axum::Json(serde_json::json!({"error":"Insufficient client scope"})),
            )
                .into_response();
        }
        return next.run(req).await;
    }
    let Some(expected) = configured.filter(|t| !t.is_empty()) else {
        return next.run(req).await;
    };

    let path = req.uri().path();
    if path.starts_with("/assets/")
        || path == "/manifest.webmanifest"
        || path == "/sw.js"
        || path == "/favicon.ico"
        || path == "/api/share/status"
    {
        return next.run(req).await;
    }

    let header_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|s| s.to_string());

    let cookie_token = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|c| {
            c.split(';').find_map(|part| {
                let part = part.trim();
                part.strip_prefix("ax_share=").map(|s| s.to_string())
            })
        });

    let provided = header_token.or(cookie_token);
    let presented = provided.is_some() || share_query_token(req.uri().query()).is_some();
    let secure = share_secure(req.headers());
    if presented && peer_locked(peer) {
        return too_many_failures();
    }
    if let Some(location) = share_redirect_location(path, req.uri().query(), &expected) {
        return share_cookie_redirect(&location, &expected, secure);
    }
    if provided
        .as_deref()
        .is_some_and(|token| token_eq(token, &expected))
    {
        let mut res = next.run(req).await;
        if let Ok(val) = share_cookie(&expected, secure).parse() {
            res.headers_mut().append(header::SET_COOKIE, val);
        }
        return res;
    }

    if presented {
        record_failure(peer);
    }
    ax_usage::log_share(None, "auth fail");
    crate::share_api::unauthorized_gate_html()
}

pub(crate) fn request_has_share_token(
    headers: &axum::http::HeaderMap,
    query: Option<&str>,
    expected: Option<&str>,
) -> bool {
    let Some(expected) = expected.filter(|token| !token.is_empty()) else {
        return false;
    };
    share_query_token(query).is_some_and(|token| token_eq(&token, expected))
        || bearer_share_token(headers).is_some_and(|token| token_eq(token, expected))
        || cookie_share_token(headers).is_some_and(|token| token_eq(token, expected))
}

fn share_redirect_location(path: &str, query: Option<&str>, expected: &str) -> Option<String> {
    if !path.starts_with('/') || path.starts_with("//") || path.contains('\\') {
        return None;
    }
    let query = query?;
    let mut kept = Vec::new();
    let mut matched = false;
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or("");
        if key == "token" {
            if token_eq(&urlencoding_decode(parts.next().unwrap_or("")), expected) {
                matched = true;
            }
            continue;
        }
        kept.push(pair);
    }
    if !matched {
        return None;
    }
    if kept.is_empty() {
        Some(path.to_string())
    } else {
        Some(format!("{path}?{}", kept.join("&")))
    }
}

fn share_secure(headers: &HeaderMap) -> bool {
    headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|proto| proto.eq_ignore_ascii_case("https"))
}

/// Share-gate cookie: `HttpOnly`, `SameSite=Lax`, and `Secure` when the edge is HTTPS.
fn share_cookie(token: &str, secure: bool) -> String {
    format!(
        "ax_share={token}; Path=/; HttpOnly; SameSite=Lax{}",
        if secure { "; Secure" } else { "" }
    )
}

fn share_cookie_redirect(location: &str, expected: &str, secure: bool) -> Response {
    let mut builder = Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::CACHE_CONTROL, "no-store");
    if let Ok(location) = header::HeaderValue::from_str(location) {
        builder = builder.header(header::LOCATION, location);
    }
    let cookie = share_cookie(expected, secure);
    if let Ok(cookie) = header::HeaderValue::from_str(&cookie) {
        builder = builder.header(header::SET_COOKIE, cookie);
    }
    builder
        .body(axum::body::Body::empty())
        .expect("share redirect uses validated headers")
}

fn share_query_token(query: Option<&str>) -> Option<String> {
    query.and_then(|query| {
        query.split('&').find_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            match (parts.next(), parts.next()) {
                (Some("token"), Some(value)) => Some(urlencoding_decode(value)),
                _ => None,
            }
        })
    })
}

fn bearer_share_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|token| !token.starts_with("axc_") && !token.starts_with("axn_"))
}

fn cookie_share_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookie| {
            cookie.split(';').find_map(|part| {
                let part = part.trim();
                part.strip_prefix("ax_share=")
            })
        })
}

fn urlencoding_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let Ok(b) =
                    u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16)
                {
                    out.push(b as char);
                    i += 3;
                } else {
                    out.push(bytes[i] as char);
                    i += 1;
                }
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    out
}

fn http_scope(path: &str, read: bool) -> Option<&'static str> {
    if path == "/api/access/team" || path.starts_with("/api/access/team/") {
        return Some("admin:team");
    }
    if path == "/api/access/nodes" || path.starts_with("/api/access/nodes/") {
        return Some("admin:nodes");
    }
    if path.starts_with("/api/access/") {
        return Some("admin:clients");
    }
    if path == "/api/memory" || path.starts_with("/api/memory/") {
        return Some(if read { "memory:read" } else { "memory:write" });
    }
    if path.starts_with("/api/policy/share/") {
        return Some(if read { "share:read" } else { "share:write" });
    }
    if read
        && [
            "/api/stats",
            "/api/nodes",
            "/api/edges",
            "/api/files",
            "/api/search",
            "/api/graph",
            "/api/insights",
        ]
        .iter()
        .any(|prefix| path == *prefix || path.starts_with(&format!("{prefix}/")))
    {
        return Some("context:read");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clients_only_reach_scoped_api_paths() {
        assert_eq!(http_scope("/api/memory/entries", true), Some("memory:read"));
        assert_eq!(
            http_scope("/api/memory/entries", false),
            Some("memory:write")
        );
        assert_eq!(http_scope("/api/access/team/u1", false), Some("admin:team"));
        assert_eq!(
            http_scope("/api/access/credentials/c1/revoke", false),
            Some("admin:clients")
        );
        assert_eq!(http_scope("/api/agent/terminal", false), None);
        assert_eq!(http_scope("/api/nodes", false), None);
    }

    #[test]
    fn token_comparison_matches_only_the_exact_token() {
        assert!(token_eq("sekret", "sekret"));
        assert!(!token_eq("sekreT", "sekret"));
        assert!(!token_eq("sekre", "sekret"));
        assert!(!token_eq("", "sekret"));
        let mut headers = HeaderMap::new();
        headers.insert(header::COOKIE, "ax_share=sekret".parse().unwrap());
        assert!(request_has_share_token(&headers, None, Some("sekret")));
        assert!(!request_has_share_token(&headers, None, Some("other")));
        assert!(request_has_share_token(
            &HeaderMap::new(),
            Some("token=sekret"),
            Some("sekret")
        ));
    }

    #[test]
    fn wrong_query_token_is_not_a_redirect() {
        assert!(share_redirect_location("/notes", Some("token=nope"), "sekret").is_none());
    }

    #[test]
    fn share_redirect_stays_on_this_host() {
        assert!(
            share_redirect_location("//evil.example", Some("token=sekret"), "sekret").is_none()
        );
    }

    #[test]
    fn share_cookie_flags_follow_transport() {
        let plain = share_cookie("sekret", false);
        assert!(plain.contains("HttpOnly"));
        assert!(!plain.contains("Secure"));
        let tls = share_cookie("sekret", true);
        assert!(tls.contains("HttpOnly"));
        assert!(tls.contains("Secure"));
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        assert!(share_secure(&headers));
    }

    #[tokio::test]
    async fn share_link_sets_a_cookie_and_drops_the_token_from_the_url() {
        use axum::body::Body;
        use axum::http::Request;
        use tower::ServiceExt;

        let app = axum::Router::new()
            .route("/notes", axum::routing::get(|| async { "page" }))
            .layer(axum::middleware::from_fn(fixed_share));
        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/notes?token=sekret&stay=1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::SEE_OTHER);
        let location = first
            .headers()
            .get(header::LOCATION)
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(location, "/notes?stay=1");
        assert!(!location.contains("sekret"));
        let cookie = first
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(cookie.contains("ax_share=sekret"));
        assert!(cookie.contains("HttpOnly"));

        let second = app
            .oneshot(
                Request::builder()
                    .uri("/notes?stay=1")
                    .header(header::COOKIE, "ax_share=sekret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::OK);
    }

    fn share_request(uri: &str, peer: &str, cookie: Option<&str>, proxied: bool) -> Request {
        let mut builder = axum::http::Request::builder().uri(uri);
        if let Some(cookie) = cookie {
            builder = builder.header(header::COOKIE, format!("ax_share={cookie}"));
        }
        if proxied {
            builder = builder.header("cf-connecting-ip", "198.51.100.9");
        }
        let mut req = builder.body(axum::body::Body::empty()).unwrap();
        let peer: SocketAddr = peer.parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(peer));
        req
    }

    fn share_app() -> axum::Router {
        axum::Router::new()
            .route("/notes", axum::routing::get(|| async { "page" }))
            .layer(axum::middleware::from_fn(fixed_share))
    }

    async fn share_status(app: &axum::Router, req: Request) -> StatusCode {
        use tower::ServiceExt;
        app.clone().oneshot(req).await.unwrap().status()
    }

    /// Once a peer has used up its failures, every credential it presents
    /// gets 429 without being checked, the correct one included, so the
    /// answer no longer tells a guesser which guess was right.
    #[tokio::test]
    async fn a_locked_peer_cannot_tell_a_right_guess_from_a_wrong_one() {
        let app = share_app();
        let peer = "198.51.100.21:40000";
        for _ in 0..MAX_FAILURES {
            let status =
                share_status(&app, share_request("/notes", peer, Some("stale"), false)).await;
            assert_eq!(status, StatusCode::UNAUTHORIZED);
        }
        assert_eq!(
            share_status(&app, share_request("/notes", peer, Some("stale"), false)).await,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            share_status(&app, share_request("/notes", peer, Some("sekret"), false)).await,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            share_status(&app, share_request("/notes?token=sekret", peer, None, false)).await,
            StatusCode::TOO_MANY_REQUESTS
        );
        let mut bearer = share_request("/notes", peer, None, false);
        bearer
            .headers_mut()
            .insert(header::AUTHORIZATION, "Bearer sekret".parse().unwrap());
        assert_eq!(share_status(&app, bearer).await, StatusCode::TOO_MANY_REQUESTS);
        let mut client = share_request("/notes", peer, None, false);
        client
            .headers_mut()
            .insert(header::AUTHORIZATION, "Bearer axc_guess".parse().unwrap());
        assert_eq!(share_status(&app, client).await, StatusCode::TOO_MANY_REQUESTS);

        // Another peer is not affected.
        let other = "198.51.100.22:40000";
        assert_eq!(
            share_status(&app, share_request("/notes", other, Some("sekret"), false)).await,
            StatusCode::OK
        );
    }

    /// Behind a tunnel every viewer arrives from 127.0.0.1 with a proxy
    /// header; one viewer's failures must not lock out the others.
    #[tokio::test]
    async fn failures_through_a_local_proxy_lock_nobody() {
        let app = share_app();
        let peer = "127.0.0.1:40001";
        for _ in 0..MAX_FAILURES * 2 {
            assert_eq!(
                share_status(&app, share_request("/notes", peer, Some("wrong"), true)).await,
                StatusCode::UNAUTHORIZED
            );
        }
        let mapped = "[::ffff:127.0.0.1]:40002";
        assert_eq!(
            share_status(&app, share_request("/notes", mapped, Some("wrong"), true)).await,
            StatusCode::UNAUTHORIZED
        );
    }

    async fn fixed_share(req: axum::extract::Request, next: Next) -> Response {
        enforce_share_token(req, next, Some("sekret".into())).await
    }
}
