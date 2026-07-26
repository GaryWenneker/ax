//! List discovered extractor plugins.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::workspace_state::WebHub;

pub fn router_hub(hub: WebHub) -> Router {
    Router::new().route("/", get(handle_list)).with_state(hub)
}

async fn handle_list(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let root = hub.read().await.project_root.clone();
    let host = ax_plugins::load_plugins(&root);
    let plugins: Vec<_> = host
        .manifests()
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "name": m.name,
                "extensions": m.extensions,
                "mode": if m.wasm.is_some() { "wasm" } else { "process" },
                "command": m.command,
                "wasm": m.wasm,
            })
        })
        .collect();
    Json(serde_json::json!({
        "count": plugins.len(),
        "plugins": plugins,
        "pluginsDir": root.join(".ax").join("plugins").to_string_lossy(),
    }))
}
