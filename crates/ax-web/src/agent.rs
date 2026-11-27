//! Agent terminal API — install, profiles, chat streaming.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use ax_agent::chat::chunk_text;
use ax_agent::config::{load_agents_config, save_agents_config, AgentsConfig};
use ax_agent::profiles::{
    create_profile, list_profiles, refresh_auth_statuses, remove_profile, set_active_profile,
    update_profile, AuthStatus,
};
use ax_agent::run_agent_turn;
use ax_installer::{
    auth_command, build_child_env, detect_cli_available, ensure_agent_ready, headless_command,
    install_cli, install_cli_targets, install_selected, uninstall_targets, TARGETS,
};
use axum::extract::{Path as AxPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{
    sse::{Event, KeepAlive, Sse},
    IntoResponse, Json,
};
use axum::routing::{delete, get, post, put};
use axum::Router;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::workspace_state::WebHub;

async fn handle_status(State(hub): State<WebHub>) -> impl IntoResponse {
    ax_installer::ensure_global_config();
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    drop(ws);
    let targets = ax_installer::agent_status(&root).unwrap_or_default();
    let cfg = refresh_auth_statuses().unwrap_or_else(|_| load_agents_config());
    Json(serde_json::json!({
        "ok": true,
        "readonly": hub.readonly,
        "targets": targets,
        "catalog": targets,
        "config": cfg,
        "all_targets": TARGETS,
    }))
}

#[derive(Deserialize)]
struct InstallBody {
    targets: Vec<String>,
}

/// Install routes write IDE config files, so only the local browser may call them.
fn refuse_install(hub: &WebHub, headers: &HeaderMap) -> Option<axum::response::Response> {
    let error = if hub.readonly {
        "Read-only mode"
    } else if !crate::dav::mount::allowed(headers, false, hub.port) {
        "Only the local browser on this machine can install ax into IDEs"
    } else {
        return None;
    };
    Some(
        (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "ok": false, "error": error })),
        )
            .into_response(),
    )
}

fn report_json(r: &ax_installer::TargetReport) -> serde_json::Value {
    serde_json::json!({
        "id": r.id,
        "display_name": r.display_name,
        "files": r.files.iter().map(|f| f.path.to_string_lossy()).collect::<Vec<_>>(),
        "notes": r.notes,
    })
}

