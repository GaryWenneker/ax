//! Local-owner setup of a persistent standalone node. No shell, arbitrary
//! config paths or remote-client control. Credentials exist only in memory and
//! the child environment and OS credential store; JSON contains only settings.
use crate::{
    access_api::Actor,
    server_config::{NodeRole, ProjectRegistration, ServerConfig},
    workspace_state::WebHub,
};
use ax_mcp::replication_credentials::{self, Credentials};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::{Child, Command};

const TOKEN_ENV: &str = "AX_COMMAND_CENTER_REPLICATION_TOKEN";
const CONFIG_FILE: &str = "command-center-node.json";
const CONTROL_ENV: &str = "AX_COMMAND_CENTER_NODE_CONTROL";
const RUNTIME_FILE: &str = "command-center-node-runtime.json";

#[derive(Default)]
pub(crate) struct ManagedNode {
    child: Option<Child>,
    runtime_file: Option<PathBuf>,
    root: Option<PathBuf>,
    credential: Option<String>,
    control: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedNode {
    config: ServerConfig,
    port: u16,
    advertise: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetupRequest {
    role: NodeRole,
    cluster_id: String,
    project_id: String,
    primary_url: Option<String>,
    token: Option<String>,
    port: u16,
    advertise: Option<String>,
}

fn fail(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({"error":message}))).into_response()
}
fn local_owner(actor: &Actor) -> bool {
    actor.client_id.is_none() && actor.role == ax_global_db::access::MemberRole::Owner
}
fn read_saved(root: &Path) -> Result<Option<SavedNode>, String> {
    let path = root.join(".ax").join(CONFIG_FILE);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Cannot read node settings".into()),
        Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 16384 => {
            Err("Unsafe node settings file".into())
        }
        Ok(_) => {
            serde_json::from_slice(&std::fs::read(path).map_err(|_| "Cannot read node settings")?)
                .map(Some)
                .map_err(|_| "Invalid node settings".into())
        }
    }
}
fn write_saved(root: &Path, saved: &SavedNode) -> Result<(), String> {
    let mut file = tempfile::NamedTempFile::new_in(root.join(".ax"))
        .map_err(|_| "Cannot stage node settings")?;
    serde_json::to_writer(&mut file, saved).map_err(|_| "Cannot write node settings")?;
    file.as_file()
        .sync_all()
        .map_err(|_| "Cannot flush node settings")?;
    file.persist(root.join(".ax").join(CONFIG_FILE))
        .map_err(|_| "Cannot save node settings")?;
    Ok(())
}
fn validate_request(
    request: &SetupRequest,
    root: &Path,
    node_id: String,
) -> Result<SavedNode, String> {
    if request.port < 1024 {
        return Err("Choose a service port between 1024 and 65535".into());
    }
    let config = ServerConfig {
        schema_version: 1,
        cluster_id: setup_id(&request.cluster_id, request.role, "cluster"),
        node_id,
        role: request.role,
        primary_url: request.primary_url.clone(),
        projects: vec![ProjectRegistration {
            id: setup_id(&request.project_id, request.role, "project"),
            path: root.to_owned(),
            token_env: TOKEN_ENV.into(),
        }],
    };
    config.validate()?;
    if let Some(url) = request.advertise.as_deref() {
        crate::server_discovery::validate_advertise(&config, url)?;
        // Advertised primaries must be usable by the existing trusted-origin policy.
        let mut as_replica = config.clone();
        as_replica.role = NodeRole::Replica;
        as_replica.primary_url = Some(url.to_owned());
        as_replica.validate()?;
    }
    Ok(SavedNode {
        config,
        port: request.port,
        advertise: request.advertise.clone(),
    })
}
fn setup_id(value: &str, role: NodeRole, prefix: &str) -> String {
    let value = value.trim();
    if value.is_empty() && role == NodeRole::Primary {
        format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
    } else {
        value.to_owned()
    }
}
fn valid_token(token: &str) -> bool {
    token.len() >= 32
        && token.len() <= 4096
        && token.is_ascii()
        && !token
            .bytes()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
}
fn executable() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|_| "Cannot locate Ax executable")?;
    if exe.file_stem().and_then(|v| v.to_str()) != Some("ax") {
        return Err("Node setup requires Command Center started by the Ax CLI".into());
    }
    Ok(exe)
}
fn runtime_config(root: &Path, config: &ServerConfig) -> Result<PathBuf, String> {
    let mut file = tempfile::NamedTempFile::new_in(root.join(".ax"))
        .map_err(|_| "Cannot stage runtime configuration")?;
    serde_json::to_writer(&mut file, config).map_err(|_| "Cannot write runtime configuration")?;
    let path = root.join(".ax").join(RUNTIME_FILE);
    file.persist(&path)
        .map_err(|_| "Cannot save runtime configuration")?;
    Ok(path)
}

