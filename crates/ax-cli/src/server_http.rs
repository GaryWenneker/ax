//! Minimal authenticated HTTP transport for standalone Ax snapshots.
//!
//! The listener defaults to loopback. TLS termination for hosted deployments
//! belongs to a reverse proxy; bind a private interface only by explicit opt-in.

use crate::{
    server_cli,
    server_config::{NodeRole, ServerConfig},
    server_discovery, server_push, server_snapshot,
};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path as AxumPath, State},
    http::{header, HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use sqlx::SqlitePool;
use std::{
    net::{IpAddr, SocketAddr},
    path::Path,
    sync::Arc,
};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;

struct StateData {
    config: ServerConfig,
    access_db: SqlitePool,
    exports: Semaphore,
    control: Option<String>,
    shutdown: Option<tokio::sync::watch::Sender<bool>>,
}

fn project_bootstrap_token(config: &ServerConfig, project_id: &str) -> Option<String> {
    let project = config
        .projects
        .iter()
        .find(|project| project.id == project_id)?;
    let token = std::env::var(&project.token_env).ok()?;
    if token.len() < 32 || !token.is_ascii() || token.bytes().any(|b| b.is_ascii_whitespace()) {
        return None;
    }
    Some(token)
}

fn exact_secret(candidate: &str, expected: &str) -> bool {
    candidate.len() == expected.len() && bool::from(candidate.as_bytes().ct_eq(expected.as_bytes()))
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty())
}

async fn health(State(state): State<Arc<StateData>>) -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"status":"ok", "node_id":state.config.node_id, "role":state.config.role}),
    )
}

fn authorize_control(state: &StateData, headers: &HeaderMap) -> Result<(), StatusCode> {
    let expected = state.control.as_deref().ok_or(StatusCode::NOT_FOUND)?;
    let candidate = bearer_token(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    if !exact_secret(candidate, expected) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(())
}

async fn managed_status(
    State(state): State<Arc<StateData>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    authorize_control(&state, &headers)?;
    Ok(Json(
        serde_json::json!({"node_id":state.config.node_id,"cluster_id":state.config.cluster_id,"role":state.config.role}),
    ))
}

async fn managed_stop(
    State(state): State<Arc<StateData>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    authorize_control(&state, &headers)?;
    state
        .shutdown
        .as_ref()
        .ok_or(StatusCode::NOT_FOUND)?
        .send(true)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(serde_json::json!({"stopped":true})))
}

