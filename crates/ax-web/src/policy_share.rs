//! HTTP routes for remote policy share sync and Microsoft auth.

use ax_share::{
    microsoft_auth_status, microsoft_clear_tokens, poll_device_flow_once, project_config_path,
    run_sync, share_config_for_api, share_status_for_api, start_device_flow,
    write_ms_client_id_to_config, write_project_share_config, ShareConfig, ShareSyncStatus,
    SyncDirection,
};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
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

pub fn microsoft_auth_router(hub: WebHub) -> Router {
    Router::new()
        .route("/device/start", post(ms_device_start))
        .route("/device/poll", post(ms_device_poll))
        .route("/status", get(ms_auth_status))
        .route("/config", put(ms_set_config))
        .route("/", delete(ms_sign_out))
        .with_state(hub)
}

/// Microsoft sign-in changes the owner's own account and config file, so
/// only the local owner's browser on a writable server may do it.
fn refuse_ms_change(hub: &WebHub, headers: &HeaderMap) -> Result<(), (StatusCode, Json<ApiError>)> {
    let error = if hub.readonly {
        "Read-only mode"
    } else if !crate::local_owner::browser_serves(headers, hub.port) {
        "Microsoft sign-in can only be changed in the Command Center on this computer"
    } else {
        return Ok(());
    };
    Err((
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: error.into(),
        }),
    ))
}

async fn project_root(hub: &WebHub) -> Result<std::path::PathBuf, (StatusCode, Json<ApiError>)> {
    let guard = hub.read().await;
    Ok(guard.project_root.clone())
}

/// Runs share work that can block outside the async workers. Loading or
/// saving the share config reaches the OS credential store, which may wait
/// on a Keychain prompt or a D-Bus call, and a sync runs git.
async fn off_worker<T, F>(work: F) -> Result<T, (StatusCode, Json<ApiError>)>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(work).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: format!("share task failed: {e}"),
            }),
        )
    })
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
    let config = off_worker({
        let root = root.clone();
        move || share_config_for_api(&root)
    })
    .await?;
    Ok(Json(config_response(&root, config)))
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
    let config = off_worker({
        let root = root.clone();
        move || {
            write_project_share_config(&root, &body)?;
            Ok::<_, String>(share_config_for_api(&root))
        }
    })
    .await?
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError { error: e }),
        )
    })?;
    Ok(Json(config_response(&root, config)))
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
    // A sync loads the token from the credential store and runs git, both
    // blocking, so the whole run happens on a blocking thread.
    let runtime = tokio::runtime::Handle::current();
    let outcome = off_worker(move || {
        runtime
            .block_on(run_sync(&root, &pool, direction))
            .map(|r| r.status)
            .inspect_err(|e| {
                let mut st = share_status_for_api(&root);
                st.last_error = Some(e.clone());
                let _ = ax_share::save_share_status(&root, &st);
            })
    })
    .await?;
    outcome
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiError { error: e })))
}

async fn ms_device_start(
    State(hub): State<WebHub>,
    headers: HeaderMap,
) -> Result<Json<ax_share::DeviceFlowStart>, (StatusCode, Json<ApiError>)> {
    refuse_ms_change(&hub, &headers)?;
    start_device_flow()
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiError { error: e })))
}

async fn ms_device_poll(
    State(hub): State<WebHub>,
    headers: HeaderMap,
) -> Result<Json<DevicePollResponse>, (StatusCode, Json<ApiError>)> {
    refuse_ms_change(&hub, &headers)?;
    match poll_device_flow_once().await {
        Ok(Some(_)) => Ok(Json(DevicePollResponse {
            complete: true,
            status: microsoft_auth_status(),
        })),
        Ok(None) => Ok(Json(DevicePollResponse {
            complete: false,
            status: microsoft_auth_status(),
        })),
        Err(e) => Err((StatusCode::BAD_REQUEST, Json(ApiError { error: e }))),
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
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Json(body): Json<MsClientConfigBody>,
) -> Result<Json<ax_share::MicrosoftAuthStatus>, (StatusCode, Json<ApiError>)> {
    refuse_ms_change(&hub, &headers)?;
    // Sign-in reads AX_MS_CLIENT_ID before the config file, so a saved id
    // would have no effect while the variable is set.
    if std::env::var("AX_MS_CLIENT_ID").is_ok_and(|value| !value.trim().is_empty()) {
        return Err((
            StatusCode::CONFLICT,
            Json(ApiError {
                error: "AX_MS_CLIENT_ID is set in the server's environment and overrides a saved client id; change or unset it and restart the server".into(),
            }),
        ));
    }
    write_ms_client_id_to_config(&body.client_id)
        .map_err(|e| (StatusCode::BAD_REQUEST, Json(ApiError { error: e })))?;
    // The new id is read back from the config file. The process environment
    // is not changed: other request threads read it at the same time.
    Ok(Json(microsoft_auth_status()))
}

async fn ms_sign_out(
    State(hub): State<WebHub>,
    headers: HeaderMap,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    refuse_ms_change(&hub, &headers)?;
    microsoft_clear_tokens().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError { error: e }),
        )
    })?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Work that blocks (a Keychain prompt, a D-Bus call, git) must not hold
    /// the async worker: here the blocked work waits for a task on the same
    /// single-threaded runtime, which only runs if the worker stays free.
    #[tokio::test(flavor = "current_thread")]
    async fn blocking_share_work_leaves_the_async_worker_free() {
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        let other_task = tokio::spawn(async move {
            let _ = tx.send(());
        });
        let blocked = off_worker(move || rx.recv_timeout(Duration::from_secs(2)).is_ok());
        let result = tokio::time::timeout(Duration::from_secs(10), blocked)
            .await
            .expect("the blocking work never returned");
        assert!(
            result.is_ok_and(|ran_alongside| ran_alongside),
            "the blocking work held the async worker"
        );
        other_task.await.unwrap();
    }
}
