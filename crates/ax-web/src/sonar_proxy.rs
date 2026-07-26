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

/// Patch fetch/XHR so absolute `/api/…` (and static assets) stay on the proxy.
/// SonarQube's axios treats leading-slash paths as host-absolute and ignores
/// `data-base-url` — that breaks the iframe locally and via Cloudflare tunnels.
const PROXY_PATH_PATCH: &str = r#"<script id="ax-sonar-proxy-path">(function(){
  var P='/api/ship/sonar/ui';
  function needs(u){
    return u.charAt(0)==='/' && u.indexOf(P+'/')!==0 && u!==P && (
      u.indexOf('/api/')===0 || u.indexOf('/js/')===0 || u.indexOf('/css/')===0 ||
      u.indexOf('/static/')===0 || u.indexOf('/fonts/')===0 || u.indexOf('/images/')===0 ||
      u.indexOf('/webfonts/')===0 || u.indexOf('/batch_bootstrap')===0
    );
  }
  function fix(u){
    if(typeof u!=='string'||!u) return u;
    if(needs(u)) return P+u;
    if(u.indexOf('http')===0){
      try{
        var a=document.createElement('a'); a.href=u;
        if(a.origin===location.origin){
          var path=a.pathname+(a.search||'')+(a.hash||'');
          if(needs(a.pathname)) return P+path;
        }
      }catch(e){}
    }
    return u;
  }
  var _f=window.fetch;
  window.fetch=function(input, init){
    try{
      if(typeof input==='string') input=fix(input);
      else if(input && typeof Request!=='undefined' && input instanceof Request){
        var nu=fix(input.url);
        if(nu!==input.url) input=new Request(nu, input);
      }
    }catch(e){}
    return _f.call(this, input, init);
  };
  var xo=XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open=function(method, url){
    if(typeof url==='string') arguments[1]=fix(url);
    return xo.apply(this, arguments);
  };
})();</script>"#;

