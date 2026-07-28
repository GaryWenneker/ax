//! Model pricing API (`~/.ax/usage.db` snapshots).

use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use ax_usage::{
    list_coding_agents, list_latest_prices, price_history, pricing_status, sync_pricing,
};

#[derive(Deserialize)]
pub struct ListQuery {
    source: Option<String>,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    model: String,
    source: Option<String>,
    #[serde(default = "default_days")]
    days: i64,
}

#[derive(Deserialize)]
pub struct AgentsQuery {
    #[serde(default = "default_days")]
    days: i64,
}

fn default_days() -> i64 {
    30
}

async fn handle_catalog(Query(q): Query<ListQuery>) -> impl IntoResponse {
    let status = match pricing_status().await {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e })),
            )
                .into_response();
        }
    };
    match list_latest_prices(q.source.as_deref()).await {
        Ok(models) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": status,
                "models": models,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn handle_status() -> impl IntoResponse {
    match pricing_status().await {
        Ok(s) => (StatusCode::OK, Json(s)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn handle_history(Query(q): Query<HistoryQuery>) -> impl IntoResponse {
    if q.model.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "model is required" })),
        )
            .into_response();
    }
    match price_history(&q.model, q.source.as_deref(), q.days).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

async fn handle_agents(Query(q): Query<AgentsQuery>) -> impl IntoResponse {
    match list_coding_agents(q.days).await {
        Ok(rows) => (StatusCode::OK, Json(rows)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct SyncBody {
    #[serde(default)]
    force: bool,
}

async fn handle_sync(body: Option<Json<SyncBody>>) -> impl IntoResponse {
    let force = body.map(|b| b.force).unwrap_or(true);
    match sync_pricing(force).await {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

pub fn router() -> Router {
    Router::new()
        .route("/pricing", get(handle_catalog))
        .route("/pricing/status", get(handle_status))
        .route("/pricing/history", get(handle_history))
        .route("/pricing/agents", get(handle_agents))
        .route("/pricing/sync", post(handle_sync))
}