async fn authorize_project(
    state: &StateData,
    id: &str,
    headers: &HeaderMap,
) -> Result<String, StatusCode> {
    if state.config.role != NodeRole::Primary {
        return Err(StatusCode::NOT_FOUND);
    }
    if !state.config.projects.iter().any(|project| project.id == id) {
        return Err(StatusCode::NOT_FOUND);
    }
    let token = bearer_token(headers).ok_or(StatusCode::UNAUTHORIZED)?;
    let node_id = headers
        .get("x-ax-node-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 128
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        });

    let principal =
        ax_global_db::access::authenticate_node_token_for_project(&state.access_db, token, id)
            .await
            .map_err(|error| {
                tracing::warn!(project = %id, "node authentication failed: {}", error);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

    if let Some(principal) = &principal {
        if node_id.is_some_and(|id| id != principal.node_id) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    if principal.is_none() {
        let Some(node_id) = node_id else {
            return Err(StatusCode::UNAUTHORIZED);
        };
        let Some(expected) = project_bootstrap_token(&state.config, id) else {
            return Err(StatusCode::UNAUTHORIZED);
        };
        if !exact_secret(token, &expected) {
            return Err(StatusCode::UNAUTHORIZED);
        }
        ax_global_db::access::register_trusted_replica(
            &state.access_db,
            node_id,
            node_id,
            None,
            id,
        )
        .await
        .map_err(|error| {
            tracing::warn!(project = %id, node = %node_id, "replica registration failed: {}", error);
            if error.to_string() == "replica is blocked" { StatusCode::FORBIDDEN } else { StatusCode::INTERNAL_SERVER_ERROR }
        })?;
    }
    principal
        .as_ref()
        .map(|p| p.node_id.clone())
        .or_else(|| node_id.map(str::to_owned))
        .ok_or(StatusCode::UNAUTHORIZED)
}

async fn snapshot(
    State(state): State<Arc<StateData>>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<server_snapshot::Snapshot>, StatusCode> {
    let node_id = authorize_project(&state, &id, &headers).await?;
    let _permit = state
        .exports
        .acquire()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let result = server_snapshot::export(&state.config, &id)
        .await
        .map_err(|error| {
            tracing::warn!(project = %id, "snapshot export failed: {}", error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    sqlx::query("UPDATE replication_nodes SET last_seen_at = CURRENT_TIMESTAMP WHERE id = ? AND trust_state = 'trusted'")
        .bind(node_id).execute(&state.access_db).await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(result))
}

async fn exchange(
    State(state): State<Arc<StateData>>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
    Json(incoming): Json<server_snapshot::Snapshot>,
) -> Result<Json<crate::server_exchange::ExchangeReply>, StatusCode> {
    let node_id = authorize_project(&state, &id, &headers).await?;
    if incoming.primary_node_id != node_id {
        return Err(StatusCode::FORBIDDEN);
    }
    if incoming.schema_version != 3 {
        return Err(StatusCode::CONFLICT);
    }
    server_snapshot::validate(&incoming, &state.config.cluster_id, &id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let _permit = state
        .exports
        .acquire()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let outcome = async {
        let snapshot = crate::server_exchange::publish(&state.config, &id, Some(&incoming)).await?;
        crate::server_data::apply(&state.config, &snapshot).await?;
        let report = crate::server_apply::apply(&state.config, &snapshot).await?;
        Ok::<_, String>((snapshot, report))
    }
    .await;
    if let Some(project) = state.config.projects.iter().find(|p| p.id == id) {
        let status = outcome
            .as_ref()
            .map(|(snapshot, _)| {
                (
                    "full_data_synced; two_way_preserved".into(),
                    snapshot.revision.clone(),
                )
            })
            .map_err(Clone::clone);
        crate::server_replica::record_attempt(&state.config, project, &status)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    let (snapshot, report) = outcome.map_err(|error| {
        tracing::warn!(project=%id,"complete node synchronization failed: {}",error);
        StatusCode::CONFLICT
    })?;
    sqlx::query("UPDATE replication_nodes SET last_sync_at=CURRENT_TIMESTAMP,last_seen_at=CURRENT_TIMESTAMP WHERE id=? AND trust_state='trusted'").bind(&node_id).execute(&state.access_db).await.map_err(|_|StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(crate::server_exchange::ExchangeReply {
        snapshot,
        contributed_revision: incoming.revision,
        memory_conflicts: report.conflicts,
    }))
}

async fn enrollment(
    State(state): State<Arc<StateData>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let mut projects = Vec::new();
    let mut node = None;
    for project in &state.config.projects {
        if let Ok(id) = authorize_project(&state, &project.id, &headers).await {
            projects.push(project.id.clone());
            node = Some(id);
        }
    }
    if projects.is_empty() {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(Json(
        serde_json::json!({"protocol_version":3,"node_id":node,"cluster_id":state.config.cluster_id,"project_ids":projects,"datasets":["graph","source","logs","agent_history","working_context","durable_sessions","structured_logs","memories","rules","skills"]}),
    ))
}
async fn chunk_read(
    State(state): State<Arc<StateData>>,
    AxumPath((id, hash)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> Result<Bytes, StatusCode> {
    authorize_project(&state, &id, &headers).await?;
    let root = state
        .config
        .projects
        .iter()
        .find(|p| p.id == id)
        .ok_or(StatusCode::NOT_FOUND)?
        .path
        .clone();
    let bytes = tokio::task::spawn_blocking(move || ax_db::replication::read_chunk(&root, &hash))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::NOT_FOUND)?;
    Ok(Bytes::from(bytes))
}
async fn chunk_write(
    State(state): State<Arc<StateData>>,
    AxumPath((id, hash)): AxumPath<(String, String)>,
    headers: HeaderMap,
    bytes: Bytes,
) -> Result<StatusCode, StatusCode> {
    authorize_project(&state, &id, &headers).await?;
    if bytes.len() > ax_db::replication::CHUNK_BYTES {
        return Err(StatusCode::PAYLOAD_TOO_LARGE);
    }
    let _permit = state
        .exports
        .acquire()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let root = state
        .config
        .projects
        .iter()
        .find(|p| p.id == id)
        .ok_or(StatusCode::NOT_FOUND)?
        .path
        .clone();
    tokio::task::spawn_blocking(move || ax_db::replication::store_chunk(&root, &hash, &bytes))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(StatusCode::CREATED)
}

fn private_bind(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local() || ip.is_unspecified(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local() || ip.is_unspecified(),
    }
}

fn public_routes() -> Router<Arc<StateData>> {
    let chunks = Router::new()
        .route(
            "/v1/projects/{id}/chunks/{hash}",
            get(chunk_read).put(chunk_write),
        )
        .layer(DefaultBodyLimit::max(ax_db::replication::CHUNK_BYTES));
    Router::new()
        .route("/health", get(health))
        .route("/v1/enrollment", post(enrollment))
        .route("/v1/projects/{id}/snapshot", get(snapshot))
        .route("/v1/projects/{id}/exchange", post(exchange))
        .layer(DefaultBodyLimit::max(
            server_snapshot::MAX_SNAPSHOT_BYTES + 1024 * 1024,
        ))
        .merge(chunks)
}

pub struct RunOptions {
    pub bind: SocketAddr,
    pub allow_private_http: bool,
    pub discovery_port: Option<u16>,
    pub advertise: Option<String>,
    pub sync_interval_seconds: Option<u64>,
    pub push_port: u16,
    pub auto_apply_memories: bool,
    pub managed: bool,
}

/// Start the listener and serve until the process receives shutdown.
pub async fn run(path: &Path, options: RunOptions) -> Result<(), String> {
    let RunOptions {
        bind,
        allow_private_http,
        discovery_port,
        advertise,
        sync_interval_seconds,
        push_port,
        auto_apply_memories,
        managed,
    } = options;
    let config = server_cli::load(path)?;
    let control = if managed {
        let token = std::env::var("AX_COMMAND_CENTER_NODE_CONTROL")
            .map_err(|_| "managed node control credential is missing")?;
        if token.len() < 32
            || token.len() > 4096
            || !token.is_ascii()
            || token
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        {
            return Err("managed node control credential is invalid".into());
        }
        Some(token)
    } else {
        None
    };
    if auto_apply_memories && config.role != NodeRole::Replica {
        return Err("--auto-apply-memories is only valid for replicas".into());
    }
    if let Some(seconds) = sync_interval_seconds {
        if config.role != NodeRole::Replica {
            return Err("--sync-interval-seconds is only valid for replicas".into());
        }
        if !(5..=86_400).contains(&seconds) {
            return Err("--sync-interval-seconds must be between 5 and 86400".into());
        }
    }
    if discovery_port.is_some() != advertise.is_some() {
        return Err("LAN discovery requires both --discovery-port and --advertise".into());
    }
    if let Some(url) = advertise.as_deref() {
        server_discovery::validate_advertise(&config, url)?;
    }
    if !bind.ip().is_loopback() && (!allow_private_http || !private_bind(bind.ip())) {
        return Err("non-loopback bind requires --allow-private-http on a private network; terminate TLS at the reverse proxy".into());
    }
    let access_db_path = ax_global_db::global_db_path().map_err(|e| e.to_string())?;
    let access_db = ax_global_db::open_and_init(&access_db_path)
        .await
        .map_err(|e| e.to_string())?;
    ax_global_db::access::record_local_node(
        &access_db,
        &config.node_id,
        if config.role == NodeRole::Primary {
            "primary"
        } else {
            "replica"
        },
        config.primary_url.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;
    let announcement = discovery_port.zip(advertise).map(|(port, url)| {
        let node = config.clone();
        tokio::spawn(async move {
            if let Err(error) = server_discovery::announce(node, url, port).await {
                tracing::warn!("discovery announcement stopped: {}", error);
            }
        })
    });
    // UDP notifications can be lost. All replicas have periodic authenticated
    // polling enabled as a fallback even if no --sync-interval-seconds is set.
    // Discovery is never allowed to change the configured primary URL.
    let polling_interval = if config.role == NodeRole::Replica {
        Some(sync_interval_seconds.unwrap_or(60))
    } else {
        None
    };
    let periodic_sync = polling_interval.map(|seconds| {
        let config_path = path.to_path_buf();
        tokio::spawn(async move {
            loop {
                if let Err(error) =
                    crate::server_replica::sync_with_apply(&config_path, auto_apply_memories).await
                {
                    tracing::warn!("periodic replica synchronization failed: {}", error);
                }
                tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
            }
        })
    });
    // A signed UDP registration records the replica's actual source socket.
    // The primary sends signed change hints to live registrations only.
    let push_task = if push_port == 0 {
        None
    } else {
        let node = config.clone();
        let config_path = path.to_path_buf();
        Some(tokio::spawn(async move {
            let result = match node.role {
                NodeRole::Primary => server_push::primary(node, push_port).await,
                NodeRole::Replica => {
                    server_push::replica(node, config_path, push_port, auto_apply_memories).await
                }
            };
            if let Err(error) = result {
                tracing::warn!(
                    "UDP push unavailable; HTTPS polling remains active: {}",
                    error
                );
            }
        }))
    };
    let (shutdown, mut shutdown_rx) = tokio::sync::watch::channel(false);
    let state = Arc::new(StateData {
        config,
        access_db,
        exports: Semaphore::new(4),
        control,
        shutdown: managed.then_some(shutdown.clone()),
    });
    let public = public_routes();
    let controlled = public
        .clone()
        .route("/managed/status", post(managed_status))
        .route("/managed/stop", post(managed_stop));
    let separate_control = managed && bind.ip() != IpAddr::V4(std::net::Ipv4Addr::LOCALHOST);
    let app = if separate_control {
        public.clone()
    } else {
        controlled.clone()
    }
    .with_state(state.clone());
    let control_task = if separate_control {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", bind.port()))
            .await
            .map_err(|e| format!("Cannot bind local control: {e}"))?;
        let app = controlled.with_state(state.clone());
        let mut receiver = shutdown.subscribe();
        Some(tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = receiver.changed().await;
                })
                .await
        }))
    } else {
        None
    };
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "Ax standalone server listening on {}",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    // The detached node outlives web/MCP, so it must keep each registered
    // project's local index synchronized even after those frontends close.
    let sync_tasks: Vec<_> = state
        .config
        .projects
        .iter()
        .map(|project| ax_core::Ax::spawn_background_watch(project.path.clone()))
        .collect();
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            if managed { tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = shutdown_rx.changed() => {} } }
            else { let _ = tokio::signal::ctrl_c().await; }
        })
        .await
        .map_err(|e| e.to_string());
    let _ = shutdown.send(true);
    if let Some(task) = control_task {
        let _ = task.await;
    }
    if let Some(task) = announcement {
        task.abort();
    }
    if let Some(task) = periodic_sync {
        task.abort();
    }
    if let Some(task) = push_task {
        task.abort();
    }
    for task in sync_tasks {
        task.abort();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_auth_extracts_only_bearer_tokens() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer axn_test".parse().unwrap());
        assert_eq!(bearer_token(&headers), Some("axn_test"));
        headers.insert(header::AUTHORIZATION, "Basic axn_test".parse().unwrap());
        assert_eq!(bearer_token(&headers), None);
    }

    #[test]
    fn public_ips_are_not_private_bind_targets() {
        assert!(!private_bind("8.8.8.8".parse().unwrap()));
        assert!(private_bind("192.168.1.5".parse().unwrap()));
        assert!(private_bind("fd00::1".parse().unwrap()));
    }

    #[tokio::test]
    async fn managed_control_requires_its_own_secret_and_is_disabled_for_cli_servers() {
        let (shutdown, mut receiver) = tokio::sync::watch::channel(false);
        let mut state = StateData {
            config: ServerConfig {
                schema_version: 1,
                cluster_id: "team".into(),
                node_id: "primary".into(),
                role: NodeRole::Primary,
                primary_url: None,
                projects: vec![],
            },
            access_db: sqlx::sqlite::SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .unwrap(),
            exports: Semaphore::new(4),
            control: Some("local-control-secret".into()),
            shutdown: Some(shutdown),
        };
        let mut headers = HeaderMap::new();
        assert_eq!(
            authorize_control(&state, &headers),
            Err(StatusCode::UNAUTHORIZED)
        );
        headers.insert(
            header::AUTHORIZATION,
            "Bearer replication-credential".parse().unwrap(),
        );
        assert_eq!(
            authorize_control(&state, &headers),
            Err(StatusCode::UNAUTHORIZED)
        );
        assert!(!*receiver.borrow());
        headers.insert(
            header::AUTHORIZATION,
            "Bearer local-control-secret".parse().unwrap(),
        );
        let result = managed_stop(State(Arc::new(state)), headers.clone()).await;
        assert!(result.is_ok());
        receiver.changed().await.unwrap();
        assert!(*receiver.borrow());
        state = StateData {
            config: ServerConfig {
                schema_version: 1,
                cluster_id: "team".into(),
                node_id: "ordinary".into(),
                role: NodeRole::Primary,
                primary_url: None,
                projects: vec![],
            },
            access_db: sqlx::sqlite::SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .unwrap(),
            exports: Semaphore::new(4),
            control: None,
            shutdown: None,
        };
        assert_eq!(
            authorize_control(&state, &headers),
            Err(StatusCode::NOT_FOUND)
        );
    }

    #[tokio::test]
    async fn exchange_binds_authenticated_node_and_project_before_retaining_data() {
        use crate::server_config::ProjectRegistration;
        let dir = tempfile::tempdir().unwrap();
        let ax = ax_core::Ax::init(dir.path()).await.unwrap();
        let config = ServerConfig {
            schema_version: 1,
            cluster_id: "team".into(),
            node_id: "primary".into(),
            role: NodeRole::Primary,
            primary_url: None,
            projects: vec![ProjectRegistration {
                id: "project".into(),
                path: dir.path().into(),
                token_env: "AX_TEST_UNUSED_ENROLLMENT".into(),
            }],
        };
        let db = ax_global_db::open_and_init(&dir.path().join("access.db"))
            .await
            .unwrap();
        ax_global_db::access::register_trusted_replica(&db, "replica", "Replica", None, "project")
            .await
            .unwrap();
        let credential = ax_global_db::access::issue_node_credential(&db, "replica", None)
            .await
            .unwrap();
        let state = Arc::new(StateData {
            config: config.clone(),
            access_db: db.clone(),
            exports: Semaphore::new(4),
            control: None,
            shutdown: None,
        });
        let mut other = config.clone();
        other.role = NodeRole::Replica;
        other.node_id = "other".into();
        let incoming = server_snapshot::export_native(&other, "project")
            .await
            .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", credential.token).parse().unwrap(),
        );
        headers.insert("x-ax-node-id", "replica".parse().unwrap());
        let forged = exchange(
            State(state.clone()),
            AxumPath("project".into()),
            headers.clone(),
            Json(incoming.clone()),
        )
        .await;
        assert!(matches!(forged, Err(StatusCode::FORBIDDEN)));
        let wrong_project = exchange(
            State(state.clone()),
            AxumPath("other-project".into()),
            headers.clone(),
            Json(incoming.clone()),
        )
        .await;
        assert!(matches!(wrong_project, Err(StatusCode::NOT_FOUND)));
        ax_global_db::access::revoke_node(&db, "replica")
            .await
            .unwrap();
        let blocked = exchange(
            State(state),
            AxumPath("project".into()),
            headers,
            Json(incoming),
        )
        .await;
        assert!(matches!(
            blocked,
            Err(StatusCode::UNAUTHORIZED) | Err(StatusCode::FORBIDDEN)
        ));
        let created: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='ax_node_contributions')",
        )
        .fetch_one(ax.db_pool())
        .await
        .unwrap();
        assert!(!created);
    }
    #[tokio::test]
    async fn three_nodes_exchange_complete_graph_source_memory_logs_and_conversations_over_http() {
        use crate::server_config::ProjectRegistration;
        let primary_root = tempfile::tempdir().unwrap();
        let a_root = tempfile::tempdir().unwrap();
        let b_root = tempfile::tempdir().unwrap();
        async fn seed(root: &std::path::Path, label: &str) -> ax_core::Ax {
            let ax = ax_core::Ax::init(root).await.unwrap();
            let pool = ax.db_pool();
            sqlx::query("INSERT INTO nodes(id,kind,name,qualified_name,file_path,language,start_line,end_line,start_column,end_column,updated_at) VALUES ('same','function',?,?,'src/main.rs','rust',1,1,0,10,1)").bind(label).bind(label).execute(pool).await.unwrap();
            sqlx::query("INSERT INTO files VALUES ('src/main.rs','hash','rust',1,1,1,1,NULL)")
                .execute(pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO file_contents VALUES ('src/main.rs','hash',?,?,1)")
                .bind(format!("fn {label}() {{}}\n"))
                .bind(label.len() as i64 + 9)
                .execute(pool)
                .await
                .unwrap();
            sqlx::query("INSERT INTO edges(source,target,kind) VALUES ('same','same','calls')")
                .execute(pool)
                .await
                .unwrap();
            ax_memory::store::remember(
                pool,
                ax_memory::types::RememberInput {
                    title: label.into(),
                    body: format!("Memory from {label}"),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
            std::fs::write(root.join(".ax/mcp-verbose-2026-10-12.log"),format!("2026-10-12T00:00:00.000Z [ax-mcp] inbound tool=ax_node args={{\"node\":\"{label}\"}}\n")).unwrap();
            ax_usage::durable_project_apply(root,"chat",&serde_json::json!({"action":"append","kind":"user","body":format!("Conversation from {label}")})).await.unwrap();
            ax
        }
        let primary_ax = seed(primary_root.path(), "primary").await;
        let a_ax = seed(a_root.path(), "a").await;
        let b_ax = seed(b_root.path(), "b").await;
        let token = "x".repeat(64);
        std::env::set_var("AX_TEST_FULL_SYNC_CREDENTIAL", &token);
        let primary = ServerConfig {
            schema_version: 1,
            cluster_id: "team".into(),
            node_id: "primary".into(),
            role: NodeRole::Primary,
            primary_url: None,
            projects: vec![ProjectRegistration {
                id: "project".into(),
                path: primary_root.path().into(),
                token_env: "AX_TEST_FULL_SYNC_CREDENTIAL".into(),
            }],
        };
        let db = ax_global_db::open_and_init(&primary_root.path().join("access.db"))
            .await
            .unwrap();
        let state = Arc::new(StateData {
            config: primary.clone(),
            access_db: db.clone(),
            exports: Semaphore::new(4),
            control: None,
            shutdown: None,
        });
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let (shutdown, receiver) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            axum::serve(listener, public_routes().with_state(state))
                .with_graceful_shutdown(async move {
                    let _ = receiver.await;
                })
                .await
                .unwrap();
        });
        let mut a = primary.clone();
        a.role = NodeRole::Replica;
        a.node_id = "a_1".into();
        a.primary_url = Some(format!("http://{address}"));
        a.projects[0].path = a_root.path().into();
        let mut b = a.clone();
        b.node_id = "b".into();
        b.projects[0].path = b_root.path().into();
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap();
        let unauthorized = client
            .get(format!("http://{address}/v1/projects/project/snapshot"))
            .send()
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), reqwest::StatusCode::UNAUTHORIZED);
        let enroll = client
            .post(format!("http://{address}/v1/enrollment"))
            .bearer_auth(&token)
            .header("x-ax-node-id", "a_1")
            .send()
            .await
            .unwrap()
            .json::<serde_json::Value>()
            .await
            .unwrap();
        assert_eq!(enroll["protocol_version"], 3);
        assert_eq!(enroll["project_ids"], serde_json::json!(["project"]));
        async fn sync(client: &reqwest::Client, config: &ServerConfig) {
            let (snapshot, _) =
                crate::server_replica::fetch_snapshot(client, config, &config.projects[0])
                    .await
                    .unwrap();
            crate::server_data::apply(config, &snapshot).await.unwrap();
            crate::server_apply::apply(config, &snapshot).await.unwrap();
        }
        sync(&client, &a).await;
        sync(&client, &b).await;
        sync(&client, &a).await;
        for (root, ax) in [
            (primary_root.path(), &primary_ax),
            (a_root.path(), &a_ax),
            (b_root.path(), &b_ax),
        ] {
            let nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes")
                .fetch_one(ax.db_pool())
                .await
                .unwrap();
            assert_eq!(nodes, 3);
            let memories = ax_memory::store::list(ax.db_pool(), 100, 0)
                .await
                .unwrap()
                .0;
            assert!(memories.iter().any(|m| m.body == "Memory from primary"));
            assert!(memories.iter().any(|m| m.body == "Memory from a"));
            assert!(memories.iter().any(|m| m.body == "Memory from b"));
            let matches=ax_memory::store::recall(ax.db_pool(),"Memory from b",10).await.unwrap();
            assert!(matches.iter().any(|m|m.memory.body=="Memory from b"));
            let logs = ax_usage::read_merged_verbose_log(root);
            assert!(logs.contains("primary"));
            assert!(logs.contains("a"));
            assert!(logs.contains("b"));
            let counts = ax_db::replication::counts(ax.db_pool()).await.unwrap();
            assert_eq!(counts.len(), 2);
            assert!(counts.iter().all(|d| d.history_rows >= 3));
        }
        let source: String = sqlx::query_scalar(
            "SELECT content FROM file_contents WHERE path='ax-replica/a_1/src/main.rs'",
        )
        .fetch_one(b_ax.db_pool())
        .await
        .unwrap();
        assert_eq!(source, "fn a() {}\n");
        let native = crate::server_snapshot::export_native(&a, "project")
            .await
            .unwrap();
        let items = crate::server_data::manifests(&native).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].nodes, 1);
        // Both restarted application reads and local re-index cleanup retain foreign graph rows.
        let queries = ax_db::queries::QueryBuilder::new(a_ax.db_pool().clone());
        queries.clear_all().await.unwrap();
        let reopened = ax_core::Ax::open(a_root.path()).await.unwrap();
        let foreign: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id GLOB 'ax-replica/*'")
                .fetch_one(reopened.db_pool())
                .await
                .unwrap();
        assert_eq!(foreign, 2);
        let before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ax_node_contributions")
            .fetch_one(primary_ax.db_pool())
            .await
            .unwrap();
        let bad = client
            .post(format!("http://{address}/v1/projects/project/exchange"))
            .bearer_auth(&token)
            .header("x-ax-node-id", "b")
            .json(&native)
            .send()
            .await
            .unwrap();
        assert_eq!(bad.status(), reqwest::StatusCode::FORBIDDEN);
        let after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ax_node_contributions")
            .fetch_one(primary_ax.db_pool())
            .await
            .unwrap();
        assert_eq!(before, after);
        let _ = shutdown.send(());
        server.await.unwrap();
        std::env::remove_var("AX_TEST_FULL_SYNC_CREDENTIAL");
    }
}
