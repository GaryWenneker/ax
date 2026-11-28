//! Context savings API (`~/.ax/usage.db`).

use axum::{
    extract::{DefaultBodyLimit, Path, Query},
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
/// Answer for a write refused because the server is read-only.
pub(crate) fn readonly_refusal() -> axum::response::Response {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "error": "AX_WEB_READONLY=1" })),
    )
        .into_response()
}

async fn handle_import(readonly: bool) -> axum::response::Response {
    if readonly {
        return readonly_refusal();
    }
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

/// Largest `/tokenize` request body. The tokenizer reads at most
/// [`ax_usage::TOKENIZE_MAX_INPUT_BYTES`] of the text; this leaves room for
/// longer text (answered as truncated) without parsing arbitrary bodies.
const TOKENIZE_MAX_BODY_BYTES: usize = 512 * 1024;

async fn handle_tokenize(Json(body): Json<TokenizeBody>) -> axum::response::Response {
    match tokio::task::spawn_blocking(move || tokenize_text(&body.text)).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "tokenize failed" })),
        )
            .into_response(),
    }
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
    let readonly = hub.readonly;
    Router::new()
        .route("/savings", get(handle_savings))
        .route("/savings/import", post(move || handle_import(readonly)))
        .route("/savings/call/{id}", get(handle_call_detail))
        .route(
            "/tokenize",
            post(handle_tokenize).layer(DefaultBodyLimit::max(TOKENIZE_MAX_BODY_BYTES)),
        )
        .merge(crate::pricing_api::router(readonly))
        .merge(crate::mcp_trace::router(hub.clone()))
        .merge(crate::mcp_quality::router(hub))
}
