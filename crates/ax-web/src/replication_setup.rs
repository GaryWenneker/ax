//! Local-owner setup of a supervised standalone node. No shell, arbitrary
//! config paths or remote-client control. Credentials exist only in memory and
//! the child environment; persisted settings contain the environment name.
use crate::{
    access_api::Actor,
    server_config::{NodeRole, ProjectRegistration, ServerConfig},
    workspace_state::WebHub,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::process::{Child, Command};

const TOKEN_ENV: &str = "AX_COMMAND_CENTER_REPLICATION_TOKEN";
const CONFIG_FILE: &str = "command-center-node.json";

#[derive(Default)]
pub(crate) struct ManagedNode {
    child: Option<Child>,
    runtime_file: Option<tempfile::NamedTempFile>,
    root: Option<PathBuf>,
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
        cluster_id: request.cluster_id.clone(),
        node_id,
        role: request.role,
        primary_url: request.primary_url.clone(),
        projects: vec![ProjectRegistration {
            id: request.project_id.clone(),
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
fn runtime_config(root: &Path, config: &ServerConfig) -> Result<tempfile::NamedTempFile, String> {
    let mut file = tempfile::NamedTempFile::new_in(root.join(".ax"))
        .map_err(|_| "Cannot stage runtime configuration")?;
    serde_json::to_writer(&mut file, config).map_err(|_| "Cannot write runtime configuration")?;
    Ok(file)
}
async fn running(node: &mut ManagedNode) -> bool {
    if let Some(child) = &mut node.child {
        match child.try_wait() {
            Ok(None) => return true,
            _ => {
                node.child = None;
                node.root = None;
                node.runtime_file = None;
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
    let active = running(&mut managed).await;
    let saved = match read_saved(&root) {
        Ok(saved) => saved,
        Err(error) => return fail(StatusCode::BAD_REQUEST, &error),
    };
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
    Json(serde_json::json!({"settings":saved,"running":active,"active_project":managed.root.as_ref().map(|p|p==&root).unwrap_or(false),"safety":{"mode":"preserve_and_exchange","memories":counts.0,"rules":counts.1,"skills":counts.2,"review_required":review,"sync_failed":failed}})).into_response()
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
    let mut managed = hub.replication.lock().await;
    if running(&mut managed).await {
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
    let supplied = request.token.as_deref().filter(|v| !v.is_empty());
    let generated = supplied.is_none() && previous.is_none() && request.role == NodeRole::Primary;
    let token = if generated {
        format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        )
    } else {
        supplied
            .map(str::to_owned)
            .or_else(|| std::env::var(TOKEN_ENV).ok())
            .unwrap_or_default()
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
                .arg(file.path())
                .env(TOKEN_ENV, &token)
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
    drop(probe);
    let mut command = Command::new(exe);
    command
        .args(["server", "run", "--config"])
        .arg(file.path())
        .args([
            "--bind",
            &format!("127.0.0.1:{}", saved.port),
            "--push-port",
            "0",
        ])
        .env(TOKEN_ENV, &token)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    if saved.config.role == NodeRole::Replica {
        command.arg("--auto-apply-memories");
    }
    if let Some(url) = saved.advertise.as_deref() {
        command.args(["--discovery-port", "9071", "--advertise", url]);
    }
    let mut child = match command.spawn() {
        Ok(v) => v,
        Err(_) => {
            return fail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot start standalone Ax",
            )
        }
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(400))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("valid local client");
    let mut ready = false;
    for _ in 0..40 {
        if child.try_wait().ok().flatten().is_some() {
            break;
        }
        if let Ok(response) = client
            .get(format!("http://127.0.0.1:{}/health", saved.port))
            .send()
            .await
        {
            if let Ok(body) = response.json::<serde_json::Value>().await {
                if body["node_id"].as_str() == Some(&saved.config.node_id) {
                    ready = true;
                    break;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    if !ready {
        let _ = child.kill().await;
        return fail(
            StatusCode::INTERNAL_SERVER_ERROR,
            "Node did not start. Check the port and Ax installation. Previous settings were kept.",
        );
    }
    // Keep the non-secret config alive for the replica's periodic reads.
    // It is replaced atomically only after the process has stopped.
    if let Err(e) = write_saved(&root, &saved) {
        let _ = child.kill().await;
        return fail(StatusCode::INTERNAL_SERVER_ERROR, &e);
    }
    // NamedTempFile is owned by the monitor, not detached from this service.
    managed.child = Some(child);
    managed.root = Some(root);
    // Periodic polling needs the runtime file for the child's whole lifetime.
    managed_runtime(&mut managed, file);
    Json(serde_json::json!({"started":true,"credential":if generated {Some(serde_json::json!({"token":token}))} else {None}})).into_response()
}

fn managed_runtime(node: &mut ManagedNode, file: tempfile::NamedTempFile) {
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
    let mut managed = hub.replication.lock().await;
    if let Some(mut child) = managed.child.take() {
        if child.kill().await.is_err() && child.try_wait().ok().flatten().is_none() {
            managed.child = Some(child);
            return fail(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Cannot stop managed node",
            );
        }
    }
    managed.root = None;
    managed.runtime_file = None;
    Json(serde_json::json!({"stopped":true})).into_response()
}

#[derive(Deserialize)]
pub(crate) struct DiscoveryRequest {
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
    if let Err(e) = config.validate() {
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
}
