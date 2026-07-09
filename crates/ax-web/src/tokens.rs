//! Token usage API (`~/.ax/usage.db`).

use axum::{
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use ax_usage::{query_summary, UsagePeriod, UsageQuery};

#[derive(Deserialize)]
pub struct TokensQuery {
    #[serde(default = "default_period")]
    period: String,
    from: Option<String>,
    to: Option<String>,
}

fn default_period() -> String {
    "month_to_date".into()
}

async fn handle_tokens(Query(q): Query<TokensQuery>) -> impl IntoResponse {
    let period = UsagePeriod::parse(&q.period).unwrap_or(UsagePeriod::MonthToDate);
    if period == UsagePeriod::Custom && q.from.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "custom period requires from=YYYY-MM-DD" })),
        )
            .into_response();
    }
    match query_summary(&UsageQuery {
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

pub fn router() -> Router {
    Router::new().route("/tokens", get(handle_tokens))
}