fn operation_lock(root: &Path) -> Result<std::fs::File, String> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join(".ax/command-center-node.lock"))
        .map_err(|_| "Cannot open replication lock")?;
    file.try_lock_exclusive()
        .map_err(|_| "Replication is being changed by another Ax process; retry shortly")?;
    Ok(file)
}

async fn stored_credentials(root: &Path, node: &str) -> Result<Credentials, String> {
    if let (Ok(token), Ok(control)) = (std::env::var(TOKEN_ENV), std::env::var(CONTROL_ENV)) {
        if valid_token(&token) && valid_token(&control) {
            return Ok(Credentials { token, control });
        }
    }
    let root = root.to_owned();
    let node = node.to_owned();
    tokio::task::spawn_blocking(move || replication_credentials::load(&root, &node))
        .await
        .map_err(|_| "Cannot access replication credential store")?
}

async fn save_credentials(
    root: &Path,
    node: &str,
    credentials: &Credentials,
) -> Result<Option<Option<Credentials>>, String> {
    if std::env::var(TOKEN_ENV).ok().as_deref() == Some(&credentials.token)
        && std::env::var(CONTROL_ENV).ok().as_deref() == Some(&credentials.control)
    {
        return Ok(None);
    }
    let root = root.to_owned();
    let node = node.to_owned();
    let credentials = credentials.clone();
    tokio::task::spawn_blocking(move || {
        replication_credentials::replace(&root, &node, &credentials)
    })
    .await
    .map_err(|_| "Cannot access replication credential store")?
    .map(Some)
}

fn local_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_millis(500))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Cannot create node control client".into())
}

async fn service_running(saved: &SavedNode, credentials: &Credentials) -> bool {
    let Ok(client) = local_client() else {
        return false;
    };
    let Ok(response) = client
        .post(format!("http://127.0.0.1:{}/managed/status", saved.port))
        .bearer_auth(&credentials.control)
        .send()
        .await
    else {
        return false;
    };
    if !response.status().is_success() {
        return false;
    }
    let Ok(body) = response.json::<serde_json::Value>().await else {
        return false;
    };
    body["node_id"].as_str() == Some(&saved.config.node_id)
        && body["cluster_id"].as_str() == Some(&saved.config.cluster_id)
}

fn node_command(
    root: &Path,
    saved: &SavedNode,
    credentials: &Credentials,
) -> Result<Command, String> {
    let runtime = runtime_config(root, &saved.config)?;
    let mut command = Command::new(executable()?);
    command
        .args(["server", "run", "--config"])
        .arg(runtime)
        .args([
            "--bind",
            &format!("127.0.0.1:{}", saved.port),
            "--push-port",
            "0",
            "--managed",
            "--discovery-port",
            "9071",
            "--advertise",
        ])
        .arg(
            saved
                .advertise
                .clone()
                .unwrap_or_else(|| format!("http://127.0.0.1:{}", saved.port)),
        )
        .env(TOKEN_ENV, &credentials.token)
        .env(CONTROL_ENV, &credentials.control)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false);
    // Closing a terminal or refreshing MCP must not deliver its shutdown signal
    // or retain its protocol pipes in the persistent replication service.
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    command.creation_flags(0x0000_0008 | 0x0000_0200); // Detached, new process group.
    if saved.config.role == NodeRole::Replica {
        command.arg("--auto-apply-memories");
    }
    Ok(command)
}

