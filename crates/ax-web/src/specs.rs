//! Command Center list of versioned specs.

use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;

use crate::workspace_state::WebHub;

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/", get(list))
        .route("/{id}/revisions", get(revisions).post(save))
        .route("/{id}/approve", post(approve))
        .with_state(hub)
}

async fn list(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    let pool = ws.graph_pool.clone();
    drop(ws);
    if let Err(error) = ax_usage::import_workspace_plans(&pool, &root, "local").await {
        return Json(serde_json::json!({ "specs": [], "error": error }));
    }
    match ax_usage::list_specs(&pool).await {
        Ok(items) => Json(serde_json::json!({
            "specs": items.iter().map(|item| serde_json::json!({
                "id": item.id,
                "projectId": item.project_id,
                "title": item.title,
                "path": item.markdown_path,
                "sessions": item.sessions,
                "approvedVersion": item.approved_version,
                "latestStatus": item.latest_status,
                "revisionCount": item.revision_count,
                "kind": item.kind,
                "pointerVersion": item.pointer_version,
                "pointerHash": item.pointer_hash,
            })).collect::<Vec<_>>()
        })),
        Err(error) => Json(serde_json::json!({ "specs": [], "error": error })),
    }
}

async fn revisions(State(hub): State<WebHub>, Path(id): Path<String>) -> Json<serde_json::Value> {
    let pool = hub.read().await.graph_pool.clone();
    match ax_usage::list_revisions(&pool, &id).await {
        Ok(rows) => Json(serde_json::json!({
            "revisions": rows.iter().map(|row| serde_json::json!({
                "version": row.version,
                "status": row.status,
                "source": row.source,
                "body": row.body,
                "hash": row.content_hash,
                "basedOn": row.based_on,
                "createdAt": row.created_at,
            })).collect::<Vec<_>>()
        })),
        Err(error) => Json(serde_json::json!({ "revisions": [], "error": error })),
    }
}

#[derive(Deserialize)]
struct SaveBody {
    #[serde(rename = "basedOn")]
    based_on: i64,
    body: String,
}

async fn save(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Json(body): Json<SaveBody>,
) -> Json<serde_json::Value> {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "readonly" }));
    }
    let pool = hub.read().await.graph_pool.clone();
    match ax_usage::save_revision(&pool, &id, body.based_on, &body.body).await {
        Ok(revision) => Json(serde_json::json!({
            "ok": true,
            "version": revision.version,
            "status": revision.status,
            "body": revision.body,
            "hash": revision.content_hash,
        })),
        Err(error) => Json(serde_json::json!({ "ok": false, "error": error })),
    }
}

#[derive(Deserialize)]
struct ApproveBody {
    version: i64,
}

async fn approve(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Json(body): Json<ApproveBody>,
) -> Json<serde_json::Value> {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "readonly" }));
    }
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    let pool = ws.graph_pool.clone();
    drop(ws);
    match ax_usage::approve_revision(&pool, &root, &id, body.version, &[]).await {
        Ok(revision) => Json(serde_json::json!({
            "ok": true,
            "version": revision.version,
            "status": revision.status,
        })),
        Err(error) => Json(serde_json::json!({ "ok": false, "error": error })),
    }
}
