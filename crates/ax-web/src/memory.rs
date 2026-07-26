//! Memory vault HTTP API for the Command Center.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Deserialize;

use crate::workspace_state::WebHub;
use crate::ApiError;

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/", get(handle_list).post(handle_create))
        .route("/recall", get(handle_recall))
        .route("/embed-status", get(handle_embed_status))
        .route("/capture-git", post(handle_capture_git))
        .route("/{id}", get(handle_get).put(handle_update).delete(handle_delete))
        .with_state(hub)
}

fn err(status: StatusCode, msg: impl Into<String>) -> axum::response::Response {
    (status, Json(ApiError { error: msg.into() })).into_response()
}

fn forbidden_readonly() -> axum::response::Response {
    ax_usage::log_share(None, "readonly write denied");
    err(StatusCode::FORBIDDEN, "read-only mode (AX_WEB_READONLY=1)")
}

async fn handle_embed_status() -> Json<serde_json::Value> {
    let backend = if ax_memory::onnx::onnx_available() {
        "onnx"
    } else if ax_memory::onnx::onnx_model_configured() {
        "onnx_unconfigured"
    } else {
        "hash"
    };
    let tokenizer = ax_memory::onnx::onnx_tokenizer_configured();
    let feature = ax_memory::onnx::onnx_feature_enabled();
    let model_path = ax_memory::onnx::onnx_model_path()
        .map(|p| p.to_string_lossy().into_owned());
    let tokenizer_path = ax_memory::onnx::onnx_tokenizer_path()
        .map(|p| p.to_string_lossy().into_owned());
    static LOGGED: std::sync::Once = std::sync::Once::new();
    LOGGED.call_once(|| {
        ax_usage::log_embed(
            None,
            format!("backend={backend} tokenizer={tokenizer} feature={feature}"),
        );
    });
    Json(serde_json::json!({
        "backend": backend,
        "tokenizer": tokenizer,
        "feature": feature,
        "modelPath": model_path,
        "tokenizerPath": tokenizer_path,
    }))
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
}

fn default_limit() -> usize {
    50
}

async fn handle_list(State(hub): State<WebHub>, Query(p): Query<ListQuery>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ax_memory::list(&ws.graph_pool, p.limit.min(500), p.offset).await {
        Ok((memories, total)) => {
            (StatusCode::OK, Json(serde_json::json!({ "memories": memories, "total": total }))).into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct RecallQuery {
    q: String,
    #[serde(default = "default_recall_limit")]
    limit: usize,
}

fn default_recall_limit() -> usize {
    10
}

async fn handle_recall(State(hub): State<WebHub>, Query(p): Query<RecallQuery>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ax_memory::recall(&ws.graph_pool, &p.q, p.limit.min(50)).await {
        Ok(matches) => (StatusCode::OK, Json(serde_json::json!({ "matches": matches }))).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct MemoryInput {
    #[serde(default)]
    title: String,
    body: String,
    kind: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    files: Vec<String>,
}

impl MemoryInput {
    fn into_remember(self) -> ax_memory::RememberInput {
        ax_memory::RememberInput {
            title: self.title,
            body: self.body,
            kind: self.kind,
            tags: self.tags,
            files: self.files,
            source: Some("manual".into()),
        }
    }
}

async fn handle_create(State(hub): State<WebHub>, Json(input): Json<MemoryInput>) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    match ax_memory::remember(&ws.graph_pool, input.into_remember()).await {
        Ok(row) => {
            let similar = ax_memory::find_similar(
                &ws.graph_pool,
                &format!("{} {}", row.title, row.body),
                Some(&row.id),
                0.80,
                3,
            )
            .await
            .unwrap_or_default();
            (StatusCode::OK, Json(serde_json::json!({ "memory": row, "similar": similar }))).into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn handle_get(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ax_memory::get(&ws.graph_pool, &id).await {
        Ok(Some(row)) => (StatusCode::OK, Json(row)).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "memory not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn handle_update(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Json(input): Json<MemoryInput>,
) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    match ax_memory::update(&ws.graph_pool, &id, input.into_remember()).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "memory not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn handle_delete(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    match ax_memory::delete(&ws.graph_pool, &id).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "memory not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct CaptureGitInput {
    #[serde(default = "default_capture_limit")]
    limit: usize,
}

fn default_capture_limit() -> usize {
    100
}

async fn handle_capture_git(
    State(hub): State<WebHub>,
    Json(input): Json<CaptureGitInput>,
) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    match ax_memory::capture_git_history(&ws.graph_pool, &ws.project_root, input.limit).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
