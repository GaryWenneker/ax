//! Authenticated LAN UDP notification protocol for standalone Ax.
//!
//! Replica -> primary: signed REGISTER heartbeats (node, project and socket).
//! Primary -> replica: signed CHANGE hints containing only the new revision.
//! A hint causes an authenticated HTTPS pull. UDP never transports Ax data.
//! All UDP messages are size-bounded and short-lived. No automatic leader
//! election, no change to export/share settings and no unauthenticated peers.

use crate::{
    server_config::{NodeRole, ServerConfig},
    server_replica,
    server_snapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use subtle::ConstantTimeEq;
use tokio::{net::UdpSocket, sync::Mutex};

const MAX_MESSAGE_BYTES: usize = 700;
const REGISTRATION_LIFETIME_SECS: u64 = 45;
const CLOCK_SKEW_SECS: u64 = 90;
const HEARTBEAT_SECS: u64 = 10;
const CHANGE_CHECK_SECS: u64 = 3;
const MAX_REGISTERED_NODES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Datagram {
    version: u32,
    kind: String,
    cluster_id: String,
    node_id: String,
    project_id: String,
    timestamp: u64,
    nonce: String,
    revision: Option<String>,
    signature: String,
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sign(message: &Datagram, token: &str) -> String {
    let data = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        message.version,
        message.kind,
        message.cluster_id,
        message.node_id,
        message.project_id,
        message.timestamp,
        message.nonce,
        message.revision.as_deref().unwrap_or(""),
    );
    let key = blake3::derive_key("ax-standalone-udp-notification-v1", token.as_bytes());
    blake3::keyed_hash(&key, data.as_bytes()).to_hex().to_string()
}

fn packet(
    kind: &str,
    config: &ServerConfig,
    project_id: &str,
    revision: Option<String>,
    token: &str,
) -> Datagram {
    let mut message = Datagram {
        version: 1,
        kind: kind.to_owned(),
        cluster_id: config.cluster_id.clone(),
        node_id: config.node_id.clone(),
        project_id: project_id.to_owned(),
        timestamp: now_seconds(),
        nonce: uuid::Uuid::new_v4().simple().to_string(),
        revision,
        signature: String::new(),
    };
    message.signature = sign(&message, token);
    message
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn decode(
    raw: &[u8],
    config: &ServerConfig,
    kind: &str,
    token: &str,
) -> Option<Datagram> {
    if raw.len() > MAX_MESSAGE_BYTES {
        return None;
    }
    let message: Datagram = serde_json::from_slice(raw).ok()?;
    if message.version != 1
        || message.kind != kind
        || message.cluster_id != config.cluster_id
        || message.node_id == config.node_id
        || !valid_id(&message.node_id)
        || !valid_id(&message.project_id)
        || message.nonce.len() != 32
        || !message.nonce.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    if message.timestamp.abs_diff(now_seconds()) > CLOCK_SKEW_SECS {
        return None;
    }
    if message.signature.len() != 64
        || !message.signature.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    if !bool::from(sign(&message, token).as_bytes().ct_eq(message.signature.as_bytes())) {
        return None;
    }
    match kind {
        "register" if message.revision.is_none() => Some(message),
        "changed" if message.revision.as_ref().is_some_and(|v| {
            v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())
        }) => Some(message),
        _ => None,
    }
}

fn token_for(config: &ServerConfig, project_id: &str) -> Option<String> {
    let project = config.projects.iter().find(|p| p.id == project_id)?;
    let token = std::env::var(&project.token_env).ok()?;
    if token.len() < 32
        || token.len() > 4096
        || !token.is_ascii()
        || token.bytes().any(|b| b.is_ascii_whitespace())
    {
        return None;
    }
    Some(token)
}

fn local_lan(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local(),
    }
}

#[derive(Clone)]
struct Registration {
    address: SocketAddr,
    last_seen: u64,
}

type Registry = Arc<Mutex<HashMap<(String, String), Registration>>>;