const DARK_THEME_INJECT: &str = r#"<meta name="color-scheme" content="dark"><script id="ax-sonar-theme">(function(){
  var T={
    'ax':{a:'#3ee4b2',bg:'#1e1e1e',bs:'#181818',bi:'#313131',bh:'#252826',ba:'#2e3532',bd:'#2b2b2b',bH:'#454545',t:'#cccccc',td:'#9d9d9d',th:'#ffffff',ok:'#3fb950',dg:'#f14c4c',wn:'#cca700'},
    'vscode-dark':{a:'#0078d4',bg:'#1f1f1f',bs:'#181818',bi:'#313131',bh:'#2a2d2e',ba:'#37373d',bd:'#2b2b2b',bH:'#454545',t:'#cccccc',td:'#9d9d9d',th:'#ffffff',ok:'#3fb950',dg:'#f14c4c',wn:'#cca700'},
    'ember':{a:'#e06c2b',bg:'#1a1a1a',bs:'#141414',bi:'#2a2a2a',bh:'#2c2420',ba:'#3a2e26',bd:'#2a2420',bH:'#4a3a30',t:'#cccccc',td:'#9d9d9d',th:'#ffffff',ok:'#3fb950',dg:'#f14c4c',wn:'#e0a030'},
    'emerald':{a:'#2ea87a',bg:'#1a1c1a',bs:'#141614',bi:'#262e28',bh:'#222e26',ba:'#2a3a2e',bd:'#222e24',bH:'#3a4a3e',t:'#cccccc',td:'#9d9d9d',th:'#ffffff',ok:'#3fb950',dg:'#f14c4c',wn:'#cca700'},
    'nightfall':{a:'#8b5cf6',bg:'#1a1a22',bs:'#14141c',bi:'#282838',bh:'#252535',ba:'#30304a',bd:'#252530',bH:'#3a3a50',t:'#cccccc',td:'#9d9d9d',th:'#ffffff',ok:'#3fb950',dg:'#f14c4c',wn:'#cca700'},
    'crimson':{a:'#dc3545',bg:'#1c1a1a',bs:'#161414',bi:'#302828',bh:'#2e2424',ba:'#3e2e2e',bd:'#2c2222',bH:'#4a3535',t:'#cccccc',td:'#9d9d9d',th:'#ffffff',ok:'#3fb950',dg:'#f14c4c',wn:'#cca700'},
    'ocean':{a:'#22a2c8',bg:'#1a1c1e',bs:'#141618',bi:'#262e34',bh:'#222e34',ba:'#2a3842',bd:'#222830',bH:'#364450',t:'#cccccc',td:'#9d9d9d',th:'#ffffff',ok:'#3fb950',dg:'#f14c4c',wn:'#cca700'}
  };
  function get(){try{return T[localStorage.getItem('ax-theme')]||T['ax'];}catch(e){return T['ax'];}}
  function vars(th){
    var s=document.documentElement.style;
    s.setProperty('--ax-a',th.a);s.setProperty('--ax-bg',th.bg);s.setProperty('--ax-bs',th.bs);
    s.setProperty('--ax-bi',th.bi);s.setProperty('--ax-bh',th.bh);s.setProperty('--ax-ba',th.ba);
    s.setProperty('--ax-bd',th.bd);s.setProperty('--ax-bH',th.bH);s.setProperty('--ax-t',th.t);
    s.setProperty('--ax-td',th.td);s.setProperty('--ax-th',th.th);s.setProperty('--ax-ok',th.ok);
    s.setProperty('--ax-dg',th.dg);s.setProperty('--ax-wn',th.wn);
  }
  var KEYS=["appearance.theme","sonar.ui.theme","theme","user.theme","sonar.preferences.theme","notifications.optOut"];
  function apply(){
    try{
      KEYS.forEach(function(k){localStorage.setItem(k,"dark");sessionStorage.setItem(k,"dark");});
      var r=document.documentElement,b=document.body;
      r.dataset.theme="dark";r.classList.add("dark");r.classList.remove("light");r.style.colorScheme="dark";
      r.setAttribute("data-echoes-theme","dark");
      if(b){b.dataset.theme="dark";b.classList.add("dark");b.classList.remove("light");b.style.colorScheme="dark";b.setAttribute("data-echoes-theme","dark");}
    }catch(e){}
    vars(get());
  }
  apply();
  document.addEventListener("DOMContentLoaded",apply);
  window.addEventListener("storage",function(e){if(e.key==="ax-theme")vars(get());});
  window.addEventListener("message",function(e){if(e.data&&e.data.type==="ax-theme-change")vars(get());});
  try{
    new MutationObserver(function(){
      var t=document.documentElement.dataset.theme||document.documentElement.getAttribute("data-echoes-theme");
      if(t&&t!=="dark")apply();
    }).observe(document.documentElement,{attributes:true,attributeFilter:["data-theme","data-echoes-theme","class"]});
  }catch(e){}
  var n=0,id=setInterval(function(){apply();if(++n>24)clearInterval(id);},500);
})();</script>
<style id="ax-sonar-dark">
/* === ax Command Center — theme-aware SonarQube overrides === */
:root,:root *,html,html *,
html[data-echoes-theme],html[data-theme],
html[data-echoes-theme="light"],html[data-theme="light"],
html.light,body,body[data-theme],body.light{
  --echoes-color-theme-mode:dark!important;
  color-scheme:dark!important;
  --echoes-color-background-default:var(--ax-bg)!important;
  --echoes-color-background-default-hover:var(--ax-bh)!important;
  --echoes-color-background-neutral-default:var(--ax-ba)!important;
  --echoes-color-background-neutral-weak:var(--ax-bi)!important;
  --echoes-color-background-neutral-bolder:var(--ax-bH)!important;
  --echoes-color-background-neutral-weakest:var(--ax-bs)!important;
  --echoes-color-background-accent-default:var(--ax-a)!important;
  --echoes-color-background-accent-weak:color-mix(in srgb,var(--ax-a) 15%,var(--ax-bg))!important;
  --echoes-color-background-accent-weakest:color-mix(in srgb,var(--ax-a) 6%,var(--ax-bg))!important;
  --echoes-color-background-success-default:var(--ax-ok)!important;
  --echoes-color-background-success-weak:color-mix(in srgb,var(--ax-ok) 12%,var(--ax-bg))!important;
  --echoes-color-background-danger-default:var(--ax-dg)!important;
  --echoes-color-background-danger-weak:color-mix(in srgb,var(--ax-dg) 12%,var(--ax-bg))!important;
  --echoes-color-background-warning-default:var(--ax-wn)!important;
  --echoes-color-background-warning-weak:color-mix(in srgb,var(--ax-wn) 12%,var(--ax-bg))!important;
  --echoes-color-background-info-default:var(--ax-a)!important;
  --echoes-color-background-info-weak:color-mix(in srgb,var(--ax-a) 8%,var(--ax-bg))!important;
  --echoes-color-background-disabled:var(--ax-bH)!important;
  --echoes-color-background-selected:color-mix(in srgb,var(--ax-a) 15%,var(--ax-bg))!important;
  --echoes-color-background-input:var(--ax-bi)!important;
  /* Text */
  --echoes-color-text-default:var(--ax-t)!important;
  --echoes-color-text-subdued:var(--ax-td)!important;
  --echoes-color-text-bold:var(--ax-th)!important;
  --echoes-color-text-disabled:color-mix(in srgb,var(--ax-td) 50%,var(--ax-bg))!important;
  --echoes-color-text-accent:var(--ax-a)!important;
  --echoes-color-text-on-color:var(--ax-bg)!important;
  --echoes-color-text-success:var(--ax-ok)!important;
  --echoes-color-text-danger:var(--ax-dg)!important;
  --echoes-color-text-warning:var(--ax-wn)!important;
  --echoes-color-text-info:var(--ax-a)!important;
  /* Borders */
  --echoes-color-border-default:var(--ax-bd)!important;
  --echoes-color-border-bold:var(--ax-bH)!important;
  --echoes-color-border-weak:color-mix(in srgb,var(--ax-bd) 60%,var(--ax-bg))!important;
  --echoes-color-border-accent:var(--ax-a)!important;
  --echoes-color-border-disabled:var(--ax-bd)!important;
  --echoes-color-border-success:var(--ax-ok)!important;
  --echoes-color-border-danger:var(--ax-dg)!important;
  --echoes-color-border-warning:var(--ax-wn)!important;
  /* Icons */
  --echoes-color-icon-default:var(--ax-td)!important;
  --echoes-color-icon-subdued:color-mix(in srgb,var(--ax-td) 70%,var(--ax-bg))!important;
  --echoes-color-icon-bold:var(--ax-th)!important;
  --echoes-color-icon-disabled:color-mix(in srgb,var(--ax-td) 40%,var(--ax-bg))!important;
  --echoes-color-icon-accent:var(--ax-a)!important;
  --echoes-color-icon-success:var(--ax-ok)!important;
  --echoes-color-icon-danger:var(--ax-dg)!important;
  --echoes-color-icon-warning:var(--ax-wn)!important;
  /* Focus / overlay */
  --echoes-color-focus-default:var(--ax-a)!important;
  --echoes-color-overlay-default:rgba(0,0,0,0.7)!important;
  /* SonarQube design-web tokens */
  --color-background:var(--ax-bg)!important;
  --color-backgroundSecondary:var(--ax-ba)!important;
  --color-backgroundPrimary:var(--ax-bg)!important;
  --color-text:var(--ax-t)!important;
  --color-textSubdued:var(--ax-td)!important;
  --color-border:var(--ax-bd)!important;
  --color-borderWeak:color-mix(in srgb,var(--ax-bd) 60%,var(--ax-bg))!important;
  --sw-border-color:var(--ax-bd)!important;
  /* Legacy SQ tokens */
  --background:var(--ax-bg)!important;
  --backgroundPrimary:var(--ax-bg)!important;
  --backgroundSecondary:var(--ax-ba)!important;
  --body-bg:var(--ax-bg)!important;
  --text:var(--ax-t)!important;
  --textSubdued:var(--ax-td)!important;
  --border-color:var(--ax-bd)!important;
}
/* === Global surfaces === */
html,body{background-color:var(--ax-bg)!important;color:var(--ax-t)!important;}
#content,#content>div,.page-wrapper-simple,.global-container,
.page-container,.layout-page,.layout-page-main,.layout-page-main-inner,
.layout-page-side,.layout-page-side-outer,.layout-page-side-inner,
.page-body,.overview,.overview-panel,.component-container,
.projects-page,.project-activity-page,.measure-content,
[class*="PageWrapper"],[class*="PageContent"],[class*="StyledMain"],
[class*="MainContent"],[class*="Layout"]{
  background-color:var(--ax-bg)!important;color:var(--ax-t)!important;
}
/* Navbar */
nav,.navbar,#navigation,.global-navbar,.global-navbar-menu,
[class*="TopBar"],[class*="GlobalNav"],[class*="IndexationNotification"],
[class*="NavBar"],[class*="MainBar"],[class*="menuHeader"],
header nav,header[class*="TopBar"],
nav[class*="global"],div[class*="globalNav"]{
  background-color:var(--ax-bs)!important;border-color:var(--ax-bd)!important;color:var(--ax-td)!important;
}
nav a,nav button,.navbar a,.global-navbar a{color:var(--ax-td)!important;}
nav a:hover,nav button:hover{color:var(--ax-th)!important;}
/* Info banner */
.alert-info,[class*="Banner"],[class*="Notification"],[class*="systemAnnouncement"],
.it__system-announcement,[role="status"]{
  background-color:color-mix(in srgb,var(--ax-a) 8%,var(--ax-bg))!important;
  border-color:color-mix(in srgb,var(--ax-a) 20%,var(--ax-bg))!important;
  color:color-mix(in srgb,var(--ax-a) 70%,var(--ax-th))!important;
}
/* Cards + panels */
.card,.boxed-group,.search-navigator-facet-box,.facet-box,.white-page,
.project-card,.overview-quality-gate,.sw-card,
[class*="Card"],[class*="Panel"],[class*="Paper"],
[class*="panel"],[class*="card"],[class*="ListItem"],
div[class*="project-card"],div[class*="Wrapper"]{
  background-color:var(--ax-ba)!important;border-color:var(--ax-bd)!important;color:var(--ax-t)!important;
}
/* Tables */
table,th,td,.code-components-cell,.issue-list{border-color:var(--ax-bd)!important;}
th{background-color:var(--ax-bi)!important;color:var(--ax-td)!important;}
td{background-color:var(--ax-ba)!important;color:var(--ax-t)!important;}
tr:hover td{background-color:var(--ax-bh)!important;}
/* Inputs */
input,select,textarea,.input-search,
[class*="Input"],[class*="SearchBox"],[class*="Select"],[class*="Combobox"]{
  background-color:var(--ax-bi)!important;border-color:var(--ax-bH)!important;color:var(--ax-t)!important;
}
input::placeholder,textarea::placeholder{color:var(--ax-td)!important;}
/* Dropdowns / popups */
.dropdown-menu,.popup,.Select-menu-outer,.react-select__menu,
[class*="Popup"],[class*="Dropdown"],[class*="Popover"],[class*="Overlay"],
[class*="MenuContent"],[class*="DropdownContent"],ul[role="listbox"],
[role="menu"]{
  background-color:var(--ax-ba)!important;border-color:var(--ax-bd)!important;color:var(--ax-t)!important;
}
.dropdown-menu li a,.dropdown-menu li button,[role="menuitem"]{color:var(--ax-td)!important;}
.dropdown-menu li a:hover,.dropdown-menu li button:hover,[role="menuitem"]:hover{
  background-color:var(--ax-bh)!important;color:var(--ax-th)!important;
}
/* Links — theme accent */
a{color:var(--ax-a)!important;}
a:hover{color:color-mix(in srgb,var(--ax-a) 80%,var(--ax-th))!important;}
/* Sidebar / facets */
.search-navigator,.search-navigator-facets,.facets-list,.facet-header,
.side-tabs,[class*="Sidebar"],[class*="SideBar"],[class*="sidebar"]{
  background-color:var(--ax-bg)!important;border-color:var(--ax-bd)!important;color:var(--ax-td)!important;
}
.facet button,.facet a{color:var(--ax-td)!important;}
/* Code viewer */
.source-line:hover{background-color:var(--ax-bh)!important;}
code,pre,.code,.source-viewer-code,.source,.code-line,.issue-message,.markdown code{
  background-color:var(--ax-bi)!important;color:var(--ax-t)!important;
}
/* Tabs */
.page-tab,.page-tabs a,.tabs-list a,[role="tab"]{color:var(--ax-td)!important;border-color:transparent!important;}
.page-tab.selected,.page-tabs a.active,.tabs-list a.active,[role="tab"][aria-selected="true"]{
  color:var(--ax-th)!important;border-bottom-color:var(--ax-a)!important;
}
/* Badges / pills */
.badge,.counter,.tag,.issue-type-icon,[class*="Badge"],[class*="Pill"]{
  border-color:var(--ax-bH)!important;
}
/* Tooltips */
.tooltip-inner,.rc-tooltip-inner,[role="tooltip"]{
  background-color:var(--ax-bh)!important;color:var(--ax-t)!important;
}
/* Modals */
.modal,.modal-container,.modal-body,.modal-head,.modal-foot,
.react-modal,.ReactModal__Content,[role="dialog"]{
  background-color:var(--ax-ba)!important;border-color:var(--ax-bd)!important;color:var(--ax-t)!important;
}
.modal-overlay,.ReactModal__Overlay{background-color:rgba(0,0,0,0.7)!important;}
/* Buttons */
button.button-red,button.button-primary{color:var(--ax-bg)!important;background-color:var(--ax-a)!important;}
/* Scrollbar */
::-webkit-scrollbar{width:10px;height:10px;}
::-webkit-scrollbar-track{background:var(--ax-bg);}
::-webkit-scrollbar-thumb{background:var(--ax-bd);border-radius:0;}
*{scrollbar-width:thin;scrollbar-color:var(--ax-bd) var(--ax-bg);}
/* Alerts */
.alert.alert-warning{background-color:color-mix(in srgb,var(--ax-wn) 10%,var(--ax-bg))!important;border-color:color-mix(in srgb,var(--ax-wn) 25%,var(--ax-bg))!important;color:var(--ax-wn)!important;}
.alert.alert-danger{background-color:color-mix(in srgb,var(--ax-dg) 10%,var(--ax-bg))!important;border-color:color-mix(in srgb,var(--ax-dg) 25%,var(--ax-bg))!important;color:var(--ax-dg)!important;}
.alert.alert-success{background-color:color-mix(in srgb,var(--ax-ok) 10%,var(--ax-bg))!important;border-color:color-mix(in srgb,var(--ax-ok) 25%,var(--ax-bg))!important;color:var(--ax-ok)!important;}
.alert.alert-info{background-color:color-mix(in srgb,var(--ax-a) 8%,var(--ax-bg))!important;border-color:color-mix(in srgb,var(--ax-a) 20%,var(--ax-bg))!important;color:color-mix(in srgb,var(--ax-a) 70%,var(--ax-th))!important;}
/* Headings */
h1,h2,h3,h4,h5,h6,.page-title{color:var(--ax-th)!important;}
/* Secondary text */
.note,.text-muted,.text-muted-2,.subtitle,label,.field-label,
[class*="Subdued"],[class*="subdued"],[class*="muted"]{color:var(--ax-td)!important;}
/* Loading bar */
.global-loading .bar{background-color:var(--ax-a)!important;}
/* === sw-* Tailwind utility overrides === */
[class*="sw-bg-white"]{background-color:var(--ax-bg)!important;}
[class*="sw-bg-gray"]{background-color:var(--ax-ba)!important;}
[class*="sw-text-black"]{color:var(--ax-t)!important;}
[class*="sw-text-gray"]{color:var(--ax-td)!important;}
[class*="sw-border-gray"]{border-color:var(--ax-bd)!important;}
[class*="sw-body-sm"]{color:var(--ax-t)!important;}
/* SonarQube Emotion (CSS-in-JS) overrides */
[class*="css-"][style*="background-color: rgb(255"],
[class*="css-"][style*="background-color: white"],
[class*="css-"][style*="background: rgb(255"],
[class*="css-"][style*="background: white"]{
  background-color:var(--ax-bg)!important;
}
[class*="css-"][style*="color: rgb(29"],
[class*="css-"][style*="color: rgb(0"]{
  color:var(--ax-t)!important;
}
div[style*="background-color: rgb(255"],
div[style*="background-color: white"],
section[style*="background-color: rgb(255"],
section[style*="background-color: white"],
main[style*="background-color: rgb(255"],
main[style*="background-color: white"]{
  background-color:var(--ax-bg)!important;
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
                    out_headers.insert(
                        header::LOCATION,
                        HeaderValue::from_str(&rewritten).unwrap_or_else(|_| value.clone()),
                    );
                    continue;
                }
            }
        }
        if n.eq_ignore_ascii_case("set-cookie") {
            if let Ok(v) = value.to_str() {
                let rewritten = rewrite_set_cookie(v);
                if let Ok(hv) = HeaderValue::from_str(&rewritten) {
                    out_headers.append(header::SET_COOKIE, hv);
                    continue;
                }
            }
        }
        out_headers.append(name.clone(), value.clone());
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
            // Tunnel / browser Origin+Referer confuse Sonar CSRF and absolute redirects.
            | "origin"
            | "referer"
            | "cf-connecting-ip"
            | "cf-ray"
            | "cf-visitor"
            | "cdn-loop"
            | "true-client-ip"
            | "x-forwarded-for"
            | "x-forwarded-proto"
            | "x-forwarded-host"
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

