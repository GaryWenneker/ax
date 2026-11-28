//! Which browser origins may call Command Center.

use axum::http::{header::HeaderValue, Method};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

/// CORS for the Command Center. Same-origin pages do not need this layer.
/// Other websites are not granted access.
pub(crate) fn command_center_cors(bind: &str, port: u16) -> CorsLayer {
    let origins: Vec<HeaderValue> = allowed_origin_values(bind, port)
        .into_iter()
        .filter_map(|origin| HeaderValue::from_str(&origin).ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::PATCH,
            Method::DELETE,
            Method::HEAD,
            Method::OPTIONS,
        ])
        .allow_headers(Any)
}

/// The terminal socket accepts only the local owner's browser: a loopback
/// socket peer with no proxy in between (see
/// [`crate::local_owner::local_peer`]) whose page is on a loopback origin with
/// this port. A LAN bind does not widen this.
pub(crate) fn terminal_request_allowed(headers: &axum::http::HeaderMap, port: u16) -> bool {
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    crate::local_owner::local_peer(headers) && browser_origin_allowed(origin, port)
}

pub(crate) fn browser_origin_allowed(origin: Option<&str>, port: u16) -> bool {
    let Some(origin) = origin else {
        return false;
    };
    let Ok(url) = reqwest::Url::parse(origin) else {
        return false;
    };
    if url.scheme() != "http" && url.scheme() != "https" {
        return false;
    }
    if url.port_or_known_default() != Some(port) {
        return false;
    }
    url.host_str().is_some_and(is_loopback_host)
}

fn allowed_origin_values(bind: &str, port: u16) -> Vec<String> {
    let mut origins = vec![
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
        format!("http://[::1]:{port}"),
    ];
    if let Some(extra) = explicit_bind_origin(bind, port) {
        origins.push(extra);
    }
    origins
}

fn explicit_bind_origin(bind: &str, port: u16) -> Option<String> {
    let ip: std::net::IpAddr = bind.parse().ok()?;
    if ip.is_loopback() || ip.is_unspecified() {
        return None;
    }
    match ip {
        std::net::IpAddr::V4(addr) => Some(format!("http://{addr}:{port}")),
        std::net::IpAddr::V6(addr) => Some(format!("http://[{addr}]:{port}")),
    }
}

fn is_loopback_host(host: &str) -> bool {
    let host = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    host == "localhost"
        || host == "127.0.0.1"
        || host == "::1"
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[test]
    fn loopback_origin_matches_the_command_center_port() {
        assert!(browser_origin_allowed(Some("http://127.0.0.1:7070"), 7070));
        assert!(browser_origin_allowed(Some("http://localhost:7070"), 7070));
        assert!(browser_origin_allowed(Some("http://[::1]:7070"), 7070));
        assert!(!browser_origin_allowed(Some("http://evil.example"), 7070));
        assert!(!browser_origin_allowed(
            Some("http://evil.example:7070"),
            7070
        ));
        assert!(!browser_origin_allowed(None, 7070));
        assert!(!browser_origin_allowed(Some("http://127.0.0.1:8080"), 7070));
        // A LAN address is someone else's browser, whatever the bind.
        assert!(!browser_origin_allowed(
            Some("http://192.168.1.10:7070"),
            7070
        ));
    }

    #[test]
    fn terminal_needs_the_local_peer_mark_and_a_loopback_origin() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert("origin", "http://127.0.0.1:7070".parse().unwrap());
        assert!(!terminal_request_allowed(&headers, 7070));
        headers.insert(crate::request_guard::LOCAL_PEER, "1".parse().unwrap());
        assert!(terminal_request_allowed(&headers, 7070));
        headers.insert("origin", "http://10.0.0.9:7070".parse().unwrap());
        assert!(!terminal_request_allowed(&headers, 7070));
    }

    #[tokio::test]
    async fn foreign_origin_cannot_read_the_api() {
        let response = probe("http://evil.example").await;
        assert!(response
            .headers()
            .get("access-control-allow-origin")
            .is_none());
    }

    #[tokio::test]
    async fn command_center_origin_can_read_the_api() {
        let origin = "http://127.0.0.1:7070";
        let response = probe(origin).await;
        assert_eq!(
            response
                .headers()
                .get("access-control-allow-origin")
                .and_then(|v| v.to_str().ok()),
            Some(origin)
        );
    }

    async fn probe(origin: &str) -> axum::response::Response {
        let app = Router::new()
            .route("/api/probe", get(|| async { "ok" }))
            .layer(command_center_cors("127.0.0.1", 7070));
        app.oneshot(
            Request::builder()
                .uri("/api/probe")
                .header("origin", origin)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
    }
}
