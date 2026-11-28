//! List discovered extractor plugins and whether each is trusted to run.

use axum::extract::State;
use axum::routing::get;
use axum::{Json, Router};

use crate::workspace_state::WebHub;

pub fn router_hub(hub: WebHub) -> Router {
    Router::new().route("/", get(handle_list)).with_state(hub)
}

async fn handle_list(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let root = hub.read().await.project_root.clone();
    let plugins: Vec<_> = ax_plugins::discover_plugins(&root)
        .into_iter()
        .map(|p| {
            let trusted = ax_plugins::is_trusted(&p);
            let m = p.manifest;
            serde_json::json!({
                "name": m.name,
                "extensions": m.extensions,
                "mode": if m.wasm.is_some() { "wasm" } else { "process" },
                "command": m.command,
                "wasm": m.wasm,
                "trusted": trusted,
            })
        })
        .collect();
    Json(serde_json::json!({
        "count": plugins.len(),
        "plugins": plugins,
        "pluginsDir": root.join(".ax").join("plugins").to_string_lossy(),
    }))
}
