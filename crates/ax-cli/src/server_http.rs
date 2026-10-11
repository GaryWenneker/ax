//! Minimal authenticated HTTP transport for standalone Ax snapshots.
//!
//! The listener defaults to loopback. TLS termination for hosted deployments
//! belongs to a reverse proxy; bind a private interface only by explicit opt-in.

use crate::{
    server_cli,
    server_config::{NodeRole, ServerConfig},
    server_discovery,
    server_snapshot,
};
use axum::{
    extract::{Path, State},
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
    Path(id): Path<String>,
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
) -> Result<(), String> {
    let config = server_cli::load(path)?;
    if discovery_port.is_some() != advertise.is_some() {
        return Err("LAN discovery requires both --discovery-port and --advertise".into());
    }
    if let Some(url) = advertise.as_deref() {
        server_discovery::validate_advertise(&config, url)?;
    }
    if !bind.ip().is_loopback() {
        if !allow_private_http || !private_bind(bind.ip()) {
            return Err("non-loopback bind requires --allow-private-http on a private network; terminate TLS at the reverse proxy".into());
        }
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