/// Primary: register only authenticated replicas. Never trust a discovered
/// primary or a claimed IP in packet JSON: use the UDP socket source address.
pub async fn primary(config: ServerConfig, udp_port: u16) -> Result<(), String> {
    if config.role != NodeRole::Primary {
        return Err("UDP primary registration requires a primary node".into());
    }
    let socket = Arc::new(
        UdpSocket::bind(("0.0.0.0", udp_port))
            .await
            .map_err(|e| format!("cannot bind UDP push listener: {e}"))?,
    );
    let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
    let receive_socket = socket.clone();
    let receive_registry = registry.clone();
    let receive_config = config.clone();
    let _receiver = tokio::spawn(async move {
        let mut buf = [0u8; MAX_MESSAGE_BYTES + 1];
        loop {
            let Ok((length, sender)) = receive_socket.recv_from(&mut buf).await else {
                continue;
            };
            if !local_lan(sender.ip()) || length > MAX_MESSAGE_BYTES {
                continue;
            }
            let Ok(untrusted) = serde_json::from_slice::<Datagram>(&buf[..length]) else {
                continue;
            };
            let Some(token) = token_for(&receive_config, &untrusted.project_id) else {
                continue;
            };
            let Some(message) = decode(&buf[..length], &receive_config, "register", &token)
            else {
                continue;
            };
            let mut peers = receive_registry.lock().await;
            peers.retain(|_, peer| {
                now_seconds().saturating_sub(peer.last_seen) <= REGISTRATION_LIFETIME_SECS
            });
            let key = (message.project_id, message.node_id);
            if peers.len() < MAX_REGISTERED_NODES || peers.contains_key(&key) {
                peers.insert(
                    key,
                    Registration {
                        address: sender,
                        last_seen: now_seconds(),
                    },
                );
            }
        }
    });
    let mut last_revisions: HashMap<String, String> = HashMap::new();
    loop {
        for project in &config.projects {
            let snapshot = match server_snapshot::export(&config, &project.id).await {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    tracing::warn!(
                        project = %project.id,
                        "UDP change detection could not read snapshot: {}",
                        error
                    );
                    continue;
                }
            };
            let previous = last_revisions.insert(project.id.clone(), snapshot.revision.clone());
            if previous.as_deref() == Some(snapshot.revision.as_str()) || previous.is_none() {
                continue;
            }
            let Some(token) = token_for(&config, &project.id) else {
                continue;
            };
            let message = packet(
                "changed",
                &config,
                &project.id,
                Some(snapshot.revision),
                &token,
            );
            let Ok(payload) = serde_json::to_vec(&message) else {
                continue;
            };
            let peers: Vec<SocketAddr> = {
                let mut registry = registry.lock().await;
                registry.retain(|_, peer| {
                    now_seconds().saturating_sub(peer.last_seen) <= REGISTRATION_LIFETIME_SECS
                });
                registry
                    .iter()
                    .filter(|((id, _), _)| id == &project.id)
                    .map(|(_, registration)| registration.address)
                    .collect()
            };
            for peer in peers {
                if let Err(error) = socket.send_to(&payload, peer).await {
                    tracing::warn!("UDP change notification send failed: {}", error);
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(CHANGE_CHECK_SECS)).await;
    }
}

/// Replica: send signed UDP registrations from its receive socket and pull
/// through the configured, authenticated primary HTTPS endpoint on change.
/// Periodic HTTPS sync in server_http remains the fallback for lost packets.
pub async fn replica(
    config: ServerConfig,
    config_path: PathBuf,
    udp_port: u16,
) -> Result<(), String> {
    if config.role != NodeRole::Replica {
        return Err("UDP replica notification requires a replica node".into());
    }
    let base = config
        .primary_url
        .as_deref()
        .ok_or("replica requires an explicitly configured primary")?;
    let url = reqwest::Url::parse(base).map_err(|e| e.to_string())?;
    let host = url.host_str().ok_or("primary origin has no hostname")?;
    let primary_addresses: Vec<SocketAddr> = tokio::net::lookup_host((host, udp_port))
        .await
        .map_err(|e| format!("cannot resolve primary for UDP registration: {e}"))?
        .filter(|address| local_lan(address.ip()))
        .collect();
    if primary_addresses.is_empty() {
        return Err("UDP notifications are LAN-only; use periodic HTTPS sync for remote primary".into());
    }
    let socket = Arc::new(
        UdpSocket::bind(("0.0.0.0", 0))
            .await
            .map_err(|e| format!("cannot bind UDP notification socket: {e}"))?,
    );
    let heartbeat_socket = socket.clone();
    let heartbeat_config = config.clone();
    let _registration_task = tokio::spawn(async move {
        loop {
            for project in &heartbeat_config.projects {
                let Some(token) = token_for(&heartbeat_config, &project.id) else {
                    continue;
                };
                let message = packet("register", &heartbeat_config, &project.id, None, &token);
                if let Ok(payload) = serde_json::to_vec(&message) {
                    for address in &primary_addresses {
                        let _ = heartbeat_socket.send_to(&payload, address).await;
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(HEARTBEAT_SECS)).await;
        }
    });
    let mut previous_change: HashMap<String, String> = HashMap::new();
    let mut buf = [0u8; MAX_MESSAGE_BYTES + 1];
    loop {
        let (length, source) = socket
            .recv_from(&mut buf)
            .await
            .map_err(|e| format!("UDP notification receive failed: {e}"))?;
        if length > MAX_MESSAGE_BYTES || !local_lan(source.ip()) {
            continue;
        }
        let Ok(untrusted) = serde_json::from_slice::<Datagram>(&buf[..length]) else {
            continue;
        };
        let Some(token) = token_for(&config, &untrusted.project_id) else {
            continue;
        };
        let Some(message) = decode(&buf[..length], &config, "changed", &token) else {
            continue;
        };
        let Some(revision) = message.revision else {
            continue;
        };
        if previous_change.get(&message.project_id) == Some(&revision) {
            continue;
        }
        previous_change.insert(message.project_id, revision);
        if let Err(error) = server_replica::sync(&config_path).await {
            tracing::warn!("UDP-triggered replica sync failed: {}", error);
            // Periodic polling will retry; an identical later notification
            // should also be able to retry, so forget the failed revision.
            previous_change.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(role: NodeRole) -> ServerConfig {
        ServerConfig {
            schema_version: 1,
            cluster_id: "test-cluster".into(),
            node_id: "primary".into(),
            role,
            primary_url: None,
            projects: vec![],
        }
    }

    #[test]
    fn registration_is_authenticated_per_project() {
        let config = config(NodeRole::Replica);
        let message = packet("register", &config, "work", None, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let bytes = serde_json::to_vec(&message).unwrap();
        let primary = ServerConfig {
            node_id: "other".into(),
            role: NodeRole::Primary,
            ..config
        };
        assert!(decode(&bytes, &primary, "register", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_some());
        assert!(decode(&bytes, &primary, "register", "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").is_none());
        assert!(decode(&bytes, &primary, "changed", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none());
    }

    #[test]
    fn rejects_forged_or_cross_cluster_notifications() {
        let sender = config(NodeRole::Primary);
        let msg = packet("changed", &sender, "project", Some("a".repeat(64)), "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let mut receiver = config(NodeRole::Replica);
        receiver.node_id = "replica".into();
        assert!(decode(&serde_json::to_vec(&msg).unwrap(), &receiver, "changed", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_some());
        receiver.cluster_id = "another".into();
        assert!(decode(&serde_json::to_vec(&msg).unwrap(), &receiver, "changed", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").is_none());
    }

    #[test]
    fn refuses_wan_address_for_unauthenticated_lan_routing() {
        assert!(!local_lan("8.8.8.8".parse().unwrap()));
        assert!(local_lan("192.168.1.10".parse().unwrap()));
    }
}
