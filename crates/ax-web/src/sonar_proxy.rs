//! Reverse proxy to local SonarQube — injects admin auth and dark theme.

use std::sync::OnceLock;

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

const DARK_THEME_INJECT: &str = r#"<meta name="color-scheme" content="dark"><script id="ax-sonar-theme">(function(){
  var KEYS=["appearance.theme","sonar.ui.theme","theme","user.theme","sonar.preferences.theme","notifications.optOut"];
  function applyDark(){
    try{
      KEYS.forEach(function(k){ localStorage.setItem(k,"dark"); sessionStorage.setItem(k,"dark"); });
      var r=document.documentElement,b=document.body;
      r.dataset.theme="dark"; r.classList.add("dark"); r.classList.remove("light"); r.style.colorScheme="dark";
      r.setAttribute("data-echoes-theme","dark");
      if(b){ b.dataset.theme="dark"; b.classList.add("dark"); b.classList.remove("light"); b.style.colorScheme="dark"; b.setAttribute("data-echoes-theme","dark"); }
    }catch(e){}
  }
  applyDark();
  document.addEventListener("DOMContentLoaded",applyDark);
  try{
    new MutationObserver(function(){
      var t=document.documentElement.dataset.theme||document.documentElement.getAttribute("data-echoes-theme");
      if(t&&t!=="dark") applyDark();
    }).observe(document.documentElement,{attributes:true,attributeFilter:["data-theme","data-echoes-theme","class"]});
  }catch(e){}
  var n=0,id=setInterval(function(){ applyDark(); if(++n>24)clearInterval(id); },500);
})();</script>
<style id="ax-sonar-dark">
/* === Echoes Design System — force dark tokens === */
:root,:root *,html,html *,
html[data-echoes-theme],html[data-theme],
html[data-echoes-theme="light"],html[data-theme="light"],
html.light,body,body[data-theme],body.light{
  --echoes-color-theme-mode:dark!important;
  color-scheme:dark!important;
  /* Echoes background tokens */
  --echoes-color-background-default:#1a1a1a!important;
  --echoes-color-background-default-hover:#252526!important;
  --echoes-color-background-neutral-default:#252526!important;
  --echoes-color-background-neutral-weak:#1e1e1e!important;
  --echoes-color-background-neutral-bolder:#333!important;
  --echoes-color-background-neutral-weakest:#181818!important;
  --echoes-color-background-accent-default:#2563eb!important;
  --echoes-color-background-accent-weak:#1e3a5f!important;
  --echoes-color-background-accent-weakest:#0f1f3a!important;
  --echoes-color-background-success-default:#16a34a!important;
  --echoes-color-background-success-weak:#103310!important;
  --echoes-color-background-danger-default:#dc2626!important;
  --echoes-color-background-danger-weak:#331010!important;
  --echoes-color-background-warning-default:#ca8a04!important;
  --echoes-color-background-warning-weak:#332b00!important;
  --echoes-color-background-info-default:#2563eb!important;
  --echoes-color-background-info-weak:#102030!important;
  --echoes-color-background-disabled:#333!important;
  --echoes-color-background-selected:#1e3a5f!important;
  --echoes-color-background-input:#1e1e1e!important;
  /* Echoes text tokens */
  --echoes-color-text-default:#e0e0e0!important;
  --echoes-color-text-subdued:#999!important;
  --echoes-color-text-bold:#f0f0f0!important;
  --echoes-color-text-disabled:#666!important;
  --echoes-color-text-accent:#5c9aef!important;
  --echoes-color-text-on-color:#fff!important;
  --echoes-color-text-success:#4ade80!important;
  --echoes-color-text-danger:#f87171!important;
  --echoes-color-text-warning:#facc15!important;
  --echoes-color-text-info:#60a5fa!important;
  /* Echoes border tokens */
  --echoes-color-border-default:#3a3a3a!important;
  --echoes-color-border-bold:#555!important;
  --echoes-color-border-weak:#2d2d2d!important;
  --echoes-color-border-accent:#2563eb!important;
  --echoes-color-border-disabled:#333!important;
  --echoes-color-border-success:#16a34a!important;
  --echoes-color-border-danger:#dc2626!important;
  --echoes-color-border-warning:#ca8a04!important;
  /* Echoes icon tokens */
  --echoes-color-icon-default:#bbb!important;
  --echoes-color-icon-subdued:#888!important;
  --echoes-color-icon-bold:#e0e0e0!important;
  --echoes-color-icon-disabled:#555!important;
  --echoes-color-icon-accent:#5c9aef!important;
  --echoes-color-icon-success:#4ade80!important;
  --echoes-color-icon-danger:#f87171!important;
  --echoes-color-icon-warning:#facc15!important;
  /* Echoes focus / overlay */
  --echoes-color-focus-default:#2563eb!important;
  --echoes-color-overlay-default:rgba(0,0,0,0.6)!important;
  /* SonarQube design-web / sw- tokens */
  --color-background:var(--echoes-color-background-default)!important;
  --color-backgroundSecondary:var(--echoes-color-background-neutral-default)!important;
  --color-backgroundPrimary:var(--echoes-color-background-default)!important;
  --color-text:var(--echoes-color-text-default)!important;
  --color-textSubdued:var(--echoes-color-text-subdued)!important;
  --color-border:#3a3a3a!important;
  --color-borderWeak:#2d2d2d!important;
  --sw-border-color:#3a3a3a!important;
  /* old SQ tokens */
  --background:#1a1a1a!important;
  --backgroundPrimary:#1a1a1a!important;
  --backgroundSecondary:#252526!important;
  --body-bg:#1a1a1a!important;
  --text:#e0e0e0!important;
  --textSubdued:#999!important;
  --border-color:#3a3a3a!important;
}
/* === Global surface overrides === */
html,body{background-color:#1a1a1a!important;color:#e0e0e0!important;}
#content,#content>div,.page-wrapper-simple,.global-container,
.page-container,.layout-page,.layout-page-main,.layout-page-main-inner,
.layout-page-side,.layout-page-side-outer,.layout-page-side-inner,
.page-body,.overview,.overview-panel,.component-container,
.projects-page,.project-activity-page,.measure-content,
[class*="PageWrapper"],[class*="PageContent"],[class*="StyledMain"],
[class*="MainContent"],[class*="Layout"]{
  background-color:#1a1a1a!important;color:#e0e0e0!important;
}
/* Navbar */
nav,.navbar,#navigation,.global-navbar,.global-navbar-menu,
[class*="TopBar"],[class*="GlobalNav"],[class*="IndexationNotification"],
[class*="NavBar"],[class*="MainBar"],[class*="menuHeader"],
header nav,header[class*="TopBar"],
nav[class*="global"],div[class*="globalNav"]{
  background-color:#181818!important;border-color:#333!important;color:#ccc!important;
}
nav a,nav button,.navbar a,.global-navbar a{color:#ccc!important;}
nav a:hover,nav button:hover{color:#fff!important;}
/* Info banner */
.alert-info,[class*="Banner"],[class*="Notification"],[class*="systemAnnouncement"],
.it__system-announcement,[role="status"]{
  background-color:#102030!important;border-color:#204060!important;color:#80b0e0!important;
}
/* Cards + panels */
.card,.boxed-group,.search-navigator-facet-box,.facet-box,.white-page,
.project-card,.overview-quality-gate,.sw-card,
[class*="Card"],[class*="Panel"],[class*="Paper"],
[class*="panel"],[class*="card"],[class*="ListItem"],
div[class*="project-card"],div[class*="Wrapper"]{
  background-color:#252526!important;border-color:#3a3a3a!important;color:#e0e0e0!important;
}
/* Tables */
table,th,td,.code-components-cell,.issue-list{border-color:#3a3a3a!important;}
th{background-color:#1e1e1e!important;color:#aaa!important;}
td{background-color:#252526!important;color:#e0e0e0!important;}
tr:hover td{background-color:#2d2d30!important;}
/* Inputs */
input,select,textarea,.input-search,
[class*="Input"],[class*="SearchBox"],[class*="Select"],[class*="Combobox"]{
  background-color:#313131!important;border-color:#454545!important;color:#e0e0e0!important;
}
input::placeholder,textarea::placeholder{color:#777!important;}
/* Dropdowns / popups */
.dropdown-menu,.popup,.Select-menu-outer,.react-select__menu,
[class*="Popup"],[class*="Dropdown"],[class*="Popover"],[class*="Overlay"],
[class*="MenuContent"],[class*="DropdownContent"],ul[role="listbox"],
[role="menu"]{
  background-color:#252526!important;border-color:#3a3a3a!important;color:#e0e0e0!important;
}
.dropdown-menu li a,.dropdown-menu li button,[role="menuitem"]{color:#ccc!important;}
.dropdown-menu li a:hover,.dropdown-menu li button:hover,[role="menuitem"]:hover{
  background-color:#2d2d30!important;color:#fff!important;
}
/* Links */
a{color:#5c9aef!important;}
a:hover{color:#7cb3ff!important;}
/* Sidebar / facets */
.search-navigator,.search-navigator-facets,.facets-list,.facet-header,
.side-tabs,[class*="Sidebar"],[class*="SideBar"],[class*="sidebar"]{
  background-color:#1a1a1a!important;border-color:#3a3a3a!important;color:#ccc!important;
}
.facet button,.facet a{color:#ccc!important;}
/* Code viewer */
.source-line:hover{background-color:#2d2d30!important;}
code,pre,.code,.source-viewer-code,.source,.code-line,.issue-message,.markdown code{
  background-color:#1e1e1e!important;color:#d4d4d4!important;
}
/* Tabs */
.page-tab,.page-tabs a,.tabs-list a,[role="tab"]{color:#ccc!important;border-color:transparent!important;}
.page-tab.selected,.page-tabs a.active,.tabs-list a.active,[role="tab"][aria-selected="true"]{
  color:#fff!important;border-bottom-color:#5c9aef!important;
}
/* Badges / pills */
.badge,.counter,.tag,.issue-type-icon,[class*="Badge"],[class*="Pill"]{
  border-color:#454545!important;
}
/* Tooltips */
.tooltip-inner,.rc-tooltip-inner,[role="tooltip"]{
  background-color:#333!important;color:#e0e0e0!important;
}
/* Modals */
.modal,.modal-container,.modal-body,.modal-head,.modal-foot,
.react-modal,.ReactModal__Content,[role="dialog"]{
  background-color:#252526!important;border-color:#3a3a3a!important;color:#e0e0e0!important;
}
.modal-overlay,.ReactModal__Overlay{background-color:rgba(0,0,0,0.6)!important;}
/* Buttons */
button.button-red,button.button-primary{color:#fff!important;}
/* Scrollbar */
::-webkit-scrollbar{width:10px;height:10px;}
::-webkit-scrollbar-track{background:#1a1a1a;}
::-webkit-scrollbar-thumb{background:#424242;border-radius:0;}
*{scrollbar-width:thin;scrollbar-color:#424242 #1a1a1a;}
/* Alerts */
.alert.alert-warning{background-color:#332b00!important;border-color:#554400!important;color:#e0c040!important;}
.alert.alert-danger{background-color:#331010!important;border-color:#552020!important;color:#f08080!important;}
.alert.alert-success{background-color:#103310!important;border-color:#205520!important;color:#80e080!important;}
.alert.alert-info{background-color:#102030!important;border-color:#204060!important;color:#80b0e0!important;}
/* Headings */
h1,h2,h3,h4,h5,h6,.page-title{color:#e0e0e0!important;}
/* Secondary text */
.note,.text-muted,.text-muted-2,.subtitle,label,.field-label,
[class*="Subdued"],[class*="subdued"],[class*="muted"]{color:#999!important;}
/* Loading bar */
.global-loading .bar{background-color:#5c9aef!important;}
/* === sw-* Tailwind utility overrides === */
[class*="sw-bg-white"]{background-color:#1a1a1a!important;}
[class*="sw-bg-gray"]{background-color:#252526!important;}
[class*="sw-text-black"]{color:#e0e0e0!important;}
[class*="sw-text-gray"]{color:#999!important;}
[class*="sw-border-gray"]{border-color:#3a3a3a!important;}
[class*="sw-body-sm"]{color:#e0e0e0!important;}
/* SonarQube Emotion (CSS-in-JS) overrides via attribute selectors */
[class*="css-"][style*="background-color: rgb(255"],
[class*="css-"][style*="background-color: white"],
[class*="css-"][style*="background: rgb(255"],
[class*="css-"][style*="background: white"]{
  background-color:#1a1a1a!important;
}
[class*="css-"][style*="color: rgb(29"],
[class*="css-"][style*="color: rgb(0"]{
  color:#e0e0e0!important;
}
/* Catch-all: override any white/light inline backgrounds on divs/sections */
div[style*="background-color: rgb(255"],
div[style*="background-color: white"],
section[style*="background-color: rgb(255"],
section[style*="background-color: white"],
main[style*="background-color: rgb(255"],
main[style*="background-color: white"]{
  background-color:#1a1a1a!important;
}
</style>"#;

/// Cached upstream host + auth — populated from ship config (no podman/HTTP probe per request).
#[derive(Default)]
pub struct SonarProxyCache {
    host: String,
    auth_header: String,
    ready: bool,
}

impl SonarProxyCache {
    pub fn invalidate(&mut self) {
        self.ready = false;
        self.host.clear();
        self.auth_header.clear();
    }

    fn ensure(&mut self, config: &ShipConfig) {
        if self.ready {
            return;
        }
        self.host = config.sonar.host.trim_end_matches('/').to_string();
        self.auth_header = basic_auth(&config.sonar.admin_user, &config.sonar.admin_password);
        self.ready = true;
    }

    fn set_host(&mut self, host: String) {
        self.host = host;
        self.ready = true;
    }
}

fn sonar_http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .pool_max_idle_per_host(64)
            .connect_timeout(std::time::Duration::from_secs(2))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

pub async fn handle_sonar_ui_info(State(hub): State<WebHub>) -> impl IntoResponse {
    let config = sonar_config(&hub).await;
    let host = config.sonar.host.trim_end_matches('/').to_string();
    let reachable = tokio::task::spawn_blocking({
        let host = host.clone();
        move || ax_quality::sonar_ping_fast(&host)
    })
    .await
    .unwrap_or(false);
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
    let (mut sonar_host, auth_header) = proxy_credentials(&hub).await;

    let method = req.method().clone();
    let uri = req.uri().clone();
    let upstream_path = upstream_path_from_uri(uri.path());
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();

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

    let client = sonar_http();
    let mut upstream_url = format!(
        "{}{}{}",
        sonar_host.trim_end_matches('/'),
        upstream_path,
        query
    );

    let resp = match send_upstream(&client, &method, &upstream_url, &auth_header, &parts, &body_bytes).await
    {
        Ok(r) => r,
        Err(_) => {
            let mut last_err = None;
            let mut recovered = None;
            for candidate in ax_quality::sonar_localhost_candidates(&sonar_host) {
                upstream_url = format!(
                    "{}{}{}",
                    candidate.trim_end_matches('/'),
                    upstream_path,
                    query
                );
                match send_upstream(&client, &method, &upstream_url, &auth_header, &parts, &body_bytes)
                    .await
                {
                    Ok(r) => {
                        hub.sonar_proxy.lock().await.set_host(candidate.clone());
                        sonar_host = candidate;
                        recovered = Some(r);
                        break;
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            match recovered {
                Some(r) => r,
                None => {
                    hub.sonar_proxy.lock().await.invalidate();
                    return (
                        StatusCode::BAD_GATEWAY,
                        format!(
                            "SonarQube proxy error: {}",
                            last_err.map(|e| e.to_string()).unwrap_or_else(|| "upstream unreachable".into())
                        ),
                    )
                        .into_response();
                }
            }
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
        let mut bytes = rewrite_proxy_text(&text, inject_theme);
        if content_type.contains("json") {
            bytes = patch_sonar_theme_json(&bytes, &upstream_path);
        }
        Bytes::from(bytes)
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
    if inject_theme {
        out_headers.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-cache, must-revalidate"),
        );
        out_headers.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
        spawn_sonar_dark_theme_sync(&hub);
    }

    let mut response = Response::new(Body::from(body_out));
    *response.status_mut() = status;
    *response.headers_mut() = out_headers;
    response
}

async fn proxy_credentials(hub: &WebHub) -> (String, String) {
    {
        let cache = hub.sonar_proxy.lock().await;
        if cache.ready {
            return (cache.host.clone(), cache.auth_header.clone());
        }
    }
    let config = sonar_config(hub).await;
    let mut cache = hub.sonar_proxy.lock().await;
    cache.ensure(&config);
    (cache.host.clone(), cache.auth_header.clone())
}

async fn send_upstream(
    client: &reqwest::Client,
    method: &Method,
    upstream_url: &str,
    auth_header: &str,
    parts: &axum::http::request::Parts,
    body_bytes: &Bytes,
) -> Result<reqwest::Response, reqwest::Error> {
    let mut upstream = client.request(method.clone(), upstream_url);
    upstream = upstream.header("Authorization", auth_header);

    for (name, value) in &parts.headers {
        let n = name.as_str();
        if should_skip_request_header(n) {
            continue;
        }
        upstream = upstream.header(name, value);
    }

    if !body_bytes.is_empty()
        || method == Method::POST
        || method == Method::PUT
        || method == Method::PATCH
    {
        upstream = upstream.body(body_bytes.to_vec());
    }

    upstream.send().await
}

async fn sonar_config(hub: &WebHub) -> ShipConfig {
    let daemon = {
        let ws = hub.read().await;
        ws.ship.daemon.clone()
    };
    daemon.config().await
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
    if !inject_theme && !might_need_url_rewrite(text) {
        return text.as_bytes().to_vec();
    }
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

/// Skip O(n) rewrite when Sonar bundles contain no root-relative URLs.
fn might_need_url_rewrite(text: &str) -> bool {
    text.contains("\"/")
        || text.contains("'/")
        || text.contains("url(/")
        || text.contains("return '/'")
        || text.contains("return \"/\"")
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

fn slash_starts_route_path(chars: &[char], slash_i: usize) -> bool {
    if slash_i + 1 >= chars.len() {
        return false;
    }
    let next = chars[slash_i + 1];
    if next == '/' {
        return false;
    }
    // Only rewrite static asset paths in JS — API and app routes use J()/baseUrl + relative path.
    if !next.is_ascii_alphabetic() {
        return false;
    }
    let rest: String = chars[slash_i + 1..].iter().collect();
    should_rewrite_js_asset_path(&rest)
}

/// Paths safe to prefix in minified JS (not `/api/*` or SPA routes joined via `J()`).
fn should_rewrite_js_asset_path(path: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "js/",
        "css/",
        "static/",
        "fonts/",
        "images/",
        "webfonts/",
        "apple-touch",
        "favicon",
        "mstile",
    ];
    PREFIXES.iter().any(|p| path.starts_with(p))
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
            && slash_starts_route_path(&chars, i + 2)
        {
            let quote = chars[i + 1];
            // Skip ("/") — URL join fragment, not a route.
            if i + 3 < n && chars[i + 3] == quote {
                out.push(chars[i]);
                i += 1;
                continue;
            }
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
            && slash_starts_route_path(&chars, i + 1)
        {
            let quote = chars[i];
            // Skip "/" and '/' — join segments (`+"/"+path`) and cookie paths, not routes.
            if i + 2 < n && chars[i + 2] == quote {
                out.push(chars[i]);
                i += 1;
                continue;
            }
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
            if i < n && chars[i] == '/' && slash_starts_route_path(&chars, i) {
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
    let mut out = rewrite_sonar_base_url(html, SONAR_UI_PUBLIC_PREFIX);

    if let Some(insert_at) = head_content_start(&out) {
        out.insert_str(insert_at, DARK_THEME_INJECT);
    } else if let Some(pos) = out.to_lowercase().find("</head>") {
        out.insert_str(pos, DARK_THEME_INJECT);
    } else {
        out = format!("{DARK_THEME_INJECT}{out}");
    }

    out
}

/// Insert immediately after `<head>` or `<head …>` — Sonar uses attributes on `<head>`.
fn head_content_start(html: &str) -> Option<usize> {
    let lower = html.to_lowercase();
    let start = lower.find("<head")?;
    let rel = html.get(start..)?.find('>')?;
    Some(start + rel + 1)
}

/// Force dark theme in Sonar user-preference JSON served through the proxy.
fn patch_sonar_theme_json(bytes: &[u8], upstream_path: &str) -> Vec<u8> {
    let path = upstream_path.to_ascii_lowercase();
    if !path.contains("user_preference") && !path.contains("/users/current") {
        return bytes.to_vec();
    }
    let text = String::from_utf8_lossy(bytes);
    let out = text
        .replace(r#""key":"appearance.theme","value":"light""#, r#""key":"appearance.theme","value":"dark""#)
        .replace(r#""key":"appearance.theme","value":"system""#, r#""key":"appearance.theme","value":"dark""#)
        .replace(r#""key":"theme","value":"light""#, r#""key":"theme","value":"dark""#)
        .replace(r#""key":"theme","value":"system""#, r#""key":"theme","value":"dark""#)
        .replace(r#""theme":"light""#, r#""theme":"dark""#)
        .replace(r#""theme":"system""#, r#""theme":"dark""#)
        .replace(r#""value":"light""#, r#""value":"dark""#);
    out.into_bytes()
}

fn spawn_sonar_dark_theme_sync(hub: &WebHub) {
    let hub = hub.clone();
    tokio::spawn(async move {
        let config = {
            let ws = hub.read().await;
            ws.ship.daemon.config().await
        };
        ax_quality::ensure_sonar_dark_theme(
            &config.sonar.host,
            &config.sonar.admin_user,
            &config.sonar.admin_password,
        )
        .await;
    });
}

/// SonarQube React Router reads `#content[data-base-url]` as basename — must match the proxy prefix.
fn rewrite_sonar_base_url(html: &str, prefix: &str) -> String {
    let base = if prefix.ends_with('/') {
        prefix.to_string()
    } else {
        format!("{prefix}/")
    };
    html.replace(
        "data-base-url=\"\"",
        &format!("data-base-url=\"{base}\""),
    )
    .replace(
        "data-base-url=''",
        &format!("data-base-url='{base}'"),
    )
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
        assert_eq!(out, js, "API paths are joined via J()+path, not rewritten in JS");
    }

    #[test]
    fn rewrite_js_asset_paths_only() {
        let js = r#"fetch("/api/foo");import("/js/main.js");"/sessions""#;
        let out = rewrite_quoted_root_paths(js, SONAR_UI_PUBLIC_PREFIX);
        assert!(out.contains(&format!("{SONAR_UI_PUBLIC_PREFIX}/js/main.js")));
        assert!(out.contains(r#"fetch("/api/foo")"#));
        assert!(out.contains(r#""/sessions""#));
    }

    #[test]
    fn inject_theme_before_scripts_with_head_attributes() {
        let html = r#"<!doctype html><html><head lang="en"><script src="/js/main.js"></script></head><body></body></html>"#;
        let out = inject_dark_theme_html(html);
        let script_pos = out.find("<script src=\"/js/main.js\">").unwrap();
        let inject_pos = out.find("ax-sonar-theme").unwrap();
        assert!(inject_pos < script_pos, "theme inject must precede Sonar scripts");
    }

    #[test]
    fn patch_user_preference_json_to_dark() {
        let json = r#"{"preferences":[{"key":"appearance.theme","value":"light"}]}"#;
        let out = patch_sonar_theme_json(json.as_bytes(), "/api/user_preferences/search");
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains(r#""value":"dark""#));
    }

    #[test]
    fn inject_theme_at_start_of_head() {
        let html = r#"<!doctype html><html><head><script src="/js/main.js"></script></head><body></body></html>"#;
        let out = inject_dark_theme_html(html);
        let head_end = out.find("<script src=\"/js/main.js\">").unwrap();
        let inject_pos = out.find("ax-sonar-theme").unwrap();
        assert!(inject_pos < head_end, "theme inject must precede Sonar scripts");
        assert!(out.contains("MutationObserver"));
        assert!(!out.contains("<base href"));
    }

    #[test]
    fn rewrite_sonar_base_url_attribute() {
        let html = r#"<div id="content" data-base-url="" data-server-status="UP">"#;
        let out = rewrite_sonar_base_url(html, SONAR_UI_PUBLIC_PREFIX);
        assert!(out.contains(r#"data-base-url="/api/ship/sonar/ui/""#));
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
    fn rewrite_fast_path_skips_clean_json() {
        let json = r#"{"status":"UP","version":"10.0"}"#;
        assert_eq!(rewrite_proxy_text(json, false), json.as_bytes());
    }

    #[test]
    fn might_need_url_rewrite_detects_root_paths() {
        assert!(might_need_url_rewrite(r#"fetch("/api/foo")"#));
        assert!(!might_need_url_rewrite("plain text without urls"));
    }

    #[test]
    fn skip_accept_encoding_on_request() {
        assert!(should_skip_request_header("accept-encoding"));
    }

    #[test]
    fn rewrite_preserves_wildcard_and_markup_fragments() {
        let js = r#"const a="/*",b="/>",c="/ ""#;
        let out = rewrite_quoted_root_paths(js, SONAR_UI_PUBLIC_PREFIX);
        assert!(out.contains(r#""/*""#), "{out}");
        assert!(out.contains(r#""/>""#), "{out}");
        assert!(out.contains(r#""/ ""#), "{out}");
    }

    #[test]
    fn rewrite_preserves_url_join_slash() {
        let js = r#"function un(e,t){return t?e.replace(/\/?\/$/,"")+"/"+t.replace(/^\/+/,""):e}"#;
        let out = rewrite_quoted_root_paths(js, SONAR_UI_PUBLIC_PREFIX);
        assert!(out.contains(r#"+"/"+t"#), "must not rewrite join slash: {out}");
        assert!(!out.contains("sonar/ui/\"+t"));
    }

    #[test]
    fn rewrite_still_prefixes_api_paths() {
        let js = r#"fetch("/api/navigation/global")"#;
        let out = rewrite_quoted_root_paths(js, SONAR_UI_PUBLIC_PREFIX);
        assert_eq!(out, js);
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
