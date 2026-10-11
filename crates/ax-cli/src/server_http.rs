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
    Ok(principal
        .as_ref()
        .map(|p| p.node_id.clone())
        .or_else(|| node_id.map(str::to_owned))
        .ok_or(StatusCode::UNAUTHORIZED)?)
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
    sqlx::query("UPDATE replication_nodes SET last_seen_at = CURRENT_TIMESTAMP, last_sync_at = CURRENT_TIMESTAMP WHERE id = ? AND trust_state = 'trusted'")
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
    server_snapshot::validate(&incoming, &state.config.cluster_id, &id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    let _permit = state
        .exports
        .acquire()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let snapshot = crate::server_exchange::publish(&state.config, &id, Some(&incoming))
        .await
        .map_err(|error| {
            tracing::warn!(project = %id, "node contribution was not accepted: {}", error);
            StatusCode::CONFLICT
        })?;
    // Received memory is available under separate IDs; native Primary data is
    // untouched. Policy enters pending review, never automatic activation.
    let report = crate::server_apply::apply(&state.config, &snapshot).await.map_err(|error| {
        tracing::warn!(project = %id, "cluster projection failed; durable contribution retained: {}", error); StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(crate::server_exchange::ExchangeReply {
        snapshot,
        contributed_revision: incoming.revision,
        memory_conflicts: report.conflicts,
    }))
}

fn private_bind(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local() || ip.is_unspecified(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local() || ip.is_unspecified(),
    }
}

pub struct RunOptions {
    pub bind: SocketAddr,
    pub allow_private_http: bool,
    pub discovery_port: Option<u16>,
    pub advertise: Option<String>,
    pub sync_interval_seconds: Option<u64>,
    pub push_port: u16,
    pub auto_apply_memories: bool,
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
    } = options;
    let config = server_cli::load(path)?;
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
    let state = Arc::new(StateData {
        config,
        access_db,
        exports: Semaphore::new(4),
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/projects/{id}/snapshot", get(snapshot))
        .route("/v1/projects/{id}/exchange", post(exchange))
        .layer(DefaultBodyLimit::max(
            server_snapshot::MAX_SNAPSHOT_BYTES + 1024 * 1024,
        ))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "Ax standalone server listening on {}",
        listener.local_addr().map_err(|e| e.to_string())?
    );
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|e| e.to_string());
    if let Some(task) = announcement {
        task.abort();
    }
    if let Some(task) = periodic_sync {
        task.abort();
    }
    if let Some(task) = push_task {
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
}
