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
    (status, Json(ApiError { error: msg.into() })).into_response()
}

fn forbidden_readonly() -> axum::response::Response {
    ax_usage::log_share(None, "readonly write denied");
    err(StatusCode::FORBIDDEN, "read-only mode (AX_WEB_READONLY=1)")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncInput {
    #[serde(default)]
    skip_wiki_pull: bool,
    #[serde(default)]
    dry_run: bool,
}

async fn handle_sync(State(hub): State<WebHub>, Json(input): Json<SyncInput>) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    // Copy the root out and release the workspace lock: a sync can take long,
    // and a queued workspace switch would otherwise stall every reader.
    let root = hub.read().await.project_root.clone();
    let opts = SyncOptions {
        skip_wiki_pull: input.skip_wiki_pull,
        dry_run: input.dry_run,
    };
    // The sync runs git and file scans synchronously inside its future, so
    // drive it from the blocking pool instead of an async worker.
    let runtime = tokio::runtime::Handle::current();
    let synced = tokio::task::spawn_blocking(move || {
        runtime
            .block_on(sync_catalog(&root, opts, None))
            .map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match synced {
        Ok(report) => (StatusCode::OK, Json(report)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}
