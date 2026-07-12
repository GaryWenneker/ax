//! Reverse proxy to local SonarQube — injects admin auth and dark theme.

use ax_remote::ShipConfig;
use axum::{
    body::{Body, Bytes},
    extract::{Request, State},
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use base64::{engine::general_purpose::STANDARD, Engine as _};

use crate::workspace_state::WebHub;

/// Browser-facing path prefix (full path including `/api/ship`).
pub const SONAR_UI_PUBLIC_PREFIX: &str = "/api/ship/sonar/ui";

/// Route prefix on the ship router (nested under `/api/ship`).
const SONAR_UI_ROUTE_PREFIX: &str = "/sonar/ui";

const DARK_THEME_INJECT: &str = r#"<base href="/api/ship/sonar/ui/">
<script id="ax-sonar-theme">(function(){try{
  var keys=["appearance.theme","sonar.ui.theme","theme","user.theme"];
  keys.forEach(function(k){localStorage.setItem(k,"dark");sessionStorage.setItem(k,"dark");});
  document.documentElement.dataset.theme="dark";
  document.documentElement.classList.add("dark");
}catch(e){}})();</script>
<style id="ax-sonar-dark-pref">html{color-scheme:dark;}</style>"#;

pub async fn handle_sonar_ui_info(State(hub): State<WebHub>) -> impl IntoResponse {
    let config = sonar_config(&hub).await;
    let host = effective_sonar_host(&config);
    let reachable = ax_quality::sonar_reachable(&host).await;
    Json(serde_json::json!({
        "ok": true,
        "reachable": reachable,
        "proxy_url": format!("{SONAR_UI_PUBLIC_PREFIX}/"),
        "host": host,
        "dark_mode": "auto",
    }))
}

pub async fn handle_sonar_ui_proxy(
    State(hub): State<WebHub>,
    req: Request<Body>,
) -> impl IntoResponse {
    let config = sonar_config(&hub).await;
    let sonar_host = effective_sonar_host(&config);
    let sonar = &config.sonar;

    if !ax_quality::sonar_reachable(&sonar_host).await {
        return (
            StatusCode::BAD_GATEWAY,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "SonarQube is not reachable — install and start it from the Setup tab.",
        )
            .into_response();
    }

    let method = req.method().clone();
    let uri = req.uri().clone();
    let upstream_path = upstream_path_from_uri(uri.path());
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    let upstream_url = format!(
        "{}{}{}",
        sonar_host.trim_end_matches('/'),
        upstream_path,
        query
    );

    let (parts, body) = req.into_parts();
    let body_bytes = match axum::body::to_bytes(body, 32 * 1024 * 1024).await {
        Ok(b) => b,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                "Request body too large",
            )
                .into_response()
        }
    };

    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    let mut upstream = client.request(method.clone(), &upstream_url);
    upstream = upstream.header(
        "Authorization",
        basic_auth(&sonar.admin_user, &sonar.admin_password),
    );

    for (name, value) in &parts.headers {
        let n = name.as_str();
        if should_skip_request_header(n) {
            continue;
        }
        upstream = upstream.header(name, value);
    }

    if !body_bytes.is_empty() || method == Method::POST || method == Method::PUT || method == Method::PATCH {
        upstream = upstream.body(body_bytes.to_vec());
    }

    let resp = match upstream.send().await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("SonarQube proxy error: {e}"),
            )
                .into_response()
        }
    };

    let status = StatusCode::from_u16(resp.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let resp_headers = resp.headers().clone();
    let resp_bytes = match resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                format!("SonarQube proxy read error: {e}"),
            )
                .into_response()
        }
    };

    let content_type = resp_headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    let inject_theme = content_type.contains("text/html");
    let needs_rewrite = inject_theme
        || content_type.contains("javascript")
        || content_type.contains("json")
        || content_type.contains("text/css")
        || content_type.contains("text/plain")
        || content_type.contains("application/xml")
        || content_type.contains("text/xml");

    let body_out: Bytes = if needs_rewrite {
        let text = String::from_utf8_lossy(&resp_bytes);
        Bytes::from(rewrite_proxy_text(&text, inject_theme))
    } else {
        resp_bytes
    };

    let mut out_headers = HeaderMap::new();
    for (name, value) in resp_headers.iter() {
        let n = name.as_str();
        if should_skip_response_header(n) {
            continue;
        }
        if n.eq_ignore_ascii_case("location") {
            if let Ok(v) = value.to_str() {
                if let Some(rewritten) = rewrite_location(v, &sonar_host) {
                    out_headers.insert(header::LOCATION, HeaderValue::from_str(&rewritten).unwrap_or_else(|_| value.clone()));
                    continue;
                }
            }
        }
        out_headers.insert(name.clone(), value.clone());
    }

    if !out_headers.contains_key(header::CONTENT_TYPE) {
        out_headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/html; charset=utf-8"),
        );
    }

    let mut response = Response::new(Body::from(body_out));
    *response.status_mut() = status;
    *response.headers_mut() = out_headers;
    response
}

