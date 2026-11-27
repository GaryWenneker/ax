//! Outermost checks for every Command Center request.
//!
//! The `Host` header must name this server. A page on another domain whose
//! DNS name points at this computer (DNS rebinding) is same-origin with the
//! server, so CORS does not stop it; refusing unexpected `Host` names does.
//!
//! Whether a request comes from this computer is decided here from the socket
//! peer address alone. Request headers can only take that trust away (a proxy
//! header means someone else is being forwarded), never grant it.

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

/// Host names this server answers for.
#[derive(Clone, Debug, Default)]
pub(crate) struct AllowedHosts {
    /// Bound beyond loopback: IP addresses are accepted too. A rebinding
    /// attack needs a DNS name, so an IP address in `Host` cannot be one.
    lan: bool,
    /// Lowercase host names without a port.
    names: Vec<String>,
}

impl AllowedHosts {
    /// Loopback names, plus IP addresses when bound beyond loopback, plus the
    /// host of `AX_WEB_AUTH_ORIGIN`, `AX_WEB_ALLOWED_HOSTS` (comma separated)
    /// and `web.allowed_hosts` in `~/.ax/config.json`.
    pub(crate) fn from_environment(local_only: bool) -> Self {
        let mut names = Vec::new();
        if let Ok(origin) = std::env::var("AX_WEB_AUTH_ORIGIN") {
            if let Some(host) = reqwest::Url::parse(&origin)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
            {
                names.push(host);
            }
        }
        if let Ok(list) = std::env::var("AX_WEB_ALLOWED_HOSTS") {
            names.extend(list.split(',').map(str::to_string));
        }
        names.extend(configured_hosts());
        let names = names
            .iter()
            .filter_map(|name| host_name(name.trim()))
            .collect();
        Self {
            lan: !local_only,
            names,
        }
    }

    pub(crate) fn allows(&self, host: &str) -> bool {
        let Some(name) = host_name(host) else {
            return false;
        };
        if name == "localhost" {
            return true;
        }
        if let Ok(ip) = name.parse::<IpAddr>() {
            return ip.is_loopback() || self.lan;
        }
        self.names.contains(&name)
    }
}

/// `web.allowed_hosts` from `~/.ax/config.json`.
fn configured_hosts() -> Vec<String> {
    let Some(path) = ax_utils::paths::home_dir().map(|h| h.join(".ax").join("config.json")) else {
        return Vec::new();
    };
    let Ok(bytes) = ax_utils::read_bounded(&path, 1 << 20) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Vec::new();
    };
    value["web"]["allowed_hosts"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The host part of a `Host` value or a configured entry: lowercase, without
/// port, brackets or a trailing dot. `None` when it is not a valid authority.
fn host_name(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.contains(['/', '@', ' ', '\\']) {
        return None;
    }
    let name = if let Some(rest) = value.strip_prefix('[') {
        let (ip, tail) = rest.split_once(']')?;
        if !(tail.is_empty() || tail.strip_prefix(':').is_some_and(valid_port)) {
            return None;
        }
        ip.parse::<std::net::Ipv6Addr>().ok()?;
        ip
    } else {
        match value.rsplit_once(':') {
            Some((name, port)) if valid_port(port) && !name.contains(':') => name,
            Some(_) if value.parse::<std::net::Ipv6Addr>().is_ok() => value,
            Some(_) => return None,
            None => value,
        }
    };
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    (!name.is_empty()).then_some(name)
}

fn valid_port(port: &str) -> bool {
    !port.is_empty() && port.parse::<u16>().is_ok()
}

/// Set by [`guard`] on requests whose socket peer is this computer and that
/// carry no proxy header. Any copy a client sends is removed first.
pub(crate) const LOCAL_PEER: &str = "x-ax-local-peer";

/// Headers a reverse proxy or tunnel adds for the client it forwards.
const FORWARDING: [&str; 7] = [
    "forwarded",
    "x-forwarded-for",
    "x-forwarded-host",
    "x-forwarded-proto",
    "x-real-ip",
    "cf-connecting-ip",
    "true-client-ip",
];

/// True when the TCP peer is a loopback address and no proxy forwarded the
/// request. A request with no known peer is not local.
pub(crate) fn peer_is_local(peer: Option<SocketAddr>, headers: &HeaderMap) -> bool {
    peer.is_some_and(|addr| addr.ip().is_loopback())
        && !FORWARDING.iter().any(|name| headers.contains_key(*name))
}

/// Refuses requests whose `Host` (or HTTP/2 authority) is not allowed, and
/// marks requests from this computer with [`LOCAL_PEER`]. A request without
/// `Host` or authority is not a browser request, so it cannot be DNS
/// rebinding and is passed on.
pub(crate) async fn guard(
    State(allowed): State<AllowedHosts>,
    mut req: Request,
    next: Next,
) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .map(|v| v.to_str().unwrap_or("\u{0}").to_string())
        .or_else(|| req.uri().authority().map(|a| a.as_str().to_string()));
    if let Some(host) = host {
        if !allowed.allows(&host) {
            return (
                StatusCode::FORBIDDEN,
                "Unknown host. Add it to web.allowed_hosts in ~/.ax/config.json to allow it.",
            )
                .into_response();
        }
    }
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|info| info.0);
    let local = peer_is_local(peer, req.headers());
    let headers = req.headers_mut();
    headers.remove(LOCAL_PEER);
    if local {
        headers.insert(LOCAL_PEER, HeaderValue::from_static("1"));
    }
    next.run(req).await
}

