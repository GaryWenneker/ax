use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use crate::workspace_state::WebHub;

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/manifests", get(list_manifests))
        .route("/replay", post(start_replay))
        .route("/replay/{id}", get(show_replay))
        .with_state(hub)
}

async fn list_manifests(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let ws = hub.read().await;
    let pool = ws.graph_pool.clone();
    drop(ws);
    let project: String = sqlx::query_scalar(
        "SELECT project_id FROM context_manifests ORDER BY created_at DESC, id LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_else(|| "ax".into());
    match ax_usage::context_authority::list_manifest_summaries(&pool, &project).await {
        Ok(manifests) => Json(serde_json::json!({
            "projectId": project,
            "manifests": manifests,
        })),
        Err(error) => Json(serde_json::json!({
            "projectId": project,
            "manifests": [],
            "error": error,
        })),
    }
}

#[derive(Deserialize)]
struct ReplayBody {
    #[serde(rename = "manifestId")]
    manifest_id: String,
}

async fn start_replay(
    State(hub): State<WebHub>,
    Json(body): Json<ReplayBody>,
) -> Json<serde_json::Value> {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "readonly" }));
    }
    let pool = hub.read().await.graph_pool.clone();
    match ax_usage::context_authority::start_replay_for_manifest(&pool, &body.manifest_id).await {
        Ok(receipt) => Json(serde_json::json!({
            "ok": true,
            "run": receipt,
        })),
        Err(error) => Json(serde_json::json!({ "ok": false, "error": error })),
    }
}

async fn show_replay(State(hub): State<WebHub>, Path(id): Path<String>) -> Json<serde_json::Value> {
    let pool = hub.read().await.graph_pool.clone();
    match ax_usage::context_authority::load_replay_run(&pool, &id).await {
        Ok(run) => Json(serde_json::json!({ "ok": true, "run": run })),
        Err(error) => Json(serde_json::json!({ "ok": false, "error": error })),
    }
}
