//! Minimal authenticated HTTP transport for standalone Ax snapshots.
//!
//! The listener defaults to loopback. TLS termination for hosted deployments
//! belongs to a reverse proxy; bind a private interface only by explicit opt-in.

use crate::{
    server_cli,
    server_config::{NodeRole, ServerConfig},
    server_discovery,
    server_push,
    server_snapshot,
};
use axum::{
    extract::{Path as AxumPath, State},
    http::{header, HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use std::{collections::HashMap, net::{IpAddr, SocketAddr}, path::Path, sync::Arc};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;

struct StateData {
    config: ServerConfig,
    tokens: HashMap<String, String>,
    exports: Semaphore,
}

fn load_tokens(config: &ServerConfig) -> Result<HashMap<String, String>, String> {
    config.projects.iter().map(|project| {
        let token = std::env::var(&project.token_env)
            .map_err(|_| format!("missing secret in environment variable {}", project.token_env))?;
        if token.len() < 32 || !token.is_ascii() || token.bytes().any(|b| b.is_ascii_whitespace()) {
            return Err(format!(
                "{} must contain a random ASCII token of at least 32 non-whitespace characters",
                project.token_env
            ));
        }
        Ok((project.id.clone(), token))
    }).collect()
}

fn authorized(headers: &HeaderMap, secret: &str) -> bool {
    let Some(value) = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let Some(candidate) = value.strip_prefix("Bearer ") else { return false };
    if candidate.len() != secret.len() { return false; }
    bool::from(candidate.as_bytes().ct_eq(secret.as_bytes()))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok"}))
}

async fn snapshot(
    State(state): State<Arc<StateData>>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> Result<Json<server_snapshot::Snapshot>, StatusCode> {
    if state.config.role != NodeRole::Primary {
        return Err(StatusCode::NOT_FOUND);
    }
    let Some(token) = state.tokens.get(&id) else {
        return Err(StatusCode::NOT_FOUND);
    };
    if !authorized(&headers, token) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let _permit = state.exports.acquire().await.map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let result = server_snapshot::export(&state.config, &id).await
        .map_err(|error| {
            tracing::warn!(project = %id, "snapshot export failed: {}", error);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    Ok(Json(result))
}

fn private_bind(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_link_local() || ip.is_unspecified(),
        IpAddr::V6(ip) => ip.is_unique_local() || ip.is_unicast_link_local() || ip.is_unspecified(),
    }
}

/// Start the listener and serve until the process receives shutdown.
pub async fn run(
    path: &Path,
    bind: SocketAddr,
    allow_private_http: bool,
    discovery_port: Option<u16>,
    advertise: Option<String>,
    sync_interval_seconds: Option<u64>,
    push_port: u16,
    auto_apply_memories: bool,
) -> Result<(), String> {
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
    let tokens = load_tokens(&config)?;
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
                if let Err(error) = crate::server_replica::sync_with_apply(&config_path, auto_apply_memories).await {
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
                NodeRole::Replica => server_push::replica(node, config_path, push_port, auto_apply_memories).await,
            };
            if let Err(error) = result {
                tracing::warn!("UDP push unavailable; HTTPS polling remains active: {}", error);
            }
        }))
    };
    let state = Arc::new(StateData {
        config,
        tokens,
        exports: Semaphore::new(4),
    });
    let app = Router::new()
        .route("/health", get(health))
        .route("/v1/projects/{id}/snapshot", get(snapshot))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind(bind).await.map_err(|e| e.to_string())?;
    println!("Ax standalone server listening on {}", listener.local_addr().map_err(|e| e.to_string())?);
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
    fn bearer_auth_requires_exact_match() {
        let mut headers = HeaderMap::new();
        headers.insert(header::AUTHORIZATION, "Bearer aaaaa".parse().unwrap());
        assert!(authorized(&headers, "aaaaa"));
        assert!(!authorized(&headers, "aaaab"));
        assert!(!authorized(&headers, "aaaa"));
        headers.insert(header::AUTHORIZATION, "Basic aaaaa".parse().unwrap());
        assert!(!authorized(&headers, "aaaaa"));
    }

    #[test]
    fn public_ips_are_not_private_bind_targets() {
        assert!(!private_bind("8.8.8.8".parse().unwrap()));
        assert!(private_bind("192.168.1.5".parse().unwrap()));
        assert!(private_bind("fd00::1".parse().unwrap()));
    }
}
