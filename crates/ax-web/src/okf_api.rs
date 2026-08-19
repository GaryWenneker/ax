//! Open Knowledge Format (OKF) export API for the Command Center.

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use ax_core::{
    export_okf_bundle, publish_okf_wiki, validate_okf_bundle, OkfConfig, OkfExportOptions,
    OkfPublishOptions,
};
use ax_db::queries::QueryBuilder;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::workspace_state::WebHub;
use crate::ApiError;

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/config", get(handle_config))
        .route("/export", post(handle_export))
        .route("/validate", post(handle_validate))
        .route("/publish", post(handle_publish))
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OkfConfigResponse {
    enabled: bool,
    out_dir: String,
    out_dir_abs: String,
    kinds: Vec<String>,
    auto_export_on_sync: bool,
    wiki_enabled: bool,
    wiki_remote_configured: bool,
    wiki_subdir: String,
    bundle_exists: bool,
    format: &'static str,
}

async fn handle_config(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    let cfg = OkfConfig::load(&ws.project_root);
    let out_abs = cfg.out_dir_abs(&ws.project_root);
    let bundle_exists = out_abs.join("index.md").is_file();
    (
        StatusCode::OK,
        Json(OkfConfigResponse {
            enabled: cfg.enabled,
            out_dir: cfg.out_dir.clone(),
            out_dir_abs: out_abs.display().to_string(),
            kinds: cfg.kinds.clone(),
            auto_export_on_sync: cfg.auto_export_on_sync,
            wiki_enabled: cfg.azdo_wiki.enabled,
            wiki_remote_configured: !cfg.azdo_wiki.remote.trim().is_empty(),
            wiki_subdir: cfg.azdo_wiki.subdir.clone(),
            bundle_exists,
            format: "Open Knowledge Format (OKF)",
        }),
    )
        .into_response()
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ExportInput {
    #[serde(default)]
    limit: Option<usize>,
}

async fn handle_export(
    State(hub): State<WebHub>,
    Json(input): Json<ExportInput>,
) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    let qb = QueryBuilder::new(ws.graph_pool.clone());
    drop(ws);

    let nodes = match qb.get_all_nodes().await {
        Ok(n) => n,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let edges = match qb.get_all_edges().await {
        Ok(e) => e,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    match export_okf_bundle(
        &root,
        &nodes,
        &edges,
        &OkfExportOptions {
            out: None,
            limit: input.limit.unwrap_or(0),
        },
    ) {
        Ok(report) => (
            StatusCode::OK,
            Json(json!({
                "exported": report.exported,
                "outDir": report.out_dir.display().to_string(),
                "byKind": report.by_kind,
                "format": "Open Knowledge Format (OKF)",
            })),
        )
            .into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}

async fn handle_validate(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    let cfg = OkfConfig::load(&ws.project_root);
    let out_dir = cfg.out_dir_abs(&ws.project_root);
    drop(ws);

    match validate_okf_bundle(&out_dir) {
        Ok(report) => (
            StatusCode::OK,
            Json(json!({
                "ok": report.ok,
                "missingIndex": report.missing_index,
                "pages": report.pages,
                "danglingLinks": report.dangling_links,
                "outDir": out_dir.display().to_string(),
            })),
        )
            .into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PublishInput {
    #[serde(default)]
    dry_run: bool,
    #[serde(default)]
    no_push: bool,
}

async fn handle_publish(
    State(hub): State<WebHub>,
    Json(input): Json<PublishInput>,
) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    let cfg = OkfConfig::load(&root);
    let bundle_dir = cfg.out_dir_abs(&root);
    drop(ws);

    if !cfg.azdo_wiki.enabled {
        return err(
            StatusCode::BAD_REQUEST,
            "OKF wiki publish is disabled. Set ax.json okf.azdoWiki.enabled=true",
        );
    }
    if cfg.azdo_wiki.remote.trim().is_empty() {
        return err(
            StatusCode::BAD_REQUEST,
            "okf.azdoWiki.remote is empty — set a git URL for the wiki remote",
        );
    }

    match publish_okf_wiki(
        &root,
        &bundle_dir,
        &OkfPublishOptions {
            dry_run: input.dry_run,
            no_push: input.no_push,
        },
    ) {
        Ok(report) => (
            StatusCode::OK,
            Json(json!({
                "wikiAction": report.wiki_action,
                "subdir": report.subdir.display().to_string(),
                "filesCopied": report.files_copied,
                "committed": report.committed,
                "pushed": report.pushed,
                "dryRun": report.dry_run,
            })),
        )
            .into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_input_defaults_limit() {
        let v: ExportInput = serde_json::from_str("{}").unwrap();
        assert!(v.limit.is_none());
    }

    #[test]
    fn publish_input_defaults() {
        let v: PublishInput = serde_json::from_str("{}").unwrap();
        assert!(!v.dry_run);
        assert!(!v.no_push);
    }
}