async fn handle_install(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Json(body): Json<InstallBody>,
) -> impl IntoResponse {
    if let Some(r) = refuse_install(&hub, &headers) {
        return r;
    }
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    drop(ws);
    match install_selected(&root, &body.targets) {
        Ok(summary) => Json(serde_json::json!({
            "ok": true,
            "reports": summary.reports.iter().map(report_json).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

async fn handle_install_stream(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Json(body): Json<InstallBody>,
) -> impl IntoResponse {
    if let Some(r) = refuse_install(&hub, &headers) {
        return r;
    }
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    drop(ws);
    let targets = body.targets.clone();
    tokio::spawn(async move {
        let _ = tx.send(
            serde_json::json!({"type":"line","text":"Installing ax MCP for selected agents…"})
                .to_string(),
        );
        match install_selected(&root, &targets) {
            Ok(summary) => {
                for r in &summary.reports {
                    let _ = tx.send(serde_json::json!({"type":"line","text": format!("Configured {}", r.display_name)}).to_string());
                    for n in &r.notes {
                        let _ = tx.send(serde_json::json!({"type":"line","text": n}).to_string());
                    }
                }
                let _ = tx.send(serde_json::json!({"type":"done","ok":true}).to_string());
            }
            Err(e) => {
                let _ =
                    tx.send(serde_json::json!({"type":"done","ok":false,"error":e}).to_string());
            }
        }
    });
    sse_from_channel(rx).into_response()
}

async fn handle_cli_install_stream(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Json(body): Json<InstallBody>,
) -> impl IntoResponse {
    if let Some(r) = refuse_install(&hub, &headers) {
        return r;
    }
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let targets = body.targets.clone();
    tokio::task::spawn_blocking(move || {
        let results = install_cli_targets(&targets, &mut |line| {
            let _ = tx.send(serde_json::json!({"type":"line","text": line}).to_string());
        });
        let ok = results.iter().all(|r| r.ok);
        let _ = tx.send(
            serde_json::json!({
                "type": "done",
                "ok": ok,
                "results": results.iter().map(|r| serde_json::json!({
                    "target": r.target,
                    "display_name": r.display_name,
                    "ok": r.ok,
                    "already_installed": r.already_installed,
                    "message": r.message,
                })).collect::<Vec<_>>(),
            })
            .to_string(),
        );
    });
    sse_from_channel(rx).into_response()
}

async fn handle_uninstall(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Json(body): Json<InstallBody>,
) -> impl IntoResponse {
    if let Some(r) = refuse_install(&hub, &headers) {
        return r;
    }
    let root = hub.read().await.project_root.clone();
    match uninstall_targets(&root, &body.targets) {
        Ok(reports) => Json(serde_json::json!({
            "ok": true,
            "reports": reports.len(),
            "results": reports.iter().map(report_json).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

#[derive(Deserialize)]
struct ConfigPutBody {
    config: AgentsConfig,
}

async fn handle_put_config(
    State(hub): State<WebHub>,
    Json(body): Json<ConfigPutBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    match save_agents_config(&body.config) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

async fn handle_profiles(State(_hub): State<WebHub>) -> impl IntoResponse {
    let cfg = refresh_auth_statuses().unwrap_or_else(|_| load_agents_config());
    Json(serde_json::json!({ "ok": true, "config": cfg }))
}

#[derive(Deserialize)]
struct CreateProfileBody {
    agent: String,
    id: String,
    label: String,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    key_env: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

async fn handle_create_profile(
    State(hub): State<WebHub>,
    Json(body): Json<CreateProfileBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    match create_profile(
        &body.agent,
        &body.id,
        &body.label,
        body.provider.as_deref(),
        body.key_env.as_deref(),
        body.model.as_deref(),
    ) {
        Ok(entry) => Json(serde_json::json!({ "ok": true, "profile": entry })).into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

#[derive(Deserialize)]
struct ActiveProfileBody {
    agent: String,
    profile_id: String,
}

async fn handle_set_active_profile(
    State(hub): State<WebHub>,
    Json(body): Json<ActiveProfileBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    match set_active_profile(&body.agent, &body.profile_id) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

#[derive(Deserialize)]
struct UpdateProfileBody {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    key_env: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

async fn handle_update_profile(
    State(hub): State<WebHub>,
    AxPath((agent, id)): AxPath<(String, String)>,
    Json(body): Json<UpdateProfileBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    match update_profile(
        &agent,
        &id,
        body.label.as_deref(),
        body.provider.as_deref(),
        body.key_env.as_deref(),
        body.model.as_deref(),
    ) {
        Ok(entry) => Json(serde_json::json!({ "ok": true, "profile": entry })).into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

async fn handle_delete_profile(
    AxPath((agent, id)): AxPath<(String, String)>,
    Query(keep): Query<QueryKeep>,
) -> impl IntoResponse {
    match remove_profile(&agent, &id, keep.keep_dir) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

#[derive(Deserialize, Default)]
struct QueryKeep {
    #[serde(default)]
    keep_dir: bool,
}

async fn handle_auth_stream(
    State(hub): State<WebHub>,
    AxPath((agent, id)): AxPath<(String, String)>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    let profiles = list_profiles(&agent);
    let Some(entry) = profiles.iter().find(|p| p.id == id) else {
        return Json(serde_json::json!({ "ok": false, "error": "Profile not found" }))
            .into_response();
    };
    if entry.data_dir.is_empty() {
        return Json(
            serde_json::json!({ "ok": false, "error": "Builtin profiles do not need auth" }),
        )
        .into_response();
    }

    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let data_dir = entry.data_dir.clone();
    let agent_id = agent.clone();
    let profile_id = id.clone();
    tokio::task::spawn_blocking(move || {
        let send = |v: serde_json::Value| {
            let _ = tx.send(v.to_string());
        };
        send(
            serde_json::json!({"type":"line","text": format!("Starting auth for {agent_id}/{profile_id}…")}),
        );

        if !detect_cli_available(&agent_id) {
            send(
                serde_json::json!({"type":"line","text": format!("{} CLI not found — installing…", agent_id)}),
            );
            let mut lines = Vec::new();
            match install_cli(&agent_id, &mut |line| lines.push(line.to_string())) {
                Ok(outcome) => {
                    for line in lines {
                        send(serde_json::json!({"type":"line","text": line}));
                    }
                    send(serde_json::json!({"type":"line","text": outcome.message}));
                    if !outcome.ok && !detect_cli_available(&agent_id) {
                        send(serde_json::json!({
                            "type":"line",
                            "text":"Install the CLI manually, log in, then click Mark authenticated."
                        }));
                        send(
                            serde_json::json!({"type":"done","ok":false,"manual":true,"error":"CLI not available"}),
                        );
                        return;
                    }
                }
                Err(e) => {
                    send(
                        serde_json::json!({"type":"line","text": format!("CLI install failed: {e}")}),
                    );
                    send(serde_json::json!({"type":"done","ok":false,"manual":true,"error":e}));
                    return;
                }
            }
        }

        let profile_env = match ax_agent::profiles::ensure_profile_env(&agent_id, &profile_id) {
            Ok(env) => env,
            Err(e) => {
                send(serde_json::json!({"type":"done","ok":false,"error": e}));
                return;
            }
        };
        let spec = match auth_command(&agent_id, &data_dir, &profile_env) {
            Ok(s) => s,
            Err(e) => {
                send(serde_json::json!({"type":"done","ok":false,"error": e}));
                return;
            }
        };

        let mut cmd = std::process::Command::new(&spec.program);
        cmd.args(&spec.args);
        apply_env(&mut cmd, &spec.extra_env);
        send(
            serde_json::json!({"type":"line","text":"Auth process started — complete login in the opened window or terminal."}),
        );
        let line_tx = tx.clone();
        let on_line = move |line: String| {
            let _ = line_tx.send(serde_json::json!({"type":"line","text": line}).to_string());
        };
        match wait_streaming(cmd, AUTH_TIMEOUT, on_line, || tx.is_closed()) {
            Ok(status) => {
                let ok = status.success();
                if ok
                    || ax_agent::profiles::detect_auth_status(&agent_id, &data_dir)
                        == AuthStatus::Authenticated
                {
                    let _ = ax_agent::profiles::mark_authenticated(&agent_id, &profile_id);
                    send(serde_json::json!({"type":"line","text":"Profile marked authenticated."}));
                } else {
                    send(
                        serde_json::json!({"type":"line","text":"Auth process ended — if you logged in successfully, click Mark authenticated."}),
                    );
                }
                send(serde_json::json!({"type":"done","ok":true}));
            }
            Err(e) => {
                send(
                    serde_json::json!({"type":"line","text": format!("Auth did not finish ({e}). Log in manually with the CLI, then click Mark authenticated.")}),
                );
                send(
                    serde_json::json!({"type":"done","ok":false,"manual":true,"error":e.to_string()}),
                );
            }
        }
    });
    sse_from_channel(rx).into_response()
}

async fn handle_mark_authenticated(
    State(hub): State<WebHub>,
    AxPath((agent, id)): AxPath<(String, String)>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    match ax_agent::profiles::mark_authenticated(&agent, &id) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

#[derive(Deserialize)]
struct ChatBody {
    prompt: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    agent: Option<String>,
    #[serde(default)]
    profile_id: Option<String>,
}

async fn handle_chat_stream(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Json(body): Json<ChatBody>,
) -> impl IntoResponse {
    // A chat run starts an agent with tool access in the project, like the
    // terminal, so only the local browser may start one.
    if !crate::dav::mount::allowed(&headers, hub.readonly, hub.port) {
        let error = if hub.readonly {
            "Read-only mode"
        } else {
            "Only the local browser on this machine can run the agent"
        };
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "ok": false, "error": error })),
        )
            .into_response();
    }
    let cfg = load_agents_config();
    let agent = body.agent.clone().unwrap_or_else(|| {
        cfg.preferred_external
            .clone()
            .unwrap_or_else(|| "builtin".into())
    });
    let profile_id = body
        .profile_id
        .clone()
        .or_else(|| cfg.active_profile.get(&agent).cloned())
        .unwrap_or_else(|| "default".into());

    let session_id = body.session_id.clone().unwrap_or_else(|| {
        format!(
            "sess-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or(0)
        )
    });

    let mode = resolve_agent_mode(&cfg, &agent);
    let explicit_external =
        body.agent.as_deref().is_some_and(|a| a != "builtin") || cfg.terminal_mode == "external";
    let (tx, rx) = mpsc::unbounded_channel::<String>();

    let prompt = body.prompt.clone();
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    drop(ws);
    // When the browser closes the stream, stop the run: dropping it kills an
    // external agent process (see `output_with_timeout`).
    let closed = tx.clone();
    let run = async move {
        let _ = tx.send(serde_json::json!({"type":"system","text": format!("Agent: {agent} · Profile: {profile_id} · Mode: {mode}")}).to_string());

        let explicit_external = explicit_external;
        if mode == "external" {
            let agent_for_ensure = agent.clone();
            let root_for_ensure = root.clone();
            let tx_ensure = tx.clone();
            let ensure_result = tokio::task::spawn_blocking(move || {
                ensure_agent_ready(&agent_for_ensure, &root_for_ensure, &mut |line| {
                    let _ = tx_ensure
                        .send(serde_json::json!({"type":"system","text": line}).to_string());
                })
            })
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r);

            if let Err(e) = ensure_result {
                let _ = tx.send(serde_json::json!({"type":"error","message": e}).to_string());
            } else {
                match run_external_prompt(&agent, &profile_id, &prompt, &root).await {
                    Ok(text) => stream_text_chunks(&tx, &text),
                    Err(e) => {
                        if explicit_external {
                            let _ = tx.send(
                                serde_json::json!({"type":"error","message": format!("External agent failed: {e}")})
                                    .to_string(),
                            );
                        } else {
                            let _ = tx.send(serde_json::json!({"type":"system","text": format!("External agent failed ({e}) — falling back to built-in.")}).to_string());
                            run_builtin_turn(&tx, &prompt, &root).await;
                        }
                    }
                }
            }
        } else {
            run_builtin_turn(&tx, &prompt, &root).await;
        }
        let _ = tx.send(serde_json::json!({"type":"done","session_id": session_id}).to_string());
    };
    tokio::spawn(async move {
        tokio::select! {
            () = run => {}
            () = closed.closed() => {}
        }
    });

    sse_from_channel(rx).into_response()
}

fn resolve_agent_mode(cfg: &AgentsConfig, agent: &str) -> String {
    match cfg.terminal_mode.as_str() {
        "builtin" => "builtin".into(),
        "external" => "external".into(),
        _ => {
            if agent == "builtin" {
                "builtin".into()
            } else if detect_cli_available(agent) {
                "external".into()
            } else {
                "builtin".into()
            }
        }
    }
}

fn apply_env(cmd: &mut std::process::Command, extra: &HashMap<String, String>) {
    for (k, v) in build_child_env() {
        cmd.env(k, v);
    }
    for (k, v) in extra {
        cmd.env(k, v);
    }
}

fn apply_env_tokio(cmd: &mut tokio::process::Command, extra: &HashMap<String, String>) {
    for (k, v) in build_child_env() {
        cmd.env(k, v);
    }
    for (k, v) in extra {
        cmd.env(k, v);
    }
}

fn stream_text_chunks(tx: &mpsc::UnboundedSender<String>, text: &str) {
    for chunk in chunk_text(text, 48) {
        let _ = tx.send(serde_json::json!({"type":"token","text": chunk}).to_string());
    }
}

async fn run_builtin_turn(tx: &mpsc::UnboundedSender<String>, prompt: &str, root: &Path) {
    match run_agent_turn(root, prompt).await {
        Ok(turn) => {
            for tool in &turn.tools {
                let _ = tx.send(
                    serde_json::json!({
                        "type": "tool_start",
                        "name": tool.name,
                    })
                    .to_string(),
                );
                let preview: String = tool.output.chars().take(2000).collect();
                let _ = tx.send(
                    serde_json::json!({
                        "type": "tool_end",
                        "name": tool.name,
                        "preview": preview,
                    })
                    .to_string(),
                );
            }
            stream_text_chunks(tx, &turn.answer);
        }
        Err(e) => {
            let _ = tx.send(serde_json::json!({"type":"error","message": e}).to_string());
        }
    }
}

async fn run_external_prompt(
    agent: &str,
    profile_id: &str,
    prompt: &str,
    project_root: &PathBuf,
) -> Result<String, String> {
    let envs = ax_agent::profiles::ensure_profile_env(agent, profile_id)?;
    let spec = headless_command(agent, prompt, project_root, &envs)?;
    let mut cmd = tokio::process::Command::new(&spec.program);
    cmd.args(&spec.args);
    apply_env_tokio(&mut cmd, &spec.extra_env);
    cmd.current_dir(project_root);
    let output = output_with_timeout(cmd, AGENT_RUN_TIMEOUT).await?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        let msg = if err.trim().is_empty() {
            out.into_owned()
        } else {
            err.into_owned()
        };
        return Err(msg);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// How long a sign-in CLI may run before it is stopped.
const AUTH_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// How long one external agent run may take before it is stopped.
const AGENT_RUN_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Longest piece of one output line sent to the browser.
const LINE_CAP: u64 = 8 * 1024;

/// Reads `pipe` line by line (long lines in pieces) until it closes.
fn spawn_line_reader(
    pipe: impl std::io::Read + Send + 'static,
    on_line: std::sync::Arc<dyn Fn(String) + Send + Sync>,
    done: std::sync::mpsc::Sender<()>,
) {
    std::thread::spawn(move || {
        use std::io::{BufRead, Read};
        let mut reader = std::io::BufReader::new(pipe);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match (&mut reader).take(LINE_CAP).read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) => {
                    let text = String::from_utf8_lossy(&buf);
                    let text = text.trim_end_matches(['\r', '\n']);
                    if !text.is_empty() {
                        on_line(text.to_string());
                    }
                }
            }
        }
        let _ = done.send(());
    });
}

/// Runs `cmd` to the end and returns its exit status. Its stdout and stderr
/// lines go to `on_line` as they arrive, so a chatty child never blocks on a
/// full pipe. The child is killed when `timeout` passes or as soon as
/// `cancelled` returns true.
fn wait_streaming(
    mut cmd: std::process::Command,
    timeout: Duration,
    on_line: impl Fn(String) + Send + Sync + 'static,
    cancelled: impl Fn() -> bool,
) -> Result<std::process::ExitStatus, String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let on_line: std::sync::Arc<dyn Fn(String) + Send + Sync> = std::sync::Arc::new(on_line);
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let mut readers = 0;
    if let Some(out) = child.stdout.take() {
        spawn_line_reader(out, on_line.clone(), done_tx.clone());
        readers += 1;
    }
    if let Some(err) = child.stderr.take() {
        spawn_line_reader(err, on_line, done_tx);
        readers += 1;
    }
    let deadline = std::time::Instant::now() + timeout;
    let outcome = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => {}
            Err(e) => break Err(e.to_string()),
        }
        if cancelled() {
            break Err("stopped because the browser closed the stream".to_string());
        }
        if std::time::Instant::now() >= deadline {
            break Err(format!("stopped after {} seconds", timeout.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    if outcome.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    // Let the readers pass on the last lines. A helper the child left
    // running may keep a pipe open, so this wait is bounded.
    for _ in 0..readers {
        if done_rx.recv_timeout(Duration::from_secs(2)).is_err() {
            break;
        }
    }
    outcome
}

/// Runs `cmd` and collects its output. The child is killed when `limit`
/// passes or when the returned future is dropped (the browser went away).
async fn output_with_timeout(
    mut cmd: tokio::process::Command,
    limit: Duration,
) -> Result<std::process::Output, String> {
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    match tokio::time::timeout(limit, cmd.output()).await {
        Ok(output) => output.map_err(|e| e.to_string()),
        Err(_) => Err(format!(
            "the agent run was stopped after {} seconds",
            limit.as_secs()
        )),
    }
}

fn sse_from_channel(
    mut rx: mpsc::UnboundedReceiver<String>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>>> {
    let stream = async_stream::stream! {
        while let Some(data) = rx.recv().await {
            let is_done = data.contains("\"type\":\"done\"") || data.contains("\"type\":\"error\"");
            yield Ok(Event::default().data(data));
            if is_done {
                break;
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Read-only mode refuses every agent request that is not a read, so a new
/// handler cannot forget the check.
async fn refuse_writes_when_readonly(
    State(hub): State<WebHub>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let read = req.method() == axum::http::Method::GET || req.method() == axum::http::Method::HEAD;
    if hub.readonly && !read {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })),
        )
            .into_response();
    }
    next.run(req).await
}

pub fn router_hub(hub: WebHub) -> Router {
    let terminal = Router::new()
        .route("/pty/ws", get(crate::agent_pty::handle_pty_ws))
        .with_state(hub.clone())
        .layer(axum::middleware::from_fn_with_state(
            hub.clone(),
            crate::agent_pty::reject_untrusted_origin,
        ));
    Router::new()
        .route("/status", get(handle_status))
        .route("/install", post(handle_install))
        .route("/install/stream", post(handle_install_stream))
        .route("/cli/install/stream", post(handle_cli_install_stream))
        .route("/uninstall", post(handle_uninstall))
        .route("/config", put(handle_put_config))
        .route("/profiles", get(handle_profiles))
        .route("/profiles", post(handle_create_profile))
        .route("/profiles/active", put(handle_set_active_profile))
        .route("/profiles/{agent}/{id}", put(handle_update_profile))
        .route("/profiles/{agent}/{id}", delete(handle_delete_profile))
        .route(
            "/profiles/{agent}/{id}/auth/stream",
            post(handle_auth_stream),
        )
        .route(
            "/profiles/{agent}/{id}/authenticated",
            post(handle_mark_authenticated),
        )
        .route("/chat/stream", post(handle_chat_stream))
        .merge(terminal)
        .layer(axum::middleware::from_fn_with_state(
            hub.clone(),
            refuse_writes_when_readonly,
        ))
        .with_state(hub)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn sh(script: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new("sh");
        cmd.arg("-c").arg(script);
        cmd
    }

    /// Runs `f` on a thread and fails if it does not finish in time.
    fn finishes_within<T: Send + 'static>(secs: u64, f: impl FnOnce() -> T + Send + 'static) -> T {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(Duration::from_secs(secs))
            .expect("the child process run did not finish")
    }

    #[test]
    fn a_chatty_auth_cli_does_not_block_on_a_full_pipe() {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let seen = lines.clone();
        // 200 KiB on stdout and stderr, far beyond a pipe buffer.
        let status = finishes_within(20, move || {
            wait_streaming(
                sh("i=0; while [ $i -lt 4000 ]; do echo line-$i-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx; echo err-$i-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx >&2; i=$((i+1)); done"),
                Duration::from_secs(60),
                move |line| seen.lock().unwrap().push(line),
                || false,
            )
        })
        .unwrap();
        assert!(status.success());
        let lines = lines.lock().unwrap();
        assert!(lines.iter().any(|l| l.starts_with("line-3999-")));
        assert!(lines.iter().any(|l| l.starts_with("err-3999-")));
    }

    #[test]
    fn a_stuck_auth_cli_is_stopped_at_the_timeout() {
        let started = std::time::Instant::now();
        let result = finishes_within(20, || {
            wait_streaming(sh("sleep 60"), Duration::from_millis(300), |_| {}, || false)
        });
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn an_auth_cli_is_stopped_when_the_client_goes_away() {
        let result = finishes_within(20, || {
            wait_streaming(sh("sleep 60"), Duration::from_secs(60), |_| {}, || true)
        });
        assert!(result.is_err());
    }

    fn alive(pid: &str) -> bool {
        std::process::Command::new("kill")
            .arg("-0")
            .arg(pid)
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    async fn pid_from(file: &std::path::Path) -> String {
        for _ in 0..100 {
            if let Ok(text) = std::fs::read_to_string(file) {
                if !text.trim().is_empty() {
                    return text.trim().to_string();
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the child never wrote its pid");
    }

    fn sleeper(pid_file: &std::path::Path) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c")
            .arg(format!("echo $$ > '{}'; exec sleep 60", pid_file.display()));
        cmd
    }

    async fn gone(pid: &str) -> bool {
        for _ in 0..100 {
            if !alive(pid) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        false
    }

    #[tokio::test]
    async fn an_external_agent_run_is_killed_when_it_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pid");
        let run = tokio::spawn(output_with_timeout(sleeper(&file), Duration::from_secs(60)));
        let pid = pid_from(&file).await;
        run.abort();
        let _ = run.await;
        let stopped = gone(&pid).await;
        if !stopped {
            let _ = std::process::Command::new("kill").arg(&pid).status();
        }
        assert!(
            stopped,
            "the agent process kept running after its run was dropped"
        );
    }

    #[tokio::test]
    async fn an_external_agent_run_is_killed_at_the_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pid");
        let run = tokio::spawn(output_with_timeout(
            sleeper(&file),
            Duration::from_millis(500),
        ));
        let pid = pid_from(&file).await;
        let result = tokio::time::timeout(Duration::from_secs(10), run).await;
        let stopped = gone(&pid).await;
        if !stopped {
            let _ = std::process::Command::new("kill").arg(&pid).status();
        }
        assert!(
            matches!(result, Ok(Ok(Err(_)))),
            "the run did not end with an error at the timeout"
        );
        assert!(stopped);
    }
}
