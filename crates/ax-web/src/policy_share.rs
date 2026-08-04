//! HTTP routes for remote policy share sync and Microsoft auth.

use ax_share::{
    microsoft_auth_status, microsoft_clear_tokens, poll_device_flow_once, project_config_path,
    run_sync, share_config_for_api, share_status_for_api, start_device_flow,
    write_ms_client_id_to_config, write_project_share_config, ShareConfig, ShareSyncStatus,
    SyncDirection,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::workspace_state::WebHub;

#[derive(Serialize)]
struct ApiError {
    error: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareConfigResponse {
    #[serde(flatten)]
    config: ShareConfig,
    config_path: String,
    scope: &'static str,
}

pub fn policy_share_router(hub: WebHub) -> Router {
    Router::new()
        .route("/config", get(get_share_config).put(put_share_config))
        .route("/status", get(get_share_status))
        .route("/sync", post(post_share_sync))
        .with_state(hub)
}

pub fn microsoft_auth_router() -> Router {
    Router::new()
        .route("/device/start", post(ms_device_start))
        .route("/device/poll", post(ms_device_poll))
        .route("/status", get(ms_auth_status))
        .route("/config", put(ms_set_config))
        .route("/", delete(ms_sign_out))
}

async fn project_root(hub: &WebHub) -> Result<std::path::PathBuf, (StatusCode, Json<ApiError>)> {
    let guard = hub.read().await;
    Ok(guard.project_root.clone())
}

fn config_response(root: &std::path::Path, config: ShareConfig) -> ShareConfigResponse {
    ShareConfigResponse {
        config,
        config_path: project_config_path(root).display().to_string(),
        scope: "project",
    }
}

async fn get_share_config(
    State(hub): State<WebHub>,
) -> Result<Json<ShareConfigResponse>, (StatusCode, Json<ApiError>)> {
    let root = project_root(&hub).await?;
    Ok(Json(config_response(
        &root,
        share_config_for_api(&root),
    )))
}

async fn put_share_config(
    State(hub): State<WebHub>,
    Json(body): Json<ShareConfig>,
) -> Result<Json<ShareConfigResponse>, (StatusCode, Json<ApiError>)> {
    if hub.readonly {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "Read-only mode".into(),
            }),
        ));
    }
    let root = project_root(&hub).await?;
    write_project_share_config(&root, &body).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError { error: e }),
        )
    })?;
    Ok(Json(config_response(
        &root,
        share_config_for_api(&root),
    )))
}

async fn get_share_status(
    State(hub): State<WebHub>,
) -> Result<Json<ShareStatusResponse>, (StatusCode, Json<ApiError>)> {
    let root = project_root(&hub).await?;
    Ok(Json(ShareStatusResponse {
        sync: share_status_for_api(&root),
        microsoft: microsoft_auth_status(),
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareStatusResponse {
    sync: ShareSyncStatus,
    microsoft: ax_share::MicrosoftAuthStatus,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncBody {
    #[serde(default = "default_direction")]
    direction: String,
}

fn default_direction() -> String {
    "pull".into()
}

async fn post_share_sync(
    State(hub): State<WebHub>,
    Json(body): Json<SyncBody>,
) -> Result<Json<ShareSyncStatus>, (StatusCode, Json<ApiError>)> {
    if hub.readonly {
        return Err((
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "Read-only mode".into(),
            }),
        ));
    }
    let root = project_root(&hub).await?;
    let pool = hub.read().await.policy.store.pool().clone();
    let direction = match body.direction.as_str() {
        "push" => SyncDirection::Push,
        "both" => SyncDirection::Both,
        _ => SyncDirection::Pull,
    };
    match run_sync(&root, &pool, direction).await {
        Ok(r) => Ok(Json(r.status)),
        Err(e) => {
            let mut st = share_status_for_api(&root);
            st.last_error = Some(e.clone());
            let _ = ax_share::save_share_status(&root, &st);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError { error: e }),
            ))
        }
    }
}

async fn ms_device_start() -> Result<Json<ax_share::DeviceFlowStart>, (StatusCode, Json<ApiError>)> {
    start_device_flow().await.map(Json).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError { error: e }),
        )
    })
}

async fn ms_device_poll() -> Result<Json<DevicePollResponse>, (StatusCode, Json<ApiError>)> {
    match poll_device_flow_once().await {
        Ok(Some(_)) => Ok(Json(DevicePollResponse {
            complete: true,
            status: microsoft_auth_status(),
        })),
        Ok(None) => Ok(Json(DevicePollResponse {
            complete: false,
            status: microsoft_auth_status(),
        })),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError { error: e }),
        )),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DevicePollResponse {
    complete: bool,
    status: ax_share::MicrosoftAuthStatus,
}

async fn ms_auth_status() -> Json<ax_share::MicrosoftAuthStatus> {
    Json(microsoft_auth_status())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MsClientConfigBody {
    client_id: String,
}

async fn ms_set_config(
    Json(body): Json<MsClientConfigBody>,
) -> Result<Json<ax_share::MicrosoftAuthStatus>, (StatusCode, Json<ApiError>)> {
    write_ms_client_id_to_config(&body.client_id).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            Json(ApiError { error: e }),
        )
    })?;
    std::env::set_var("AX_MS_CLIENT_ID", body.client_id.trim());
    Ok(Json(microsoft_auth_status()))
}

async fn ms_sign_out() -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    microsoft_clear_tokens().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError { error: e }),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}