async fn sonar_config(hub: &WebHub) -> ShipConfig {
    let daemon = {
        let ws = hub.read().await;
        ws.ship.daemon.clone()
    };
    daemon.config().await
}

fn effective_sonar_host(config: &ShipConfig) -> String {
    let name = config
        .sonar
        .podman_container
        .as_deref()
        .unwrap_or("sonarqube");
    let pref = if config.sonar.container_runtime == "auto" {
        None
    } else {
        Some(config.sonar.container_runtime.as_str())
    };
    let discovery = ax_quality::discover_sonar(&config.sonar.host, name, pref);
    if discovery.reachable {
        discovery.host
    } else {
        config.sonar.host.clone()
    }
}

fn upstream_path_from_uri(path: &str) -> String {
    let stripped = path
        .strip_prefix(SONAR_UI_ROUTE_PREFIX)
        .unwrap_or(path);
    if stripped.is_empty() || stripped == "/" {
        "/".into()
    } else {
        format!("/{}", stripped.trim_start_matches('/'))
    }
}

fn basic_auth(user: &str, password: &str) -> String {
    format!(
        "Basic {}",
        STANDARD.encode(format!("{user}:{password}"))
    )
}

fn should_skip_request_header(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "host"
            | "connection"
            | "authorization"
            | "content-length"
            | "transfer-encoding"
            | "accept-encoding"
    )
}

fn should_skip_response_header(name: &str) -> bool {
    matches!(
        name.to_lowercase().as_str(),
        "connection"
            | "transfer-encoding"
            | "content-length"
            | "content-encoding"
            | "content-security-policy"
            | "content-security-policy-report-only"
            | "x-frame-options"
            | "cross-origin-opener-policy"
            | "cross-origin-embedder-policy"
            | "cross-origin-resource-policy"
    )
}

fn rewrite_proxy_text(text: &str, inject_theme: bool) -> Vec<u8> {
    let mut out = if inject_theme {
        rewrite_html_root_urls(text, SONAR_UI_PUBLIC_PREFIX)
    } else {
        rewrite_quoted_root_paths(text, SONAR_UI_PUBLIC_PREFIX)
    };
    out = rewrite_quoted_root_paths(&out, SONAR_UI_PUBLIC_PREFIX);
    out = rewrite_sonar_asset_helpers(&out, SONAR_UI_PUBLIC_PREFIX);
    if inject_theme {
        out = inject_dark_theme_html(&out);
    }
    out.into_bytes()
}

/// Rewrite root-relative URLs in HTML attributes (`src="/…"`, `href="/…"`, etc.).
fn rewrite_html_root_urls(html: &str, prefix: &str) -> String {
    let mut out = html.to_string();
    for attr in ["src", "href", "action", "poster", "content", "data-src"] {
        for quote in ['"', '\''] {
            let needle = format!("{attr}={quote}/");
            let replacement = format!("{attr}={quote}{prefix}/");
            if !out.contains(&replacement) {
                out = out.replace(&needle, &replacement);
            }
        }
    }
    dedupe_proxy_prefix(&out, prefix)
}

fn dedupe_proxy_prefix(text: &str, prefix: &str) -> String {
    let double = format!("{prefix}{prefix}");
    let mut out = text.to_string();
    while out.contains(&double) {
        out = out.replace(&double, prefix);
    }
    out
}

/// SonarQube SPA builds runtime asset URLs via `__assetsPath` — rewrite so chunks load through the proxy.
fn rewrite_sonar_asset_helpers(text: &str, prefix: &str) -> String {
    let prefix_slash = format!("{prefix}/");
    text.replace("return '/' + filename", &format!("return '{prefix_slash}' + filename"))
        .replace("return \"/\" + filename", &format!("return \"{prefix_slash}\" + filename"))
        .replace(
            "return '/' + e",
            &format!("return '{prefix_slash}' + e"),
        )
        .replace(
            "return \"/\" + e",
            &format!("return \"{prefix_slash}\" + e"),
        )
}

