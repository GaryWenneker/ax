//! Ship Command Center API (SSE + status + settings).

use std::convert::Infallible;
use std::sync::Arc;

use ax_quality::{
    bootstrap_sonar, discover_sonar, ensure_sonar_live_with_log, inspect_setup,
    start_sonar_container_with_log, stop_sonar_container_with_log, InstallLog, SonarBootstrapConfig,
};
use ax_remote::ShipConfig;
use ax_ship::{ShipDaemon, ShipEvent, ShipReport};
use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Json,
    },
    routing::{get, post},
    Router,
};
use futures_util::stream::Stream;
use serde::Deserialize;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct ShipApiState {
    pub daemon: Arc<ShipDaemon>,
    pub report: Arc<Mutex<Option<ShipReport>>>,
    pub readonly: bool,
}

pub fn router(state: ShipApiState) -> Router {
    Router::new()
        .route("/events", get(handle_ship_events))
        .route("/status", get(handle_ship_status))
        .route("/impact", get(handle_ship_impact))
        .route("/command", post(handle_ship_command))
        .route("/config", get(handle_get_config).put(handle_put_config))
        .route("/sonar/discover", get(handle_sonar_discover))
        .route("/sonar/install", post(handle_sonar_install))
        .route("/sonar/install/stream", post(handle_sonar_install_stream))
        .route("/sonar/start", post(handle_sonar_start))
        .route("/sonar/start/stream", post(handle_sonar_start_stream))
        .route("/sonar/stop", post(handle_sonar_stop))
        .route("/sonar/stop/stream", post(handle_sonar_stop_stream))
        .route("/sonar/bootstrap", post(handle_sonar_bootstrap))
        .route("/sonar/setup", get(handle_sonar_setup))
        .with_state(state)
}

fn readonly_err() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "error": "read-only mode (AX_WEB_READONLY=1)" })),
    )
}

async fn handle_ship_status(State(s): State<ShipApiState>) -> impl IntoResponse {
    let report = s.report.lock().await.clone();
    let config = s.daemon.config().await;
    Json(serde_json::json!({
        "branch": ax_git::current_branch(&s.daemon.project_root).ok().flatten(),
        "report": report,
        "config": config,
    }))
}

async fn handle_ship_impact(State(s): State<ShipApiState>) -> impl IntoResponse {
    let report = s.report.lock().await.clone();
    Json(serde_json::json!({ "report": report }))
}

