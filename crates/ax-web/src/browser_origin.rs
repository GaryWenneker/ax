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

pub(crate) fn browser_origin_allowed(origin: Option<&str>, port: u16, local_only: bool) -> bool {
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
    let Some(host) = url.host_str() else {
        return false;
    };
    if is_loopback_host(host) {
        return true;
    }
    if local_only {
        return false;
    }
    host.parse::<std::net::IpAddr>().is_ok()
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
        assert!(browser_origin_allowed(
            Some("http://127.0.0.1:7070"),
            7070,
            true
        ));
        assert!(browser_origin_allowed(
            Some("http://localhost:7070"),
            7070,
            true
        ));
        assert!(!browser_origin_allowed(
            Some("http://evil.example"),
            7070,
            true
        ));
        assert!(!browser_origin_allowed(
            Some("http://evil.example:7070"),
            7070,
            true
        ));
        assert!(!browser_origin_allowed(None, 7070, true));
        assert!(!browser_origin_allowed(
            Some("http://192.168.1.10:7070"),
            7070,
            true
        ));
        assert!(browser_origin_allowed(
            Some("http://192.168.1.10:7070"),
            7070,
            false
        ));
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
