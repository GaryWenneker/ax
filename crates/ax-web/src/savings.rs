//! Context savings API (`~/.ax/usage.db`).

use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use ax_usage::{import_agent_logs, query_savings_summary, SavingsQuery, UsagePeriod};

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

pub fn router() -> Router {
    Router::new()
        .route("/savings", get(handle_savings))
        .route("/savings/import", post(handle_import))
}