async fn handle_ship_events(
    State(s): State<ShipApiState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut rx = s.daemon.bus.subscribe();
    let report_store = s.report.clone();
    let stream = async_stream::stream! {
        if let Ok(report) = s.daemon.evaluate().await {
            *report_store.lock().await = Some(report.clone());
            yield Ok(Event::default().data(serde_json::to_string(&ShipEvent::ReportUpdated { report: report.clone() }).unwrap_or_default()));
        }
        loop {
            match rx.recv().await {
                Ok(ev) => {
                    if let ShipEvent::ReportUpdated { ref report } = ev {
                        *report_store.lock().await = Some(report.clone());
                    }
                    yield Ok(Event::default().data(serde_json::to_string(&ev).unwrap_or_default()));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct ShipCommandBody {
    cmd: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

async fn handle_ship_command(
    State(s): State<ShipApiState>,
    Json(body): Json<ShipCommandBody>,
) -> impl IntoResponse {
    match body.cmd.as_str() {
        "evaluate" => match s.daemon.evaluate().await {
            Ok(r) => Json(serde_json::json!({ "ok": true, "report": r })).into_response(),
            Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
        },
        "draft" => {
            let cfg = s.daemon.config().await;
            let pipeline = ax_ship::ShipPipeline::new(
                s.daemon.project_root.clone(),
                cfg,
                s.daemon.bus.clone(),
            );
            let title = body.title.as_deref().unwrap_or("ax ship draft");
            let pr_body = body
                .body
                .as_deref()
                .unwrap_or("Created via ax Command Center");
            match pipeline.create_draft_pr(title, pr_body).await {
                Ok(pr) => Json(serde_json::json!({ "ok": true, "pr": pr })).into_response(),
                Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
            }
        }
        other => Json(serde_json::json!({ "ok": false, "error": format!("unknown cmd: {other}") }))
            .into_response(),
    }
}

async fn handle_get_config(State(s): State<ShipApiState>) -> impl IntoResponse {
    let config = s.daemon.config().await;
    let sonar = sonar_discovery(&config).await;
    let setup = sonar_setup_status(&config, &s.daemon.project_root).await.ok();
    Json(serde_json::json!({ "config": config, "sonar": sonar, "sonar_setup": setup }))
}

async fn handle_put_config(
    State(s): State<ShipApiState>,
    Json(config): Json<ShipConfig>,
) -> impl IntoResponse {
    if s.readonly {
        return readonly_err().into_response();
    }
    let current = s.daemon.config().await;
    if let Err(e) = validate_config_change(&current, &config).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": e })),
        )
            .into_response();
    }
    match s.daemon.set_config(config).await {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": e })),
        )
            .into_response(),
    }
}

async fn handle_sonar_discover(State(s): State<ShipApiState>) -> impl IntoResponse {
    let config = s.daemon.config().await;
    let discovery = sonar_discovery(&config).await;
    let setup = sonar_setup_status(&config, &s.daemon.project_root).await.ok();
    Json(serde_json::json!({ "discovery": discovery, "setup": setup }))
}

async fn handle_sonar_setup(State(s): State<ShipApiState>) -> impl IntoResponse {
    let config = s.daemon.config().await;
    match sonar_setup_status(&config, &s.daemon.project_root).await {
        Ok(setup) => Json(serde_json::json!({ "ok": true, "setup": setup })).into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

async fn handle_sonar_bootstrap(State(s): State<ShipApiState>) -> impl IntoResponse {
    if s.readonly {
        return readonly_err().into_response();
    }
    let config = s.daemon.config().await;
    let bootstrap_cfg = SonarBootstrapConfig::from_sonar(
        &config.sonar.host,
        &config.sonar.project_key,
        &config.sonar.token_env,
    );
    match bootstrap_sonar(&bootstrap_cfg, &s.daemon.project_root).await {
        Ok(result) => {
            let setup = sonar_setup_status(&config, &s.daemon.project_root)
                .await
                .ok();
            Json(serde_json::json!({
                "ok": true,
                "result": result,
                "setup": setup,
            }))
            .into_response()
        }
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

async fn handle_sonar_install(State(s): State<ShipApiState>) -> impl IntoResponse {
    if s.readonly {
        return readonly_err().into_response();
    }
    let mut config = s.daemon.config().await;
    config.sonar.enabled = true;
    let log = InstallLog::new();
    let result = install_sonar_with_log(&mut config, &log).await;
    match result {
        Ok(discovery) => {
            let _ = s.daemon.set_config(config).await;
            Json(serde_json::json!({
                "ok": true,
                "discovery": discovery,
                "logs": log.lines(),
            }))
            .into_response()
        }
        Err(e) => Json(serde_json::json!({
            "ok": false,
            "error": e,
            "logs": log.lines(),
        }))
        .into_response(),
    }
}

async fn handle_sonar_install_stream(
    State(s): State<ShipApiState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    sonar_operation_stream(s, SonarOp::Install).await
}

async fn handle_sonar_start_stream(
    State(s): State<ShipApiState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    sonar_operation_stream(s, SonarOp::Start).await
}

async fn handle_sonar_stop_stream(
    State(s): State<ShipApiState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    sonar_operation_stream(s, SonarOp::Stop).await
}

async fn handle_sonar_stop(State(s): State<ShipApiState>) -> impl IntoResponse {
    if s.readonly {
        return readonly_err().into_response();
    }
    let config = s.daemon.config().await;
    let log = InstallLog::new();
    match stop_sonar_with_log(&config, &log).await {
        Ok(discovery) => Json(serde_json::json!({
            "ok": true,
            "discovery": discovery,
            "logs": log.lines(),
        }))
        .into_response(),
        Err(e) => Json(serde_json::json!({
            "ok": false,
            "error": e,
            "logs": log.lines(),
        }))
        .into_response(),
    }
}

enum SonarOp {
    Install,
    Start,
    Stop,
}

async fn sonar_operation_stream(
    s: ShipApiState,
    op: SonarOp,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let daemon = s.daemon.clone();
    let readonly = s.readonly;

    tokio::spawn(async move {
        if readonly {
            let _ = tx.send(
                serde_json::json!({
                    "type": "done",
                    "ok": false,
                    "error": "read-only mode (AX_WEB_READONLY=1)",
                    "logs": [],
                })
                .to_string(),
            );
            return;
        }

        let log = InstallLog::with_stream(tx.clone());
        let mut config = daemon.config().await;
        let result = match op {
            SonarOp::Install => {
                config.sonar.enabled = true;
                install_sonar_with_log(&mut config, &log).await
            }
            SonarOp::Start => start_sonar_with_log(&config, &log).await,
            SonarOp::Stop => stop_sonar_with_log(&config, &log).await,
        };

        match result {
            Ok(discovery) => {
                if matches!(op, SonarOp::Install) {
                    let _ = daemon.set_config(config).await;
                }
                let _ = tx.send(
                    serde_json::json!({
                        "type": "done",
                        "ok": true,
                        "discovery": discovery,
                        "logs": log.lines(),
                    })
                    .to_string(),
                );
            }
            Err(e) => {
                let _ = tx.send(
                    serde_json::json!({
                        "type": "done",
                        "ok": false,
                        "error": e,
                        "logs": log.lines(),
                    })
                    .to_string(),
                );
            }
        }
    });

    let stream = async_stream::stream! {
        while let Some(data) = rx.recv().await {
            let is_done = data.contains("\"type\":\"done\"");
            yield Ok(Event::default().data(data));
            if is_done {
                break;
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn handle_sonar_start(State(s): State<ShipApiState>) -> impl IntoResponse {
    if s.readonly {
        return readonly_err().into_response();
    }
    let config = s.daemon.config().await;
    let log = InstallLog::new();
    match start_sonar_with_log(&config, &log).await {
        Ok(discovery) => Json(serde_json::json!({
            "ok": true,
            "discovery": discovery,
            "logs": log.lines(),
        }))
        .into_response(),
        Err(e) => Json(serde_json::json!({
            "ok": false,
            "error": e,
            "logs": log.lines(),
        }))
        .into_response(),
    }
}

async fn sonar_discovery(config: &ShipConfig) -> ax_quality::SonarDiscovery {
    let name = config
        .sonar
        .podman_container
        .as_deref()
        .unwrap_or("sonarqube");
    let pref = if config.sonar.container_runtime == "auto" {
        None
    } else {
        Some(config.sonar.container_runtime.as_str())
    };
    discover_sonar(&config.sonar.host, name, pref)
}

async fn sonar_setup_status(
    config: &ShipConfig,
    project_root: &std::path::Path,
) -> Result<ax_quality::SonarSetupStatus, String> {
    let bootstrap_cfg = SonarBootstrapConfig::from_sonar(
        &config.sonar.host,
        &config.sonar.project_key,
        &config.sonar.token_env,
    );
    inspect_setup(
        &bootstrap_cfg,
        project_root,
        &config.sonar.scanner_path,
    )
    .await
}

async fn install_sonar_with_log(
    config: &mut ShipConfig,
    log: &InstallLog,
) -> Result<ax_quality::SonarDiscovery, String> {
    let name = config
        .sonar
        .podman_container
        .get_or_insert_with(|| "sonarqube".into())
        .clone();
    let pref = if config.sonar.container_runtime == "auto" {
        None
    } else {
        Some(config.sonar.container_runtime.as_str())
    };
    let host_port = parse_host_port(&config.sonar.host).unwrap_or(9000);
    ensure_sonar_live_with_log(&config.sonar.host, &name, pref, host_port, log).await
}

async fn start_sonar_with_log(
    config: &ShipConfig,
    log: &InstallLog,
) -> Result<ax_quality::SonarDiscovery, String> {
    let name = config
        .sonar
        .podman_container
        .as_deref()
        .unwrap_or("sonarqube");
    let pref = if config.sonar.container_runtime == "auto" {
        None
    } else {
        Some(config.sonar.container_runtime.as_str())
    };
    start_sonar_container_with_log(&config.sonar.host, name, pref, log).await
}

async fn stop_sonar_with_log(
    config: &ShipConfig,
    log: &InstallLog,
) -> Result<ax_quality::SonarDiscovery, String> {
    let name = config
        .sonar
        .podman_container
        .as_deref()
        .unwrap_or("sonarqube");
    stop_sonar_container_with_log(&config.sonar.host, name, log).await
}

async fn validate_config_change(current: &ShipConfig, next: &ShipConfig) -> Result<(), String> {
    let discovery = sonar_discovery(current).await;

    if discovery.container.is_some()
        && (next.sonar.host != current.sonar.host
            || next.sonar.podman_container != current.sonar.podman_container
            || next.sonar.container_runtime != current.sonar.container_runtime)
    {
        let msg = if discovery
            .container
            .as_ref()
            .map(|c| c.running)
            .unwrap_or(false)
        {
            "Cannot change SonarQube host, container name, or runtime while the container is running"
        } else {
            "Stop the SonarQube container before changing host, container name, or runtime"
        };
        return Err(msg.into());
    }

    if next.ship.web_port != current.ship.web_port {
        return Err(
            "Cannot change dashboard port while Command Center web UI is running on this port"
                .into(),
        );
    }

    Ok(())
}

fn parse_host_port(host: &str) -> Option<u16> {
    let trimmed = host.trim_end_matches('/');
    let after_scheme = trimmed.split("//").nth(1).unwrap_or(trimmed);
    after_scheme
        .split(':')
        .nth(1)
        .and_then(|p| p.parse().ok())
}

pub async fn start_watcher(state: ShipApiState) {
    let root = state.daemon.project_root.clone();
    let bus = state.daemon.bus.clone();
    let _ = ax_ship::ShipDaemon::new(root.clone()).run_watch().await;
    let _ = state.daemon.evaluate().await;
    let _ = bus;
}
