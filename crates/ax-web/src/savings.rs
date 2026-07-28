//! Context savings API (`~/.ax/usage.db`).

use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use ax_usage::{
    import_agent_logs, query_call_token_detail, query_savings_summary, tokenize_text, SavingsQuery,
    UsagePeriod,
};

#[derive(Deserialize)]
pub struct SavingsQueryParams {
    #[serde(default = "default_period")]
    period: String,
    from: Option<String>,
    to: Option<String>,
}

fn default_period() -> String {
    "month_to_date".into()
}

async fn handle_savings(Query(q): Query<SavingsQueryParams>) -> impl IntoResponse {
    let period = UsagePeriod::parse(&q.period).unwrap_or(UsagePeriod::MonthToDate);
    if period == UsagePeriod::Custom && q.from.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "custom period requires from=YYYY-MM-DD" })),
        )
            .into_response();
    }
    match query_savings_summary(&SavingsQuery {
        period,
        from: q.from,
        to: q.to,
    })
    .await
    {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

/// Import Cursor + Claude Code session logs into `~/.ax/usage.db`.
async fn handle_import() -> impl IntoResponse {
    match import_agent_logs(true, true).await {
        Ok(result) => (StatusCode::OK, Json(serde_json::json!(result))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
struct TokenizeBody {
    text: String,
}

async fn handle_tokenize(Json(body): Json<TokenizeBody>) -> impl IntoResponse {
    let result = tokenize_text(&body.text);
    (StatusCode::OK, Json(result)).into_response()
}

async fn handle_call_detail(Path(id): Path<i64>) -> impl IntoResponse {
    match query_call_token_detail(id).await {
        Ok(detail) => (StatusCode::OK, Json(detail)).into_response(),
        Err(e) if e.contains("not found") => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e })),
        )
            .into_response(),
    }
}

pub fn router(hub: crate::workspace_state::WebHub) -> Router {
    Router::new()
        .route("/savings", get(handle_savings))
        .route("/savings/import", post(handle_import))
        .route("/savings/call/{id}", get(handle_call_detail))
        .route("/tokenize", post(handle_tokenize))
        .merge(crate::pricing_api::router())
        .merge(crate::mcp_trace::router(hub.clone()))
        .merge(crate::mcp_quality::router(hub))
}