async fn wait_ready(child: &mut Child, saved: &SavedNode, credentials: &Credentials) -> bool {
    for _ in 0..40 {
        if child.try_wait().ok().flatten().is_some() {
            return false;
        }
        if service_running(saved, credentials).await {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

/// Reuse or start the saved service when web/MCP starts. Child lifetime is
/// independent of IDE refreshes, with authenticated local stop from Settings.
pub async fn auto_start(root: &Path) -> Result<(), String> {
    let Some(saved) = read_saved(root)? else {
        return Ok(());
    };
    saved.config.validate()?;
    let _lock = operation_lock(root)?;
    let credentials = stored_credentials(root, &saved.config.node_id).await?;
    if service_running(&saved, &credentials).await {
        return Ok(());
    }
    let probe = tokio::net::TcpListener::bind(("127.0.0.1", saved.port))
        .await
        .map_err(|_| "Replication service port is already in use")?;
    let mut command = node_command(root, &saved, &credentials)?;
    drop(probe);
    let mut child = command
        .spawn()
        .map_err(|_| "Cannot automatically start replication service")?;
    if !wait_ready(&mut child, &saved, &credentials).await {
        // This is the child we just spawned; no unrelated service is stopped.
        let _ = child.kill().await;
        return Err("Replication service did not start; saved configuration was preserved".into());
    }
    Ok(())
}
async fn running(node: &mut ManagedNode) -> bool {
    if let Some(child) = &mut node.child {
        match child.try_wait() {
            Ok(None) => return true,
            _ => {
                node.child = None;
                node.root = None;
                node.runtime_file = None;
                node.credential = None;
                node.control = None;
            }
        }
    }
    false
}

pub(crate) async fn status(
    State(hub): State<WebHub>,
    Extension(actor): Extension<Actor>,
) -> Response {
    if !local_owner(&actor) {
        return fail(
            StatusCode::FORBIDDEN,
            "Open the local owner Command Center to configure this node",
        );
    }
    let (root, pool) = {
        let state = hub.read().await;
        (state.project_root.clone(), state.graph_pool.clone())
    };
    let counts: (i64,i64,i64) = match sqlx::query_as("SELECT (SELECT COUNT(*) FROM memories),(SELECT COUNT(*) FROM policy_rules),(SELECT COUNT(*) FROM policy_skills)").fetch_one(&pool).await {
        Ok(counts)=>counts, Err(_)=>return fail(StatusCode::INTERNAL_SERVER_ERROR,"Cannot verify existing project data"),
    };
    let mut managed = hub.replication.lock().await;
    let owned_active = running(&mut managed).await;
    let saved = match read_saved(&root) {
        Ok(saved) => saved,
        Err(error) => return fail(StatusCode::BAD_REQUEST, &error),
    };
    let external_active = if let Some(saved) = &saved {
        match stored_credentials(&root, &saved.config.node_id).await {
            Ok(credentials) => service_running(saved, &credentials).await,
            Err(_) => false,
        }
    } else {
        false
    };
    let active = external_active || (owned_active && managed.root.as_ref() == Some(&root));
    let active_project = external_active || (owned_active && managed.root.as_ref() == Some(&root));
    let sync = saved.as_ref().and_then(|node| {
        if node.config.validate().is_err() {
            return None;
        }
        let path = root
            .join(".ax/server/sync-status")
            .join(&node.config.cluster_id)
            .join(format!("{}.json", node.config.projects.first()?.id));
        let meta = std::fs::symlink_metadata(&path).ok()?;
        if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 32768 {
            return None;
        }
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(path).ok()?).ok()
    });
    let sync_status = sync
        .as_ref()
        .and_then(|v| v.get("status"))
        .and_then(|v| v.as_str());
    let review = sync_status.is_some_and(|s| s.contains("review_required"));
    let failed = sync_status == Some("failed");
    Json(serde_json::json!({"settings":saved,"running":active,"active_project":active_project,"safety":{"mode":"preserve_and_exchange","memories":counts.0,"rules":counts.1,"skills":counts.2,"review_required":review,"sync_failed":failed}})).into_response()
}

pub(crate) async fn start(
    State(hub): State<WebHub>,
    Extension(actor): Extension<Actor>,
    Json(request): Json<SetupRequest>,
) -> Response {
    if !local_owner(&actor) {
        return fail(
            StatusCode::FORBIDDEN,
            "Only the local Owner can configure replication",
        );
    }
    let root = hub.read().await.project_root.clone();
    let _lock = match operation_lock(&root) {
        Ok(lock) => lock,
        Err(error) => return fail(StatusCode::CONFLICT, &error),
    };
    let mut managed = hub.replication.lock().await;
    if running(&mut managed).await && managed.root.as_ref() == Some(&root) {
        return fail(
            StatusCode::CONFLICT,
            "Stop the running managed node before changing its role or Primary",
        );
    }
    let previous = match read_saved(&root) {
        Ok(v) => v,
        Err(e) => return fail(StatusCode::BAD_REQUEST, &e),
    };
    let node_id = previous
        .as_ref()
        .map(|v| v.config.node_id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let saved = match validate_request(&request, &root, node_id) {
        Ok(v) => v,
        Err(e) => return fail(StatusCode::BAD_REQUEST, &e),
    };
    let old_credentials = stored_credentials(&root, &saved.config.node_id).await.ok();
    if previous
        .as_ref()
        .is_some_and(|old| old.config.node_id == saved.config.node_id)
    {
        if let (Some(old), Some(credentials)) = (&previous, &old_credentials) {
            if service_running(old, credentials).await {
                return fail(
                    StatusCode::CONFLICT,
                    "Stop the running managed service before changing configuration",
                );
            }
        }
    }
    let supplied = request.token.as_deref().filter(|v| !v.is_empty());
    let inherited = old_credentials
        .as_ref()
        .map(|c| c.token.clone())
        .or_else(|| std::env::var(TOKEN_ENV).ok());
    let generated = supplied.is_none()
        && inherited.is_none()
        && previous.is_none()
        && request.role == NodeRole::Primary;
    let token = if generated {
        format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )
    } else {
        supplied
            .map(str::to_owned)
            .or(inherited)
            .unwrap_or_default()
    };
    let credentials = Credentials {
        token: token.clone(),
        control: old_credentials
            .as_ref()
            .map(|c| c.control.clone())
            .unwrap_or_else(|| {
                format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                )
            }),
    };
    if !valid_token(&token) {
        return fail(StatusCode::BAD_REQUEST, "Enter the project's replication credential (32–4096 ASCII characters). Saved settings never store this credential.");
    }
    let exe = match executable() {
        Ok(v) => v,
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    // Do not terminate a separate CLI service or hijack an occupied port.
    let probe = match tokio::net::TcpListener::bind(("127.0.0.1", request.port)).await {
        Ok(v) => v,
        Err(_) => {
            return fail(
                StatusCode::CONFLICT,
                "Service port is already in use; stop that service or choose another port",
            )
        }
    };
    let file = match runtime_config(&root, &saved.config) {
        Ok(v) => v,
        Err(e) => return fail(StatusCode::INTERNAL_SERVER_ERROR, &e),
    };
    // Authenticate a proposed Primary and stage its verified snapshot before
    // changing persisted intent. This uses the existing bounded no-redirect protocol.
    if request.role == NodeRole::Replica {
        let sync = tokio::time::timeout(
            Duration::from_secs(35),
            Command::new(&exe)
                .args(["server", "apply", "--config"])
                .arg(&file)
                .env(TOKEN_ENV, &token)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .status(),
        )
        .await;
        if !matches!(sync, Ok(Ok(result)) if result.success()) {
            return fail(StatusCode::BAD_REQUEST, "Primary connection failed. Check its URL, cluster, project ID and replication credential; both nodes must support preserve-and-exchange. Existing local data and previous settings were kept.");
        }
    }
    let mut command = match node_command(&root, &saved, &credentials) {
        Ok(command) => command,
        Err(error) => return fail(StatusCode::INTERNAL_SERVER_ERROR, &error),
    };
    drop(probe);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            return fail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot start standalone Ax",
            )
        }
    };
    let ready = wait_ready(&mut child, &saved, &credentials).await;
    if !ready {
        let _ = child.kill().await;
        return fail(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Node did not start. Check the port and Ax installation. Previous settings were kept.",
        );
    }
    let previous_store = match save_credentials(&root, &saved.config.node_id, &credentials).await {
        Ok(previous) => previous,
        Err(error) => {
            let _ = child.kill().await; // Only the child spawned by this request is stopped.
            return fail(StatusCode::INTERNAL_SERVER_ERROR, &error);
        }
    };
    // Keep the non-secret config alive for the replica's periodic reads.
    // It is replaced atomically only after the process has stopped.
    if let Err(e) = write_saved(&root, &saved) {
        let _ = child.kill().await;
        if let Some(previous_store) = previous_store {
            let credential_root = root.clone();
            let node = saved.config.node_id.clone();
            let rollback = tokio::task::spawn_blocking(move || match previous_store {
                Some(old) => replication_credentials::save(&credential_root, &node, &old),
                None => replication_credentials::remove(&credential_root, &node),
            })
            .await;
            if !matches!(rollback, Ok(Ok(()))) {
                return fail(StatusCode::INTERNAL_SERVER_ERROR, "Cannot save settings or restore the previous credential; reconnect before restarting replication");
            }
        }
        return fail(StatusCode::INTERNAL_SERVER_ERROR, &e);
    }
    // Keep a handle for immediate shutdown; the service survives web/MCP exit.
    managed.child = Some(child);
    managed.root = Some(root);
    managed.credential = Some(token.clone());
    managed.control = Some(credentials.control);
    // Periodic polling needs the runtime file for the child's whole lifetime.
    managed_runtime(&mut managed, file);
    Json(serde_json::json!({"started":true,"credential":if generated {Some(serde_json::json!({"token":token}))} else {None}})).into_response()
}

