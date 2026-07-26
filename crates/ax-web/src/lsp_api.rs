//! LSP enrich HTTP API for Command Center.

use axum::extract::State;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::workspace_state::WebHub;

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/status", get(handle_status))
        .route("/enrich", post(handle_enrich))
        .with_state(hub)
}

async fn handle_status() -> Json<serde_json::Value> {
    let servers = ax_lsp::discover_servers();
    Json(serde_json::json!({ "servers": servers }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EnrichBody {
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    200
}

async fn handle_enrich(
    State(hub): State<WebHub>,
    Json(body): Json<EnrichBody>,
) -> Json<serde_json::Value> {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "read-only" }));
    }
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    let queries = ax_db::queries::QueryBuilder::new(ws.graph_pool.clone());
    drop(ws);

    ax_usage::log_lsp(Some(&root), format!("enrich start limit={}", body.limit));
    crate::actions::publish_for(
        Some(&root),
        "lsp",
        format!("enrich start limit={}", body.limit),
        None,
    );

    match ax_lsp::enrich_project(&root, &queries, body.limit).await {
        Ok(report) => {
            ax_usage::log_lsp(
                Some(&root),
                format!(
                    "enrich examined={} resolved={} no_server={} no_def={} errors={}",
                    report.examined,
                    report.resolved,
                    report.skipped_no_server,
                    report.skipped_no_definition,
                    report.errors.len()
                ),
            );
            crate::actions::publish_for(
                Some(&root),
                "lsp",
                format!("enrich resolved={}", report.resolved),
                Some(serde_json::to_value(&report).unwrap_or_default()),
            );
            Json(serde_json::json!({ "ok": true, "report": report }))
        }
        Err(e) => {
            ax_usage::log_lsp(Some(&root), format!("enrich fail {e}"));
            Json(serde_json::json!({ "ok": false, "error": e.to_string() }))
        }
    }
}
