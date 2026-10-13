//! Project override for the Smart Output context token budget.

use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use serde::Deserialize;

use crate::workspace_state::WebHub;

const MIN_TOKENS: u32 = 1_000;
const MAX_TOKENS: u32 = 64_000;

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/", get(get_budget).put(put_budget))
        .with_state(hub)
}

async fn get_budget(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let root = hub.read().await.project_root.clone();
    let settings = ax_usage::load_settings(Some(&root));
    Json(serde_json::json!({
        "tokens": settings.context_budget_tokens,
        "min": MIN_TOKENS,
        "max": MAX_TOKENS,
    }))
}

#[derive(Deserialize)]
struct PutBudget {
    tokens: u32,
}

async fn put_budget(
    State(hub): State<WebHub>,
    Json(body): Json<PutBudget>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if hub.readonly {
        return Err((StatusCode::FORBIDDEN, "Command Center is read-only".into()));
    }
    if !(MIN_TOKENS..=MAX_TOKENS).contains(&body.tokens) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("tokens must be between {MIN_TOKENS} and {MAX_TOKENS}"),
        ));
    }
    let root = hub.read().await.project_root.clone();
    ax_usage::save_project_context_budget(&root, body.tokens)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err))?;
    Ok(Json(serde_json::json!({ "tokens": body.tokens })))
}