fn managed_runtime(node: &mut ManagedNode, file: PathBuf) {
    node.runtime_file = Some(file);
}

pub(crate) async fn stop(
    State(hub): State<WebHub>,
    Extension(actor): Extension<Actor>,
) -> Response {
    if !local_owner(&actor) {
        return fail(
            StatusCode::FORBIDDEN,
            "Only the local Owner can stop replication",
        );
    }
    let root = hub.read().await.project_root.clone();
    let _lock = match operation_lock(&root) {
        Ok(lock) => lock,
        Err(error) => return fail(StatusCode::CONFLICT, &error),
    };
    let mut managed = hub.replication.lock().await;
    if let Err(error) = stop_service(&root, &mut managed).await {
        return fail(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    Json(serde_json::json!({"stopped":true})).into_response()
}

async fn stop_service(root: &Path, managed: &mut ManagedNode) -> Result<(), String> {
    if let Some(saved) = read_saved(root)? {
        if let Ok(credentials) = stored_credentials(root, &saved.config.node_id).await {
            if service_running(&saved, &credentials).await {
                let response = local_client()?
                    .post(format!("http://127.0.0.1:{}/managed/stop", saved.port))
                    .bearer_auth(&credentials.control)
                    .send()
                    .await
                    .map_err(|_| "Cannot stop managed service")?;
                if !response.status().is_success() {
                    return Err("Managed service refused shutdown".into());
                }
                for _ in 0..50 {
                    if !service_running(&saved, &credentials).await {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                if service_running(&saved, &credentials).await {
                    return Err("Managed service is still stopping; retry shortly".into());
                }
            }
        }
    }
    if managed.root.as_deref() == Some(root) {
        stop_child(managed).await?;
    }
    if let Some(saved) = read_saved(root)? {
        // Do not remove intent while an unauthenticated or still-stopping service
        // occupies this port. In particular, never kill an unrelated CLI service.
        let probe = tokio::net::TcpListener::bind(("127.0.0.1", saved.port)).await
            .map_err(|_| "Service port remains occupied; unlock the credential store or stop that service before removing configuration")?;
        drop(probe);
    }
    Ok(())
}

async fn stop_child(managed: &mut ManagedNode) -> Result<(), String> {
    if let Some(mut child) = managed.child.take() {
        if child.kill().await.is_err() && child.try_wait().ok().flatten().is_none() {
            managed.child = Some(child);
            return Err("Cannot stop managed node".into());
        }
    }
    managed.root = None;
    managed.runtime_file = None;
    managed.credential = None;
    managed.control = None;
    Ok(())
}

pub(crate) async fn credential(
    State(hub): State<WebHub>,
    Extension(actor): Extension<Actor>,
) -> Response {
    if !local_owner(&actor) {
        return fail(
            StatusCode::FORBIDDEN,
            "Only the local Owner can view the replication credential",
        );
    }
    let root = hub.read().await.project_root.clone();
    let saved = match read_saved(&root) {
        Ok(Some(saved)) => saved,
        _ => return fail(StatusCode::NOT_FOUND, "No local replication configuration"),
    };
    match stored_credentials(&root, &saved.config.node_id).await {
        Ok(credentials) => Json(serde_json::json!({"token":credentials.token})).into_response(),
        Err(error) => fail(StatusCode::CONFLICT, &error),
    }
}

fn remove_saved(root: &Path) -> Result<(), String> {
    // Apply the same file/symlink bounds as configuration reads before deletion.
    if read_saved(root)?.is_some() {
        std::fs::remove_file(root.join(".ax").join(CONFIG_FILE))
            .map_err(|_| "Cannot remove local replication configuration")?;
        let runtime = root.join(".ax").join(RUNTIME_FILE);
        if runtime.is_file() {
            std::fs::remove_file(runtime).map_err(|_| "Cannot remove runtime configuration")?;
        }
    }
    Ok(())
}

pub(crate) async fn remove(
    State(hub): State<WebHub>,
    Extension(actor): Extension<Actor>,
) -> Response {
    if !local_owner(&actor) {
        return fail(
            StatusCode::FORBIDDEN,
            "Only the local Owner can remove replication configuration",
        );
    }
    let root = hub.read().await.project_root.clone();
    let _lock = match operation_lock(&root) {
        Ok(lock) => lock,
        Err(error) => return fail(StatusCode::CONFLICT, &error),
    };
    let saved = match read_saved(&root) {
        Ok(saved) => saved,
        Err(error) => return fail(StatusCode::BAD_REQUEST, &error),
    };
    let mut managed = hub.replication.lock().await;
    if let Err(error) = stop_service(&root, &mut managed).await {
        return fail(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    if let Some(saved) = saved {
        let env_credentials =
            std::env::var(TOKEN_ENV).is_ok() && std::env::var(CONTROL_ENV).is_ok();
        if !env_credentials {
            let node = saved.config.node_id;
            let credential_root = root.clone();
            match tokio::task::spawn_blocking(move || {
                replication_credentials::remove(&credential_root, &node)
            })
            .await
            {
                Ok(Ok(())) => {}
                _ => {
                    return fail(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Cannot remove stored replication credential; configuration was kept",
                    )
                }
            }
        }
    }
    if let Err(error) = remove_saved(&root) {
        return fail(StatusCode::INTERNAL_SERVER_ERROR, &error);
    }
    Json(serde_json::json!({"removed":true})).into_response()
}

#[derive(Deserialize)]
pub(crate) struct DiscoveryRequest {
    #[serde(default)]
    cluster_id: String,
}
pub(crate) async fn discover(
    State(hub): State<WebHub>,
    Extension(actor): Extension<Actor>,
    Json(request): Json<DiscoveryRequest>,
) -> Response {
    if !local_owner(&actor) {
        return fail(
            StatusCode::FORBIDDEN,
            "Only the local Owner can discover nodes",
        );
    }
    let root = hub.read().await.project_root.clone();
    let config = ServerConfig {
        schema_version: 1,
        cluster_id: request.cluster_id,
        node_id: uuid::Uuid::new_v4().to_string(),
        role: NodeRole::Primary,
        primary_url: None,
        projects: vec![ProjectRegistration {
            id: "discovery".into(),
            path: root,
            token_env: TOKEN_ENV.into(),
        }],
    };
    // A blank filter discovers every available cluster; it never grants trust.
    let mut validation = config.clone();
    if validation.cluster_id.is_empty() {
        validation.cluster_id = "discovery".into();
    }
    if let Err(e) = validation.validate() {
        return fail(StatusCode::BAD_REQUEST, &e);
    }
    match crate::server_discovery::discover_candidates(&config, 9071, 12).await {
        Ok(result) => Json(result).into_response(),
        Err(_) => fail(StatusCode::CONFLICT, "LAN discovery unavailable (UDP 9071 may be in use). You can enter the Primary URL directly."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(role: NodeRole) -> SetupRequest {
        SetupRequest {
            role,
            cluster_id: "team".into(),
            project_id: "project".into(),
            primary_url: None,
            token: None,
            port: 9070,
            advertise: None,
        }
    }
    #[test]
    fn local_owner_control_cannot_be_granted_to_client_tokens_or_admins() {
        let mut actor = Actor {
            member_id: "owner".into(),
            client_id: None,
            role: ax_global_db::access::MemberRole::Owner,
        };
        assert!(local_owner(&actor));
        actor.client_id = Some("client".into());
        assert!(!local_owner(&actor));
        actor.client_id = None;
        actor.role = ax_global_db::access::MemberRole::Admin;
        assert!(!local_owner(&actor));
    }
    #[test]
    fn pairing_requires_an_explicit_safe_origin_and_initialized_current_project() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".ax")).unwrap();
        let mut req = request(NodeRole::Replica);
        assert!(validate_request(&req, dir.path(), "node".into()).is_err());
        req.primary_url = Some("https://primary.example.com".into());
        assert!(validate_request(&req, dir.path(), "node".into()).is_ok());
        for origin in [
            "http://example.com",
            "https://user:secret@example.com",
            "https://example.com/path",
        ] {
            req.primary_url = Some(origin.into());
            assert!(validate_request(&req, dir.path(), "node".into()).is_err());
        }
    }
    #[test]
    fn durable_configuration_preserves_identity_without_credentials() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".ax")).unwrap();
        let saved = validate_request(
            &request(NodeRole::Primary),
            dir.path(),
            "stable-node".into(),
        )
        .unwrap();
        write_saved(dir.path(), &saved).unwrap();
        let again = read_saved(dir.path()).unwrap().unwrap();
        assert_eq!(again.config.node_id, "stable-node");
        let text = std::fs::read_to_string(dir.path().join(".ax").join(CONFIG_FILE)).unwrap();
        assert!(!text.contains("credential"));
        assert!(text.contains(TOKEN_ENV));
    }
    #[test]
    fn credentials_are_bounded_and_no_control_characters_reach_the_environment() {
        assert!(valid_token(&"a".repeat(32)));
        assert!(!valid_token("short"));
        assert!(!valid_token(&format!("{}\0", "a".repeat(32))));
        assert!(!valid_token(&"a".repeat(4097)));
    }
    #[test]
    fn empty_primary_ids_are_generated_but_replicas_require_existing_ids() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".ax")).unwrap();
        let mut req = request(NodeRole::Primary);
        req.cluster_id = " ".into();
        req.project_id.clear();
        let saved = validate_request(&req, dir.path(), "node".into()).unwrap();
        assert!(saved.config.cluster_id.starts_with("cluster_"));
        assert!(saved.config.projects[0].id.starts_with("project_"));
        assert!(saved.config.validate().is_ok());
        req.role = NodeRole::Replica;
        req.primary_url = Some("https://primary.example.com".into());
        assert!(validate_request(&req, dir.path(), "node".into()).is_err());
    }
    #[test]
    fn removing_configuration_preserves_project_data_and_allows_setup_again() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join(".ax");
        std::fs::create_dir(&data).unwrap();
        std::fs::write(data.join("ax.db"), b"existing project data").unwrap();
        let saved =
            validate_request(&request(NodeRole::Primary), dir.path(), "node".into()).unwrap();
        write_saved(dir.path(), &saved).unwrap();
        runtime_config(dir.path(), &saved.config).unwrap();
        remove_saved(dir.path()).unwrap();
        remove_saved(dir.path()).unwrap();
        assert!(read_saved(dir.path()).unwrap().is_none());
        assert_eq!(
            std::fs::read(data.join("ax.db")).unwrap(),
            b"existing project data"
        );
        assert!(!data.join(RUNTIME_FILE).exists());
    }
    #[test]
    fn concurrent_node_start_or_removal_is_serialized_across_processes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".ax")).unwrap();
        let first = operation_lock(dir.path()).unwrap();
        assert!(operation_lock(dir.path()).is_err());
        drop(first);
        assert!(operation_lock(dir.path()).is_ok());
    }
}
