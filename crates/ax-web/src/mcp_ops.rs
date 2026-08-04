//! MCP daemon health + reload for Command Center (mobile menu / ops API).

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use ax_mcp::daemon::{read_daemon_info, try_connect, DAEMON_INFO_FILE};
use ax_mcp::daemon_lock::is_pid_alive;
use ax_mcp::restart_daemon;

use crate::workspace_state::WebHub;

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/mcp-health", get(handle_health))
        .route("/mcp-reload", post(handle_reload))
        .with_state(hub)
}

async fn handle_health(State(hub): State<WebHub>) -> impl IntoResponse {
    let root = {
        let ws = hub.read().await;
        ws.project_root.clone()
    };
    let info = read_daemon_info(&root);
    let alive = info.as_ref().map(|i| is_pid_alive(i.pid)).unwrap_or(false);
    let connected = try_connect(&root).await.is_some();
    let ax_dir = root.join(".ax");
    let lock_path = ax_dir.join("ax.lock");
    let daemon_info_path = ax_dir.join(DAEMON_INFO_FILE);
    Json(json!({
        "ok": connected,
        "projectRoot": root.to_string_lossy(),
        "daemon": info.as_ref().map(|i| json!({
            "pid": i.pid,
            "port": i.port,
            "socketPath": i.socket_path,
            "version": i.version,
            "alive": alive,
        })),
        "connected": connected,
        "axLockPresent": lock_path.exists(),
        "daemonInfoPath": daemon_info_path.to_string_lossy(),
        "hint": if connected {
            "Shared MCP daemon is reachable."
        } else if info.is_some() && !alive {
            "Daemon metadata is stale — use Reload MCP."
        } else {
            "No shared daemon — Cursor/Takumi may each embed a full MCP process and contend on ax.db."
        },
    }))
}

async fn handle_reload(State(hub): State<WebHub>) -> impl IntoResponse {
    let root = {
        let ws = hub.read().await;
        ws.project_root.clone()
    };
    match restart_daemon(&root).await {
        Ok(report) => (
            StatusCode::OK,
            Json(json!({
                "ok": report.ok,
                "stoppedPid": report.stopped_pid,
                "startedPid": report.started_pid,
                "clearedAxLock": report.cleared_ax_lock,
                "connected": report.connected,
                "hint": report.hint,
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "ok": false,
                "error": e,
                "hint": "Try `ax daemon restart` in a terminal, then MCP: Restart Servers in the IDE.",
            })),
        )
            .into_response(),
    }
}