/// Whether Command Center may serve on a non-loopback `bind`. Anyone on the
/// network could otherwise read the project and run the agent terminal, so
/// it needs web sign-in or a share token, unless the owner explicitly allows
/// anonymous access. `Ok(Some(_))` is a warning to print.
pub(crate) fn lan_exposure(
    bind: &str,
    share_token: bool,
    login_enabled: bool,
    allow_anonymous: bool,
) -> Result<Option<String>, String> {
    if share_token || login_enabled {
        return Ok(None);
    }
    if allow_anonymous {
        return Ok(Some(format!(
            "serving on {bind} without sign-in or a share token: anyone who can reach this \
             address can read the project and change it (--allow-anonymous-lan)"
        )));
    }
    Err(format!(
        "refusing to serve on {bind} without sign-in: anyone who can reach this address could \
         read the project and run the agent. Turn on sign-in in Settings → Security (open \
         http://127.0.0.1 first), use `ax share` for a read-only token link, or pass \
         --allow-anonymous-lan to accept the risk"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_names_are_normalized() {
        assert_eq!(host_name("LocalHost:7070").as_deref(), Some("localhost"));
        assert_eq!(host_name("[::1]:7070").as_deref(), Some("::1"));
        assert_eq!(host_name("::1").as_deref(), Some("::1"));
        assert_eq!(
            host_name("admin.example.com.").as_deref(),
            Some("admin.example.com")
        );
        assert_eq!(host_name("a.example:notaport"), None);
        assert_eq!(host_name("user@evil.example"), None);
        assert_eq!(host_name(""), None);
    }

    #[test]
    fn loopback_bind_accepts_only_loopback_and_configured_names() {
        let allowed = AllowedHosts {
            lan: false,
            names: vec!["admin.example.com".into()],
        };
        for host in [
            "127.0.0.1:7070",
            "localhost",
            "[::1]:7070",
            "127.0.0.2",
            "admin.example.com",
            "ADMIN.example.com:443",
        ] {
            assert!(allowed.allows(host), "{host}");
        }
        for host in [
            "evil.example:7070",
            "127.0.0.1.evil.example",
            "localhost.evil.example",
            "192.168.1.5:7070",
            "admin.example.com.evil.example",
        ] {
            assert!(!allowed.allows(host), "{host}");
        }
    }

    #[test]
    fn only_an_unproxied_loopback_peer_is_local() {
        let loopback: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let v6: SocketAddr = "[::1]:5000".parse().unwrap();
        let remote: SocketAddr = "203.0.113.7:5000".parse().unwrap();
        let empty = HeaderMap::new();
        assert!(peer_is_local(Some(loopback), &empty));
        assert!(peer_is_local(Some(v6), &empty));
        assert!(!peer_is_local(Some(remote), &empty));
        assert!(!peer_is_local(None, &empty));
        for name in FORWARDING {
            let mut proxied = HeaderMap::new();
            proxied.insert(name, HeaderValue::from_static("127.0.0.1"));
            assert!(!peer_is_local(Some(loopback), &proxied), "{name}");
        }
    }

    #[test]
    fn a_lan_bind_needs_sign_in_a_share_token_or_the_explicit_flag() {
        let refused = lan_exposure("0.0.0.0", false, false, false).unwrap_err();
        assert!(refused.contains("--allow-anonymous-lan"), "{refused}");
        assert_eq!(lan_exposure("0.0.0.0", true, false, false), Ok(None));
        assert_eq!(lan_exposure("0.0.0.0", false, true, false), Ok(None));
        let warning = lan_exposure("0.0.0.0", false, false, true)
            .unwrap()
            .unwrap();
        assert!(warning.contains("without sign-in"), "{warning}");
    }

    #[test]
    fn lan_bind_also_accepts_ip_addresses() {
        let allowed = AllowedHosts {
            lan: true,
            names: Vec::new(),
        };
        assert!(allowed.allows("192.168.1.5:7070"));
        assert!(allowed.allows("[fe80::1]:7070"));
        assert!(!allowed.allows("evil.example:7070"));
    }
}