/// Paths safe to prefix in minified JS / HTML.
/// Includes `/api/*` — axios treats leading-slash URLs as host-absolute and
/// ignores Sonar’s `data-base-url`, so they must be rewritten for the proxy.
fn should_rewrite_js_asset_path(path: &str) -> bool {
    if path.starts_with("api/") {
        // Already under our public prefix (e.g. after a prior rewrite pass).
        return !path.starts_with("api/ship/sonar/ui");
    }
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
        "batch_bootstrap",
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
    // Path patch MUST run before Sonar scripts so early module fetches are fixed.
    let inject = format!("{PROXY_PATH_PATCH}{DARK_THEME_INJECT}");

    if let Some(insert_at) = head_content_start(&out) {
        out.insert_str(insert_at, &inject);
    } else if let Some(pos) = out.to_lowercase().find("</head>") {
        out.insert_str(pos, &inject);
    } else {
        out = format!("{inject}{out}");
    }

    out
}

/// Rewrite `Path=/` on Set-Cookie so session cookies stay under the proxy prefix
/// (required for HTTPS tunnels where the browser origin is the tunnel host).
fn rewrite_set_cookie(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    let Some(idx) = lower.find("path=/") else {
        return value.to_string();
    };
    let after = idx + "path=/".len();
    let rest = &value[after..];
    let ends_at_root = rest.is_empty()
        || rest.starts_with(';')
        || rest.starts_with(',')
        || rest.starts_with(' ');
    if !ends_at_root {
        return value.to_string();
    }
    format!(
        "{}Path={}/{}",
        &value[..idx],
        SONAR_UI_PUBLIC_PREFIX.trim_end_matches('/'),
        rest
    )
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
        assert_eq!(
            out,
            format!(r#"fetch("{SONAR_UI_PUBLIC_PREFIX}/api/navigation/navigation")"#)
        );
    }

    #[test]
    fn rewrite_js_api_and_asset_paths() {
        let js = r#"fetch("/api/foo");import("/js/main.js");"/sessions""#;
        let out = rewrite_quoted_root_paths(js, SONAR_UI_PUBLIC_PREFIX);
        assert!(out.contains(&format!("{SONAR_UI_PUBLIC_PREFIX}/js/main.js")));
        assert!(out.contains(&format!(r#"fetch("{SONAR_UI_PUBLIC_PREFIX}/api/foo")"#)));
        // SPA client routes stay unprefixed (React Router basename handles them).
        assert!(out.contains(r#""/sessions""#));
    }

    #[test]
    fn rewrite_set_cookie_root_path() {
        let out = rewrite_set_cookie("JWT-SESSION=abc; Path=/; HttpOnly; SameSite=Lax");
        assert!(out.contains(&format!("Path={SONAR_UI_PUBLIC_PREFIX}/")));
        assert!(out.contains("HttpOnly"));
        let nested = rewrite_set_cookie("X=1; Path=/api/ship/sonar/ui/; Secure");
        assert_eq!(nested, "X=1; Path=/api/ship/sonar/ui/; Secure");
    }

    #[test]
    fn inject_includes_proxy_path_patch() {
        let html = r#"<!doctype html><html><head></head><body></body></html>"#;
        let out = inject_dark_theme_html(html);
        assert!(out.contains("ax-sonar-proxy-path"));
        assert!(out.contains("ax-sonar-theme"));
        let patch_pos = out.find("ax-sonar-proxy-path").unwrap();
        let theme_pos = out.find("ax-sonar-theme").unwrap();
        assert!(patch_pos < theme_pos, "path patch must precede theme script");
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
        assert_eq!(
            out,
            format!(r#"fetch("{SONAR_UI_PUBLIC_PREFIX}/api/navigation/global")"#)
        );
        // Idempotent — already-proxied paths must not double-prefix.
        let again = rewrite_quoted_root_paths(&out, SONAR_UI_PUBLIC_PREFIX);
        assert_eq!(again, out);
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
