//! Opt-in LAN discovery. Advertisements are untrusted hints, never authority.
//! No project paths, content, tokens, sessions or credentials are broadcast.

use crate::server_config::{NodeRole, ServerConfig};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, net::SocketAddr, time::Duration};

const MAX_ANNOUNCEMENT_BYTES: usize = 512;
const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Announcement {
    protocol_version: u32,
    cluster_id: String,
    node_id: String,
    role: NodeRole,
    advertised_url: String,
}

fn announcement(config: &ServerConfig, url: &str) -> Result<Announcement, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.path() != "/"
    {
        return Err("advertised URL must be a credential-free HTTP(S) origin".into());
    }
    let message = Announcement {
        protocol_version: PROTOCOL_VERSION,
        cluster_id: config.cluster_id.clone(),
        node_id: config.node_id.clone(),
        role: config.role,
        advertised_url: url.into(),
    };
    let len = serde_json::to_vec(&message)
        .map_err(|e| e.to_string())?
        .len();
    if len > MAX_ANNOUNCEMENT_BYTES {
        return Err("discovery announcement exceeds size limit".into());
    }
    Ok(message)
}

/// Validate a browser-safe advertised origin before starting UDP broadcasts.
pub fn validate_advertise(config: &ServerConfig, url: &str) -> Result<(), String> {
    announcement(config, url).map(|_| ())
}

fn matching_candidate(raw: &[u8], config: &ServerConfig) -> Option<Announcement> {
    if raw.len() > MAX_ANNOUNCEMENT_BYTES {
        return None;
    }
    let item: Announcement = serde_json::from_slice(raw).ok()?;
    if item.protocol_version != PROTOCOL_VERSION
        || item.cluster_id != config.cluster_id
        || item.node_id == config.node_id
        || item.node_id.is_empty()
        || !item
            .node_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return None;
    }
    announcement(config, &item.advertised_url).ok()?;
    Some(item)
}

/// Listen only for the requested duration and print all distinct candidates.
/// No discovery response ever modifies the trusted primary_url in config.
pub async fn discover_candidates(
    config: &ServerConfig,
    port: u16,
    seconds: u64,
) -> Result<serde_json::Value, String> {
    if !(1..=60).contains(&seconds) {
        return Err("--seconds must be between 1 and 60".into());
    }
    let socket = tokio::net::UdpSocket::bind(("0.0.0.0", port))
        .await
        .map_err(|e| e.to_string())?;
    let until = tokio::time::Instant::now() + Duration::from_secs(seconds);
    let mut peers = BTreeMap::new();
    let mut buf = [0u8; MAX_ANNOUNCEMENT_BYTES + 1];
    loop {
        let now = tokio::time::Instant::now();
        if now >= until {
            break;
        }
        let remaining = until - now;
        let received = tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await;
        let Ok(Ok((len, source))) = received else {
            break;
        };
        if let Some(candidate) = matching_candidate(&buf[..len], config) {
            if peers.len() < 64 {
                peers.insert(
                    (candidate.node_id.clone(), source.ip()),
                    (
                        candidate.protocol_version,
                        candidate.cluster_id,
                        candidate.role,
                        candidate.advertised_url,
                    ),
                );
            }
        }
    }
    let peers = peers
        .into_iter()
        .map(|((node_id, sender_ip), (protocol, cluster, role, url))| {
            serde_json::json!({
                "protocol_version": protocol,
                "cluster_id": cluster,
                "node_id": node_id,
                "role": role,
                "sender_ip": sender_ip.to_string(),
                "advertised_url": url,
                "trusted": false,
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::json!({"candidates": peers}))
}

/// Periodically publish untrusted node hints on the configured LAN port.
/// Broadcasting is opt-in; static primary URLs are still required for sync.
pub async fn announce(config: ServerConfig, url: String, port: u16) -> Result<(), String> {
    let message = announcement(&config, &url)?;
    let encoded = serde_json::to_vec(&message).map_err(|e| e.to_string())?;
    let socket = tokio::net::UdpSocket::bind(("0.0.0.0", 0))
        .await
        .map_err(|e| e.to_string())?;
    socket.set_broadcast(true).map_err(|e| e.to_string())?;
    let target = SocketAddr::from(([255, 255, 255, 255], port));
    loop {
        if let Err(error) = socket.send_to(&encoded, target).await {
            tracing::warn!("LAN discovery broadcast failed: {}", error);
        }
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filters_by_cluster_and_never_trusts_announcements() {
        let config = ServerConfig {
            schema_version: 1,
            cluster_id: "cluster".into(),
            node_id: "self".into(),
            role: NodeRole::Replica,
            primary_url: Some("https://main.example".into()),
            projects: vec![],
        };
        let mut peer = announcement(&config, "https://replica.example").unwrap();
        peer.node_id = "peer".into();
        let encoded = serde_json::to_vec(&peer).unwrap();
        assert!(matching_candidate(&encoded, &config).is_some());
        peer.cluster_id = "other".into();
        assert!(matching_candidate(&serde_json::to_vec(&peer).unwrap(), &config).is_none());
    }

    #[test]
    fn does_not_broadcast_credentials_or_paths() {
        let config = ServerConfig {
            schema_version: 1,
            cluster_id: "cluster".into(),
            node_id: "self".into(),
            role: NodeRole::Primary,
            primary_url: None,
            projects: vec![],
        };
        assert!(announcement(&config, "https://user:secret@example.com").is_err());
        assert!(announcement(&config, "https://example.com/path").is_err());
    }
}

/// CLI presentation of the same bounded discovery used by Settings.
pub async fn discover(config: &ServerConfig, port: u16, seconds: u64) -> Result<(), String> {
    let result = discover_candidates(config, port, seconds).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
