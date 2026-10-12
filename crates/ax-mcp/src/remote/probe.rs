//! Unauthenticated, bounded publication check. Never follows a redirect or invokes tools.
use ax_global_db::remote_mcp_publications::Publication;
use reqwest::{Client, Response, StatusCode};
use serde_json::{json, Value};
use std::{
    net::{IpAddr, SocketAddr},
    time::Duration,
};
use url::Url;

fn public_address(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let b = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_unspecified()
                && !ip.is_broadcast()
                && !ip.is_multicast()
                && !ip.is_documentation()
                && b[0] != 0
                && b[0] < 224
                && !(b[0] == 100 && (64..=127).contains(&b[1]))
                && !(b[0] == 198 && (b[1] == 18 || b[1] == 19))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            ip.to_ipv4_mapped()
                .map(public_address_v4)
                .unwrap_or_else(|| {
                    !ip.is_loopback()
                        && !ip.is_unspecified()
                        && !ip.is_multicast()
                        && (s[0] & 0xe000) == 0x2000
                        && !(s[0] == 0x2001 && (s[1] == 0xdb8 || s[1] < 0x200))
                })
        }
    }
}
fn public_address_v4(ip: std::net::Ipv4Addr) -> bool {
    public_address(IpAddr::V4(ip))
}
async fn bounded_json(mut response: Response) -> Result<Value, String> {
    const LIMIT: usize = 16 * 1024;
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        return Err("Endpoint returned too much data".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Cannot read endpoint response")?
    {
        if bytes.len() + chunk.len() > LIMIT {
            return Err("Endpoint returned too much data".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| "Invalid protected-resource metadata".into())
}
async fn probe_with(
    client: &Client,
    endpoint: &str,
    metadata_url: &str,
    resource: &str,
    issuer: &str,
) -> Result<&'static str, String> {
    let response = client.post(endpoint).header("Accept", "application/json, text/event-stream")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"ax-publication-check","version":"1"}}}))
        .send().await.map_err(|_| "Endpoint could not be reached")?;
    let expected = format!("resource_metadata=\"{metadata_url}\"");
    if response.status() != StatusCode::UNAUTHORIZED
        || !response
            .headers()
            .get_all("www-authenticate")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .any(|v| {
                v.starts_with("Bearer ")
                    && v.split(',').any(|part| {
                        part.trim() == expected
                            || part.trim().strip_prefix("Bearer ") == Some(expected.as_str())
                    })
            })
    {
        return Err(
            "Endpoint did not return the expected OAuth challenge. No project access was verified."
                .into(),
        );
    }
    // Do not buffer the challenge body. The separate metadata response is bounded.
    drop(response);
    let response = client
        .get(metadata_url)
        .send()
        .await
        .map_err(|_| "Protected-resource metadata is unavailable")?;
    if response.status() != StatusCode::OK {
        return Err("Protected-resource metadata is unavailable".into());
    }
    let metadata = bounded_json(response).await?;
    if metadata["resource"].as_str() != Some(resource)
        || !metadata["authorization_servers"]
            .as_array()
            .is_some_and(|values| values.iter().any(|v| v.as_str() == Some(issuer)))
    {
        return Err("Endpoint metadata differs from the registered publisher".into());
    }
    Ok("Endpoint reachable, sign-in required. Project access has not been verified.")
}
pub async fn check(p: &Publication) -> Result<&'static str, String> {
    tokio::time::timeout(Duration::from_secs(5), async {
        let url = super::config::https_url(&p.endpoint_url)?;
        if url.origin() != super::config::https_url(&p.public_origin)?.origin()
            || url.path() != format!("/projects/{}/mcp", p.remote_project_id) { return Err("Invalid registered endpoint".into()); }
        let host = url.host_str().ok_or("Invalid endpoint host")?;
        let addresses: Vec<SocketAddr> = match url.host() {
            Some(url::Host::Ipv4(ip)) => vec![SocketAddr::new(IpAddr::V4(ip), url.port_or_known_default().unwrap_or(443))],
            Some(url::Host::Ipv6(ip)) => vec![SocketAddr::new(IpAddr::V6(ip), url.port_or_known_default().unwrap_or(443))],
            _ => tokio::net::lookup_host((host, url.port_or_known_default().unwrap_or(443))).await.map_err(|_| "Endpoint DNS could not be resolved")?.collect(),
        };
        if addresses.is_empty() || addresses.iter().any(|a| !public_address(a.ip())) {
            return Err("Endpoint checks require a public address. Private-network endpoints can still be copied.".into());
        }
        let client = Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(5)).resolve_to_addrs(host, &addresses).build().map_err(|_| "Cannot prepare endpoint check")?;
        let metadata_url = Url::parse(&p.public_origin).map_err(|_| "Invalid origin")?.join("/.well-known/oauth-protected-resource").map_err(|_| "Invalid metadata URL")?;
        probe_with(&client, &p.endpoint_url, metadata_url.as_str(), &p.resource, &p.oauth_issuer).await
    }).await.map_err(|_| "Endpoint check timed out")?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unsafe_probe_addresses_are_rejected() {
        for value in [
            "127.0.0.1",
            "10.1.2.3",
            "169.254.169.254",
            "0.0.0.0",
            "100.64.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "240.0.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
        ] {
            assert!(!public_address(value.parse().unwrap()), "{value}");
        }
        assert!(public_address("8.8.8.8".parse().unwrap()));
        assert!(public_address("2606:4700:4700::1111".parse().unwrap()));
    }
    #[tokio::test]
    async fn endpoint_check_is_unauthenticated_bounded_and_does_not_follow_redirects() {
        use axum::{
            extract::State,
            response::IntoResponse,
            routing::{get, post},
            Json, Router,
        };
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let metadata = format!("{origin}/metadata");
        let mode = Arc::new(AtomicUsize::new(0));
        let followed = Arc::new(AtomicUsize::new(0));
        let router = Router::new().route("/mcp", post({
            let metadata = metadata.clone();
            move |State((mode, _)): State<(Arc<AtomicUsize>, Arc<AtomicUsize>)>, headers: axum::http::HeaderMap, Json(body): Json<Value>| {
                let metadata = metadata.clone();
                async move {
                    assert!(!headers.contains_key("authorization")); assert_eq!(body["method"], "initialize");
                    match mode.load(Ordering::SeqCst) {
                        1 => (StatusCode::TEMPORARY_REDIRECT, [("location", "/steal")], "").into_response(),
                        2 => Json(json!({"ready":true})).into_response(),
                        5 => { tokio::time::sleep(Duration::from_millis(200)).await; StatusCode::UNAUTHORIZED.into_response() },
                        _ => (StatusCode::UNAUTHORIZED, [("www-authenticate", format!("Bearer resource_metadata=\"{metadata}\", scope=\"ax:context\""))], "").into_response(),
                    }
                }
            }
        })).route("/metadata", get(|State((mode, _)): State<(Arc<AtomicUsize>, Arc<AtomicUsize>)>| async move {
            if mode.load(Ordering::SeqCst) == 3 { return "x".repeat(17 * 1024).into_response(); }
            let resource = if mode.load(Ordering::SeqCst) == 4 { "https://wrong.example" } else { "https://ax.example" };
            Json(json!({"resource":resource,"authorization_servers":["https://issuer.example/"]})).into_response()
        })).route("/steal", get(|State((_, followed)): State<(Arc<AtomicUsize>, Arc<AtomicUsize>)>| async move { followed.fetch_add(1, Ordering::SeqCst); "not allowed" }))
        .with_state((mode.clone(), followed.clone()));
        let task = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        for case in 0..=5 {
            mode.store(case, Ordering::SeqCst);
            let result = probe_with(
                &client,
                &format!("{origin}/mcp"),
                &metadata,
                "https://ax.example",
                "https://issuer.example/",
            )
            .await;
            assert_eq!(result.is_ok(), case == 0, "case {case}: {result:?}");
        }
        assert_eq!(followed.load(Ordering::SeqCst), 0);
        task.abort();
    }
}
