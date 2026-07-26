//! Share status + themed token gate helpers.

use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};

use crate::workspace_state::WebHub;

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/status", get(handle_status))
        .with_state(hub)
}

async fn handle_status(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let sharing = std::env::var("AX_SHARE_TOKEN")
        .ok()
        .filter(|t| !t.is_empty())
        .is_some();
    Json(serde_json::json!({
        "sharing": sharing,
        "readonly": hub.readonly || sharing,
        "port": hub.port,
    }))
}

/// Themed HTML gate when share token is missing/invalid.
pub fn unauthorized_gate_html() -> Response {
    let body = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>ax — share access</title>
<style>
  :root { color-scheme: dark; --bg:#1e1e1e; --fg:#cccccc; --muted:#9d9d9d; --accent:#3ee4b2; --input:#313131; --border:#2b2b2b;
    --accent-bg: color-mix(in srgb, var(--accent) 10%, transparent);
    --accent-border: color-mix(in srgb, var(--accent) 45%, transparent); }
  * { box-sizing: border-box; }
  body { margin:0; min-height:100vh; display:flex; align-items:center; justify-content:center;
    font-family: "Segoe UI", system-ui, sans-serif; background:var(--bg); color:var(--fg); }
  .gate { width:min(420px, 92vw); border:1px solid var(--accent-border); border-radius:10px; padding:28px 24px;
    background: linear-gradient(145deg, var(--accent-bg), transparent 70%);
    box-shadow: 0 24px 64px rgba(0,0,0,0.45); }
  h1 { margin:0 0 8px; font-size:1.25rem; font-weight:650; letter-spacing:0.02em; color:#fff; }
  .brand { color:var(--accent); font-weight:700; }
  p { margin:0 0 16px; color:var(--muted); font-size:0.9rem; line-height:1.45; }
  label { display:block; font-size:0.75rem; margin-bottom:6px; color:var(--accent); text-transform:uppercase; letter-spacing:0.06em; font-weight:600; }
  input { width:100%; padding:10px 12px; border:1px solid var(--border); border-radius:6px; background:var(--input);
    color:var(--fg); font:inherit; margin-bottom:14px; outline:none; }
  input:focus { border-color:var(--accent); box-shadow:0 0 0 1px color-mix(in srgb, var(--accent) 20%, transparent); }
  button { width:100%; padding:10px 12px; border:1px solid var(--accent); border-radius:6px;
    background: color-mix(in srgb, var(--accent) 12%, transparent); color:var(--accent);
    font:inherit; font-weight:600; cursor:pointer; }
  button:hover { background: color-mix(in srgb, var(--accent) 20%, transparent); }
  code { font-family: ui-monospace, Consolas, monospace; font-size:0.85em; }
</style>
</head>
<body>
  <form class="gate" method="get" action="/">
    <h1><span class="brand">ax</span> share access</h1>
    <p>This Command Center session requires a share token. Paste the token from <code>ax share</code>.</p>
    <label for="token">Share token</label>
    <input id="token" name="token" type="password" autocomplete="off" autofocus required />
    <button type="submit">Open Command Center</button>
  </form>
</body>
</html>"#;
    (
        StatusCode::UNAUTHORIZED,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        Html(body),
    )
        .into_response()
}
