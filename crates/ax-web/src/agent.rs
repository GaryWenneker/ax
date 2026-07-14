//! Agent terminal API — install, profiles, chat streaming.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;

use ax_agent::chat::chunk_text;
use ax_agent::config::{load_agents_config, save_agents_config, AgentsConfig};
use ax_agent::run_agent_turn;
use ax_agent::profiles::{
    create_profile, list_profiles, refresh_auth_statuses, remove_profile, set_active_profile,
    update_profile, AuthStatus,
};
use ax_installer::{
    auth_command, build_child_env, detect_cli_available, ensure_agent_ready, headless_command,
    install_cli_targets, install_selected, uninstall_targets, install_cli, TARGETS,
};
use axum::extract::{Path as AxPath, Query, State};
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

async fn handle_install(
    State(hub): State<WebHub>,
    Json(body): Json<InstallBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    drop(ws);
    match install_selected(&root, &body.targets) {
        Ok(summary) => Json(serde_json::json!({
            "ok": true,
            "reports": summary.reports.iter().map(|r| serde_json::json!({
                "id": r.id,
                "display_name": r.display_name,
                "files": r.files.iter().map(|f| f.path.to_string_lossy()).collect::<Vec<_>>(),
                "notes": r.notes,
            })).collect::<Vec<_>>(),
        }))
        .into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

async fn handle_install_stream(
    State(hub): State<WebHub>,
    Json(body): Json<InstallBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    let (tx, rx) = mpsc::unbounded_channel::<String>();
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    drop(ws);
    let targets = body.targets.clone();
    tokio::spawn(async move {
        let _ = tx.send(serde_json::json!({"type":"line","text":"Installing ax MCP for selected agents…"}).to_string());
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
                let _ = tx.send(serde_json::json!({"type":"done","ok":false,"error":e}).to_string());
            }
        }
    });
    sse_from_channel(rx).into_response()
}

async fn handle_cli_install_stream(
    State(hub): State<WebHub>,
    Json(body): Json<InstallBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
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
    Json(body): Json<InstallBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    match uninstall_targets(&body.targets) {
        Ok(reports) => Json(serde_json::json!({ "ok": true, "reports": reports.len() })).into_response(),
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
        return Json(serde_json::json!({ "ok": false, "error": "Profile not found" })).into_response();
    };
    if entry.data_dir.is_empty() {
        return Json(serde_json::json!({ "ok": false, "error": "Builtin profiles do not need auth" }))
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
        send(serde_json::json!({"type":"line","text": format!("Starting auth for {agent_id}/{profile_id}…")}));

        if !detect_cli_available(&agent_id) {
            send(serde_json::json!({"type":"line","text": format!("{} CLI not found — installing…", agent_id)}));
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
                        send(serde_json::json!({"type":"done","ok":false,"manual":true,"error":"CLI not available"}));
                        return;
                    }
                }
                Err(e) => {
                    send(serde_json::json!({"type":"line","text": format!("CLI install failed: {e}")}));
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
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        match cmd.spawn() {
            Ok(mut child) => {
                send(serde_json::json!({"type":"line","text":"Auth process started — complete login in the opened window or terminal."}));
                let status = child.wait();
                let ok = status.map(|s| s.success()).unwrap_or(false);
                if ok || ax_agent::profiles::detect_auth_status(&agent_id, &data_dir) == AuthStatus::Authenticated {
                    let _ = ax_agent::profiles::mark_authenticated(&agent_id, &profile_id);
                    send(serde_json::json!({"type":"line","text":"Profile marked authenticated."}));
                } else {
                    send(serde_json::json!({"type":"line","text":"Auth process ended — if you logged in successfully, click Mark authenticated."}));
                }
                send(serde_json::json!({"type":"done","ok":true}));
            }
            Err(e) => {
                send(serde_json::json!({"type":"line","text": format!("Could not start auth ({e}). Log in manually with the CLI, then click Mark authenticated.")}));
                send(serde_json::json!({"type":"done","ok":false,"manual":true,"error":e.to_string()}));
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
    Json(body): Json<ChatBody>,
) -> impl IntoResponse {
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

    let session_id = body
        .session_id
        .clone()
        .unwrap_or_else(|| format!("sess-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)));

    let mode = resolve_agent_mode(&cfg, &agent);
    let explicit_external = body.agent.as_deref().is_some_and(|a| a != "builtin")
        || cfg.terminal_mode == "external";
    let (tx, rx) = mpsc::unbounded_channel::<String>();

    let prompt = body.prompt.clone();
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    drop(ws);
    tokio::spawn(async move {
        let _ = tx.send(serde_json::json!({"type":"system","text": format!("Agent: {agent} · Profile: {profile_id} · Mode: {mode}")}).to_string());

        let explicit_external = explicit_external;
        if mode == "external" {
            let agent_for_ensure = agent.clone();
            let root_for_ensure = root.clone();
            let tx_ensure = tx.clone();
            let ensure_result = tokio::task::spawn_blocking(move || {
                ensure_agent_ready(&agent_for_ensure, &root_for_ensure, &mut |line| {
                    let _ = tx_ensure.send(serde_json::json!({"type":"system","text": line}).to_string());
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

async fn run_builtin_turn(tx: &mpsc::UnboundedSender<String>, prompt: &str, root: &PathBuf) {
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
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let output = cmd.output().await.map_err(|e| e.to_string())?;
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

pub fn router_hub(hub: WebHub) -> Router {
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
        .route("/profiles/{agent}/{id}/auth/stream", post(handle_auth_stream))
        .route("/profiles/{agent}/{id}/authenticated", post(handle_mark_authenticated))
        .route("/chat/stream", post(handle_chat_stream))
        .route("/pty/ws", get(crate::agent_pty::handle_pty_ws))
        .with_state(hub)
}
