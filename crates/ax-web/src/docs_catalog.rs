//! Documentation catalog sync API for the Command Center.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::post,
    Router,
};
use serde::Deserialize;

use ax_docs_catalog::{sync_catalog, SyncOptions};

use crate::workspace_state::WebHub;
use crate::ApiError;

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/sync", post(handle_sync))
        .with_state(hub)
}

fn err(status: StatusCode, msg: impl Into<String>) -> axum::response::Response {
    (
        status,
        Json(ApiError {
            error: msg.into(),
        }),
    )
        .into_response()
}

fn forbidden_readonly() -> axum::response::Response {
    ax_usage::log_share(None, "readonly write denied");
    err(
        StatusCode::FORBIDDEN,
        "read-only mode (AX_WEB_READONLY=1)",
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncInput {
    #[serde(default)]
    skip_wiki_pull: bool,
    #[serde(default)]
    dry_run: bool,
}

async fn handle_sync(
    State(hub): State<WebHub>,
    Json(input): Json<SyncInput>,
) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    match sync_catalog(
        &ws.project_root,
        SyncOptions {
            skip_wiki_pull: input.skip_wiki_pull,
            dry_run: input.dry_run,
        },
        None,
    )
    .await
    {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}