/// Prefix root-relative URLs (`"/…`, `'/…`, `url(/…`) so SonarQube assets and API calls stay on the proxy path.
fn rewrite_quoted_root_paths(text: &str, prefix: &str) -> String {
    let prefix_chars: Vec<char> = prefix.chars().collect();
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(text.len() + n / 8);

    let mut i = 0;
    while i < n {
        // ("/path or ('/path — common in fetch("/api/…")
        if i + 2 < n
            && chars[i] == '('
            && (chars[i + 1] == '"' || chars[i + 1] == '\'')
            && i + 2 < n
            && chars[i + 2] == '/'
            && !(i + 3 < n && chars[i + 3] == '/')
        {
            let quote = chars[i + 1];
            let path_start = i + 2;
            let already = path_start + prefix_chars.len() <= n
                && chars[path_start..path_start + prefix_chars.len()]
                    .iter()
                    .eq(prefix_chars.iter());
            out.push('(');
            out.push(quote);
            if !already {
                out.push_str(prefix);
            }
            i += 2;
            continue;
        }

        if (chars[i] == '"' || chars[i] == '\'')
            && i + 1 < n
            && chars[i + 1] == '/'
            && !(i + 2 < n && chars[i + 2] == '/')
        {
            let already = i + 1 + prefix_chars.len() <= n
                && chars[i + 1..i + 1 + prefix_chars.len()]
                    .iter()
                    .eq(prefix_chars.iter());
            out.push(chars[i]);
            if !already {
                out.push_str(prefix);
            }
            i += 1;
            continue;
        }

        if i + 4 <= n && chars[i..i + 4].iter().collect::<String>() == "url(" {
            out.push_str("url(");
            i += 4;
            while i < n && chars[i].is_whitespace() {
                out.push(chars[i]);
                i += 1;
            }
            if i < n && chars[i] == '/' && !(i + 1 < n && chars[i + 1] == '/') {
                let already = i + prefix_chars.len() <= n
                    && chars[i..i + prefix_chars.len()]
                        .iter()
                        .eq(prefix_chars.iter());
                if !already {
                    out.push_str(prefix);
                }
            }
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }

    dedupe_proxy_prefix(&out, prefix)
}

fn inject_dark_theme_html(html: &str) -> String {
    let lower = html.to_lowercase();
    let mut out = html.to_string();

    if let Some(pos) = lower.find("</head>") {
        out.insert_str(pos, DARK_THEME_INJECT);
    } else if let Some(pos) = lower.find("<head>") {
        let insert_at = pos + "<head>".len();
        out.insert_str(insert_at, DARK_THEME_INJECT);
    } else {
        out = format!("{DARK_THEME_INJECT}{out}");
    }

    out
}

fn rewrite_location(location: &str, sonar_host: &str) -> Option<String> {
    let host_base = sonar_host.trim_end_matches('/');
    if location.starts_with(host_base) {
        let rest = location.strip_prefix(host_base).unwrap_or("/");
        return Some(format!("{SONAR_UI_PUBLIC_PREFIX}{rest}"));
    }
    if location.starts_with('/') && !location.starts_with(SONAR_UI_PUBLIC_PREFIX) {
        return Some(format!("{SONAR_UI_PUBLIC_PREFIX}{location}"));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_path_strips_route_prefix() {
        assert_eq!(upstream_path_from_uri("/sonar/ui"), "/");
        assert_eq!(upstream_path_from_uri("/sonar/ui/"), "/");
        assert_eq!(upstream_path_from_uri("/sonar/ui/dashboard"), "/dashboard");
    }

    #[test]
    fn rewrite_fetch_api_paths() {
        let js = r#"fetch("/api/navigation/navigation")"#;
        let out = rewrite_quoted_root_paths(js, SONAR_UI_PUBLIC_PREFIX);
        assert!(out.contains(&format!("fetch(\"{SONAR_UI_PUBLIC_PREFIX}/api/navigation/navigation\")")));
    }

    #[test]
    fn rewrite_root_relative_urls() {
        let html = r#"<script src="/static/main.js"></script><a href="//cdn.example.com/x">"#;
        let out = rewrite_quoted_root_paths(html, SONAR_UI_PUBLIC_PREFIX);
        assert!(out.contains(&format!("src=\"{SONAR_UI_PUBLIC_PREFIX}/static/main.js\"")));
        assert!(out.contains("href=\"//cdn.example.com/x\""));
    }

    #[test]
    fn rewrite_sonar_index_html() {
        let html = r#"<script type="module" crossorigin src="/js/polyfills-GiFbG-Ei.js"></script>
<script type="module" crossorigin src="/js/main-D4nss5BS.js"></script>
<link rel="stylesheet" crossorigin href="/css/main-sc2MJ_RG.css">
<script>
  window.__assetsPath = function (filename) {
    return '/' + filename;
  };
</script>"#;
        let out = rewrite_proxy_text(html, false);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains(&format!("src=\"{SONAR_UI_PUBLIC_PREFIX}/js/polyfills")));
        assert!(s.contains(&format!("href=\"{SONAR_UI_PUBLIC_PREFIX}/css/main")));
        assert!(s.contains(&format!("return '{SONAR_UI_PUBLIC_PREFIX}/' + filename")));
    }

    #[test]
    fn skip_content_encoding_on_response() {
        assert!(should_skip_response_header("content-encoding"));
        assert!(should_skip_response_header("Content-Encoding"));
    }

    #[test]
    fn skip_accept_encoding_on_request() {
        assert!(should_skip_request_header("accept-encoding"));
    }

    #[test]
    fn rewrite_location_to_proxy() {
        assert_eq!(
            rewrite_location("http://localhost:9000/projects", "http://localhost:9000"),
            Some("/api/ship/sonar/ui/projects".into())
        );
        assert_eq!(
            rewrite_location("/sessions/new", "http://localhost:9000"),
            Some("/api/ship/sonar/ui/sessions/new".into())
        );
    }
}
