//! Ship Command Center API (SSE + status + settings).

use std::convert::Infallible;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ax_quality::{
    bootstrap_sonar, discover_sonar, ensure_sonar_live_with_log, ensure_sonar_ready_for_scan,
    ensure_sonar_stack_online, inspect_setup, read_sonar_token, regenerate_sonar_token,
    start_sonar_container_with_log, stop_sonar_container_with_log, validate_sonar_login,
    validate_sonar_token, InstallLog, SonarBootstrapConfig, SonarClient,
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

use crate::workspace_state::WebHub;

#[derive(Clone)]
pub struct ShipApiState {
    pub daemon: Arc<ShipDaemon>,
    pub report: Arc<Mutex<Option<ShipReport>>>,
    pub readonly: bool,
    pub evaluating: Arc<AtomicBool>,
}

pub fn router_hub(hub: WebHub) -> Router {
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
        .route("/sonar/validate-login", post(handle_sonar_validate_login))
        .route("/sonar/validate-token", get(handle_sonar_validate_token))
        .route("/sonar/regenerate-token", post(handle_sonar_regenerate_token))
        .route("/sonar/scan", post(handle_sonar_scan))
        .route("/sonar/scan/stream", post(handle_sonar_scan_stream))
        .route("/sonar/exclude", post(handle_sonar_exclude))
        .route("/sonar/ui/info", get(crate::sonar_proxy::handle_sonar_ui_info))
        .route("/sonar/ui", axum::routing::any(crate::sonar_proxy::handle_sonar_ui_proxy))
        .route("/sonar/ui/", axum::routing::any(crate::sonar_proxy::handle_sonar_ui_proxy))
        .route("/sonar/ui/{*path}", axum::routing::any(crate::sonar_proxy::handle_sonar_ui_proxy))
        .with_state(hub)
}

fn readonly_err() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({ "error": "read-only mode (AX_WEB_READONLY=1)" })),
    )
}

struct ShipCtx {
    daemon: Arc<ShipDaemon>,
    report: Arc<Mutex<Option<ShipReport>>>,
    evaluating: Arc<AtomicBool>,
}

async fn ship_ctx(hub: &WebHub) -> ShipCtx {
    let ws = hub.read().await;
    ShipCtx {
        daemon: ws.ship.daemon.clone(),
        report: ws.ship.report.clone(),
        evaluating: ws.ship.evaluating.clone(),
    }
}

async fn handle_ship_status(State(hub): State<WebHub>) -> impl IntoResponse {
    let ctx = ship_ctx(&hub).await;
    ctx.daemon.reload_config().await;
    let config = ctx.daemon.config().await;
    let discovered = ax_ship::discover_git_repos(&ctx.daemon.project_root);
    let git_roots = ax_ship::resolve_git_roots(&ctx.daemon.project_root, &config).ok();
    let branches: std::collections::HashMap<String, String> = git_roots
        .as_ref()
        .map(|roots| {
            roots
                .iter()
                .filter_map(|root| {
                    let name = root.file_name()?.to_str()?;
                    let branch = ax_git::current_branch(root).ok().flatten()?;
                    Some((name.to_string(), branch))
                })
                .collect()
        })
        .unwrap_or_default();
    let branch = branches.values().next().cloned();
    let last_run = ax_ship::read_run_log(&ctx.daemon.project_root);
    let report = ctx.report.lock().await.clone();
    let run_in_progress = last_run.started_at.is_some() && last_run.finished_at.is_none();
    let evaluating = ctx.evaluating.load(Ordering::SeqCst) || run_in_progress;
    Json(serde_json::json!({
        "branch": branch,
        "branches": branches,
        "git_roots": config.ship.git_roots,
        "git_roots_discovered": discovered,
        "git_root_paths": git_roots.map(|roots| roots.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>()),
        "report": report,
        "config": config,
        "last_run": last_run,
        "evaluating": evaluating,
    }))
}

async fn handle_ship_impact(State(hub): State<WebHub>) -> impl IntoResponse {
    let ctx = ship_ctx(&hub).await;
    let report = ctx.report.lock().await.clone();
    Json(serde_json::json!({ "report": report }))
}

async fn handle_ship_events(
    State(hub): State<WebHub>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let ctx = ship_ctx(&hub).await;
    let mut rx = ctx.daemon.bus.subscribe();
    let report_store = ctx.report.clone();
    let stream = async_stream::stream! {
        let initial_log = ax_ship::read_run_log(&ctx.daemon.project_root);
        yield Ok(Event::default().data(
            serde_json::to_string(&ShipEvent::RunLogUpdated { last_run: initial_log }).unwrap_or_default(),
        ));
        let evaluating = ctx.evaluating.load(Ordering::SeqCst)
            || {
                let log = ax_ship::read_run_log(&ctx.daemon.project_root);
                log.started_at.is_some() && log.finished_at.is_none()
            };
        if !evaluating {
            if let Some(report) = report_store.lock().await.clone() {
                yield Ok(Event::default().data(
                    serde_json::to_string(&ShipEvent::ReportUpdated { report }).unwrap_or_default(),
                ));
            }
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
    State(hub): State<WebHub>,
    Json(body): Json<ShipCommandBody>,
) -> impl IntoResponse {
    let ctx = ship_ctx(&hub).await;
    match body.cmd.as_str() {
        "evaluate" => {
            if hub.readonly {
                return readonly_err().into_response();
            }
            if ctx.evaluating.swap(true, Ordering::SeqCst) {
                let last_run = ax_ship::read_run_log(&ctx.daemon.project_root);
                return Json(serde_json::json!({
                    "ok": false,
                    "error": "evaluate already running",
                    "evaluating": true,
                    "last_run": last_run,
                }))
                .into_response();
            }
            let daemon = ctx.daemon.clone();
            let evaluating = ctx.evaluating.clone();
            let report_store = ctx.report.clone();
            tokio::spawn(async move {
                let result = daemon.evaluate().await;
                evaluating.store(false, Ordering::SeqCst);
                match result {
                    Ok(report) => {
                        *report_store.lock().await = Some(report);
                    }
                    Err(message) => {
                        daemon.bus.publish(ShipEvent::Error { message });
                    }
                }
            });
            let last_run = ax_ship::read_run_log(&ctx.daemon.project_root);
            Json(serde_json::json!({
                "ok": true,
                "started": true,
                "evaluating": true,
                "last_run": last_run,
            }))
            .into_response()
        }
        "draft" => {
            let cfg = ctx.daemon.config().await;
            let pipeline = ax_ship::ShipPipeline::new(
                ctx.daemon.project_root.clone(),
                cfg,
                ctx.daemon.bus.clone(),
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

async fn handle_get_config(State(hub): State<WebHub>) -> impl IntoResponse {
    let ctx = ship_ctx(&hub).await;
    ctx.daemon.reload_config().await;
    let config = ctx.daemon.config().await;
    let discovered = ax_ship::discover_git_repos(&ctx.daemon.project_root);
    let sonar = sonar_status_light(&config).await;
    Json(serde_json::json!({
        "config": config,
        "sonar": sonar,
        "git_roots_discovered": discovered,
    }))
}

async fn handle_put_config(
    State(hub): State<WebHub>,
    Json(config): Json<ShipConfig>,
) -> impl IntoResponse {
    if hub.readonly {
        return readonly_err().into_response();
    }
    let ctx = ship_ctx(&hub).await;
    let current = ctx.daemon.config().await;
    if let Err(e) = validate_config_change(&current, &config).await {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": e })),
        )
            .into_response();
    }
    match ctx.daemon.set_config(config.clone()).await {
        Ok(()) => {
            if sonar_proxy_settings_changed(&current, &config) {
                hub.sonar_proxy.lock().await.invalidate();
            }
            Json(serde_json::json!({ "ok": true })).into_response()
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "ok": false, "error": e })),
        )
            .into_response(),
    }
}

async fn handle_sonar_discover(State(hub): State<WebHub>) -> impl IntoResponse {
    let ctx = ship_ctx(&hub).await;
    ctx.daemon.reload_config().await;
    let config = ctx.daemon.config().await;
    let discovery = sonar_discovery(&config).await;
    if config.sonar.enabled && discovery.reachable {
        ctx.daemon.spawn_sonar_auto_provision();
    }
    let setup = sonar_setup_status(&config, &ctx.daemon.project_root, discovery.reachable).await;
    Json(serde_json::json!({ "discovery": discovery, "setup": setup }))
}

async fn handle_sonar_setup(State(hub): State<WebHub>) -> impl IntoResponse {
    let ctx = ship_ctx(&hub).await;
    let config = ctx.daemon.config().await;
    let discovery = sonar_discovery(&config).await;
    match sonar_setup_status(&config, &ctx.daemon.project_root, discovery.reachable).await {
        Some(setup) => Json(serde_json::json!({ "ok": true, "setup": setup })).into_response(),
        None => Json(serde_json::json!({
            "ok": true,
            "setup": null,
            "message": "SonarQube is not reachable",
        }))
        .into_response(),
    }
}

#[derive(Debug, Deserialize)]
struct SonarValidateLoginBody {
    host: String,
    admin_user: String,
    admin_password: String,
}

async fn handle_sonar_validate_login(Json(body): Json<SonarValidateLoginBody>) -> impl IntoResponse {
    match validate_sonar_login(&body.host, &body.admin_user, &body.admin_password).await {
        Ok(valid) => Json(serde_json::json!({
            "ok": true,
            "reachable": true,
            "valid": valid,
            "message": if valid {
                "Login successful"
            } else {
                "Invalid username or password"
            },
        }))
        .into_response(),
        Err(e) if e.contains("not reachable") => Json(serde_json::json!({
            "ok": true,
            "reachable": false,
            "valid": false,
            "message": e,
        }))
        .into_response(),
        Err(e) => Json(serde_json::json!({
            "ok": false,
            "error": e,
        }))
        .into_response(),
    }
}

async fn handle_sonar_validate_token(State(hub): State<WebHub>) -> impl IntoResponse {
    let ctx = ship_ctx(&hub).await;
    let config = ctx.daemon.config().await;
    let discovery = sonar_discovery(&config).await;
    if !discovery.reachable {
        return Json(serde_json::json!({
            "ok": true,
            "reachable": false,
            "configured": false,
            "valid": false,
            "message": "SonarQube is not reachable",
        }))
        .into_response();
    }

    let token = read_sonar_token(&ctx.daemon.project_root, &config.sonar.token_env);
    let Some(token) = token else {
        return Json(serde_json::json!({
            "ok": true,
            "reachable": true,
            "configured": false,
            "valid": false,
            "message": "No scanner token — run Setup project & token",
        }))
        .into_response();
    };

    let valid = validate_sonar_token(&config.sonar.host, &token).await;
    Json(serde_json::json!({
        "ok": true,
        "reachable": true,
        "configured": true,
        "valid": valid,
        "message": if valid {
            "Scanner token is valid"
        } else {
            "Scanner token rejected by SonarQube — run Setup project & token to regenerate"
        },
    }))
    .into_response()
}

async fn handle_sonar_regenerate_token(State(hub): State<WebHub>) -> impl IntoResponse {
    if hub.readonly {
        return readonly_err().into_response();
    }
    let ctx = ship_ctx(&hub).await;
    let config = ctx.daemon.config().await;
    let bootstrap_cfg = SonarBootstrapConfig::resolve_for_project(
        &config.sonar,
        &ctx.daemon.project_root,
    );

    match regenerate_sonar_token(&bootstrap_cfg, &ctx.daemon.project_root).await {
        Ok(result) => {
            let discovery = sonar_discovery(&config).await;
            let setup = sonar_setup_status(&config, &ctx.daemon.project_root, discovery.reachable).await;
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

async fn handle_sonar_bootstrap(State(hub): State<WebHub>) -> impl IntoResponse {
    if hub.readonly {
        return readonly_err().into_response();
    }
    let ctx = ship_ctx(&hub).await;
    ctx.daemon.reload_config().await;
    let mut config = ctx.daemon.config().await;
    let bootstrap_cfg = SonarBootstrapConfig::resolve_for_project(
        &config.sonar,
        &ctx.daemon.project_root,
    );

    if config.sonar.project_key != bootstrap_cfg.project_key {
        config.sonar.project_key = bootstrap_cfg.project_key.clone();
        if let Err(e) = ctx.daemon.set_config(config.clone()).await {
            return Json(serde_json::json!({ "ok": false, "error": e })).into_response();
        }
    }

    let repo_names = sonar_repo_names(&config, &ctx.daemon.project_root).await;
    match bootstrap_sonar(
        &bootstrap_cfg,
        &ctx.daemon.project_root,
        &repo_names,
        Some(&config.sonar),
    )
    .await {
        Ok(result) => {
            config.sonar.enabled = true;
            if let Err(e) = ctx.daemon.set_config(config.clone()).await {
                return Json(serde_json::json!({ "ok": false, "error": e })).into_response();
            }
            let setup = sonar_setup_status(&config, &ctx.daemon.project_root, true).await;
            Json(serde_json::json!({
                "ok": true,
                "result": result,
                "setup": setup,
                "config": config,
            }))
            .into_response()
        }
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

async fn handle_sonar_install(State(hub): State<WebHub>) -> impl IntoResponse {
    if hub.readonly {
        return readonly_err().into_response();
    }
    let ctx = ship_ctx(&hub).await;
    let mut config = ctx.daemon.config().await;
    config.sonar.enabled = true;
    let log = InstallLog::new();
    let result = install_sonar_with_log(&mut config, &log).await;
    match result {
        Ok(discovery) => {
            let _ = ctx.daemon.set_config(config).await;
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
    State(hub): State<WebHub>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    sonar_operation_stream(hub, SonarOp::Install).await
}

async fn handle_sonar_start_stream(
    State(hub): State<WebHub>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    sonar_operation_stream(hub, SonarOp::Start).await
}

async fn handle_sonar_stop_stream(
    State(hub): State<WebHub>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    sonar_operation_stream(hub, SonarOp::Stop).await
}

async fn handle_sonar_stop(State(hub): State<WebHub>) -> impl IntoResponse {
    if hub.readonly {
        return readonly_err().into_response();
    }
    let ctx = ship_ctx(&hub).await;
    let config = ctx.daemon.config().await;
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

#[derive(Debug, Deserialize, Default)]
struct SonarScanBody {
    project_key: Option<String>,
}

#[derive(Deserialize)]
struct SonarExcludeBody {
    repo: String,
    excluded: bool,
}

async fn handle_sonar_exclude(
    State(hub): State<WebHub>,
    Json(body): Json<SonarExcludeBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return readonly_err().into_response();
    }
    let ctx = ship_ctx(&hub).await;
    let mut config = ctx.daemon.config().await;
    let repo = body.repo.trim().to_string();
    if repo.is_empty() {
        return Json(serde_json::json!({ "ok": false, "error": "repo name required" })).into_response();
    }
    if body.excluded {
        if !config.sonar.exclude_repos.contains(&repo) {
            config.sonar.exclude_repos.push(repo);
            config.sonar.exclude_repos.sort();
        }
    } else {
        config.sonar.exclude_repos.retain(|r| r != &body.repo);
    }
    let exclude_repos = config.sonar.exclude_repos.clone();
    if let Err(e) = ctx.daemon.set_config(config).await {
        return Json(serde_json::json!({ "ok": false, "error": e })).into_response();
    }
    Json(serde_json::json!({ "ok": true, "exclude_repos": exclude_repos })).into_response()
}

async fn handle_sonar_scan_stream(
    State(hub): State<WebHub>,
    body: Option<Json<SonarScanBody>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let project_key = body.map(|b| b.project_key.clone()).unwrap_or_default();
    sonar_scan_stream(hub, project_key).await
}

async fn handle_sonar_scan(
    State(hub): State<WebHub>,
    body: Option<Json<SonarScanBody>>,
) -> impl IntoResponse {
    if hub.readonly {
        return readonly_err().into_response();
    }
    let ctx = ship_ctx(&hub).await;
    let log = InstallLog::new();
    let mut config = ctx.daemon.config().await;
    let project_key = body.and_then(|b| b.project_key.clone());
    match scan_sonar_with_log(
        &mut config,
        &ctx.daemon.project_root,
        &log,
        project_key.as_deref(),
    )
    .await {
        Ok(gate) => {
            let _ = ctx.daemon.set_config(config).await;
            Json(serde_json::json!({
                "ok": true,
                "quality_gate": gate,
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

async fn sonar_scan_stream(
    hub: WebHub,
    project_key: Option<String>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let ctx = ship_ctx(&hub).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let daemon = ctx.daemon.clone();
    let readonly = hub.readonly;

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
        match scan_sonar_with_log(
            &mut config,
            &daemon.project_root,
            &log,
            project_key.as_deref(),
        )
        .await {
            Ok(gate) => {
                let _ = daemon.set_config(config).await;
                let _ = tx.send(
                    serde_json::json!({
                        "type": "done",
                        "ok": true,
                        "quality_gate": gate,
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

enum SonarOp {
    Install,
    Start,
    Stop,
}

async fn sonar_operation_stream(
    hub: WebHub,
    op: SonarOp,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let ctx = ship_ctx(&hub).await;
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let daemon = ctx.daemon.clone();
    let readonly = hub.readonly;

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
                if matches!(op, SonarOp::Install | SonarOp::Start) {
                    daemon.spawn_sonar_auto_provision();
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

async fn handle_sonar_start(State(hub): State<WebHub>) -> impl IntoResponse {
    if hub.readonly {
        return readonly_err().into_response();
    }
    let ctx = ship_ctx(&hub).await;
    let config = ctx.daemon.config().await;
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

async fn sonar_status_light(config: &ShipConfig) -> ax_quality::SonarDiscovery {
    ax_quality::SonarDiscovery {
        runtimes: vec![],
        preferred: None,
        container: None,
        database: None,
        reachable: false,
        host: config.sonar.host.trim_end_matches('/').to_string(),
        embedded_database: false,
    }
}

async fn sonar_discovery(config: &ShipConfig) -> ax_quality::SonarDiscovery {
    let host = config.sonar.host.clone();
    let name = config
        .sonar
        .podman_container
        .clone()
        .unwrap_or_else(|| "sonarqube".into());
    let runtime_pref = if config.sonar.container_runtime == "auto" {
        String::new()
    } else {
        config.sonar.container_runtime.clone()
    };
    tokio::task::spawn_blocking(move || {
        let pref = if runtime_pref.is_empty() {
            None
        } else {
            Some(runtime_pref.as_str())
        };
        discover_sonar(&host, &name, pref)
    })
    .await
    .unwrap_or_else(|_| ax_quality::SonarDiscovery {
        runtimes: vec![],
        preferred: None,
        container: None,
        database: None,
        reachable: false,
        host: config.sonar.host.trim_end_matches('/').to_string(),
        embedded_database: false,
    })
}

fn sonar_proxy_settings_changed(current: &ShipConfig, next: &ShipConfig) -> bool {
    current.sonar.host != next.sonar.host
        || current.sonar.admin_user != next.sonar.admin_user
        || current.sonar.admin_password != next.sonar.admin_password
        || current.sonar.podman_container != next.sonar.podman_container
        || current.sonar.container_runtime != next.sonar.container_runtime
}

async fn sonar_repo_names(config: &ShipConfig, project_root: &std::path::Path) -> Vec<String> {
    ax_ship::resolve_sonar_repo_names(project_root, config)
}

async fn sonar_setup_status(
    config: &ShipConfig,
    project_root: &std::path::Path,
    reachable: bool,
) -> Option<ax_quality::SonarSetupStatus> {
    if !reachable {
        return None;
    }
    let bootstrap_cfg = SonarBootstrapConfig::resolve_for_project(&config.sonar, project_root);
    let repo_names = sonar_repo_names(config, project_root).await;
    inspect_setup(
        &bootstrap_cfg,
        project_root,
        &config.sonar.scanner_path,
        &repo_names,
    )
    .await
    .ok()
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

async fn scan_sonar_with_log(
    config: &mut ShipConfig,
    project_root: &std::path::Path,
    log: &InstallLog,
    project_key: Option<&str>,
) -> Result<ax_quality::QualityGateResult, String> {
    config.sonar.enabled = true;
    let mut repo_names = sonar_repo_names(config, project_root).await;
    let workspace_repo_count = repo_names.len();
    if let Some(key) = project_key.filter(|k| !k.trim().is_empty()) {
        let bootstrap_cfg =
            SonarBootstrapConfig::resolve_for_project(&config.sonar, project_root);
        let workspace_key = ax_quality::workspace_sonar_key(&bootstrap_cfg.project_key, project_root);
        let multi_repo = workspace_repo_count > 1;
        repo_names.retain(|repo| {
            ax_quality::canonical_repo_project_key(&workspace_key, repo, multi_repo) == key
        });
        if repo_names.is_empty() {
            return Err(format!("Unknown Sonar project key '{key}'"));
        }
        log.push(format!("Scanning SonarQube project {key} ({})…", repo_names[0]));
    } else if repo_names.is_empty() {
        log.push("No child git repositories — scanning workspace as a single SonarQube project.");
    } else {
        log.push(format!(
            "Preparing {} SonarQube project(s): {}",
            repo_names.len(),
            repo_names.join(", ")
        ));
    }
    ensure_sonar_stack_online(&config.sonar, log).await?;
    ensure_sonar_ready_for_scan(&config.sonar, project_root, &repo_names).await?;
    let client = SonarClient::new(config.sonar.clone());
    log.push("Ensuring SonarQube is running…");
    client.ensure_running().await?;

    let scan_root = project_root.to_path_buf();
    let scan_repos = repo_names.clone();
    let scan_log = log.clone();
    let sonar_cfg = config.sonar.clone();
    tokio::task::spawn_blocking(move || {
        let c = SonarClient::new(sonar_cfg);
        c.run_full_scan_with_log_multi(&scan_root, &scan_repos, &scan_log, workspace_repo_count)
    })
    .await
    .map_err(|e| format!("scan task panicked: {e}"))??;

    log.push("Fetching quality gate status…");
    let gate_client = SonarClient::new(config.sonar.clone());
    gate_client.fetch_quality_gate(project_root, &repo_names).await
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
