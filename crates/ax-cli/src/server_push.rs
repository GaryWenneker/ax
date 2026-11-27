//! Authenticated LAN UDP notification protocol for standalone Ax.
//!
//! Replica -> primary: signed REGISTER heartbeats (node, project and socket).
//! Primary -> replica: signed CHANGE hints containing only the new revision.
//! A hint causes an authenticated HTTPS pull. UDP never transports Ax data.
//! The primary listens on its HTTP bind address only. Every UDP message is
//! size-bounded and short-lived, and a nonce is accepted once. No automatic leader
//! election, no change to export/share settings and no unauthenticated peers.

use crate::{
    server_config::{NodeRole, ServerConfig},
    server_replica, server_snapshot,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
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
    blake3::keyed_hash(&key, data.as_bytes())
        .to_hex()
        .to_string()
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

fn decode(raw: &[u8], config: &ServerConfig, kind: &str, token: &str) -> Option<Datagram> {
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
    if message.signature.len() != 64 || !message.signature.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    if !bool::from(
        sign(&message, token)
            .as_bytes()
            .ct_eq(message.signature.as_bytes()),
    ) {
        return None;
    }
    match kind {
        "register" if message.revision.is_none() => Some(message),
        "changed"
            if message
                .revision
                .as_ref()
                .is_some_and(|v| v.len() == 64 && v.bytes().all(|b| b.is_ascii_hexdigit())) =>
        {
            Some(message)
        }
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

const MAX_REMEMBERED_NONCES: usize = 4096;

/// Nonces of accepted datagrams, kept until the message would fail the clock
/// check anyway. A captured datagram sent again is rejected.
#[derive(Default)]
struct ReplayGuard {
    seen: HashMap<String, u64>,
}

impl ReplayGuard {
    /// True the first time a datagram's nonce is seen while it is still fresh.
    /// Call only after the signature was verified.
    fn first_use(&mut self, message: &Datagram, now: u64) -> bool {
        self.seen.retain(|_, expires| *expires >= now);
        let key = format!(
            "{}|{}|{}",
            message.project_id, message.node_id, message.nonce
        );
        if self.seen.contains_key(&key) {
            return false;
        }
        if self.seen.len() >= MAX_REMEMBERED_NONCES {
            if let Some(oldest) = self
                .seen
                .iter()
                .min_by_key(|(_, expires)| **expires)
                .map(|(key, _)| key.clone())
            {
                self.seen.remove(&oldest);
            }
        }
        self.seen
            .insert(key, message.timestamp.saturating_add(CLOCK_SKEW_SECS));
        true
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.seen.len()
    }
}

#[derive(Clone)]
struct Registration {
    address: SocketAddr,
    last_seen: u64,
}

type Registry = Arc<Mutex<HashMap<(String, String), Registration>>>;

/// Size and modification time of every file a primary snapshot is built from:
/// the project database and its WAL, the MCP logs directly inside the
/// project's `.ax` folder, and the shared usage database with its WAL. Shared
/// memory files, locks and daemon state are left out: they change without a
/// data change.
type Fingerprint = Vec<(PathBuf, Option<(u64, SystemTime)>)>;

fn change_fingerprint(project_root: &Path, usage_db: &Path) -> Fingerprint {
    let stat = |path: &Path| {
        std::fs::symlink_metadata(path)
            .ok()
            .filter(|meta| meta.is_file())
            .map(|meta| (meta.len(), meta.modified().unwrap_or(UNIX_EPOCH)))
    };
    let mut files: Fingerprint = std::fs::read_dir(project_root.join(".ax"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name == "ax.db"
                        || name == "ax.db-wal"
                        || (name.starts_with("mcp-verbose") && name.ends_with(".log"))
                })
        })
        .filter_map(|path| stat(&path).map(|meta| (path, Some(meta))))
        .collect();
    files.sort();
    let mut wal = usage_db.as_os_str().to_owned();
    wal.push("-wal");
    for path in [usage_db.to_path_buf(), PathBuf::from(wal)] {
        let meta = stat(&path);
        files.push((path, meta));
    }
    files
}

/// Decides when the primary builds a snapshot to compare revisions. Building
/// one exports the native data and merges all retained history, so it runs
/// only for projects with a live replica registration and only after one of
/// the snapshot's source files changed.
#[derive(Default)]
struct ChangeDetector {
    seen: HashMap<String, (Fingerprint, String)>,
}

impl ChangeDetector {
    /// Returns the new revision when registered replicas should get a hint.
    async fn poll<F, Fut>(
        &mut self,
        project_id: &str,
        has_peers: bool,
        fingerprint: impl Fn() -> Fingerprint,
        export: F,
    ) -> Option<String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<String, String>>,
    {
        if !has_peers {
            return None;
        }
        if self
            .seen
            .get(project_id)
            .is_some_and(|(known, _)| *known == fingerprint())
        {
            return None;
        }
        let revision = match export().await {
            Ok(revision) => revision,
            Err(error) => {
                tracing::warn!(project = %project_id, "UDP change detection could not read snapshot: {}", error);
                return None;
            }
        };
        // Take the fingerprint after the export so the export's own writes do
        // not count as a change. A write that lands during the export is
        // picked up by the replicas' periodic sync.
        let previous = self
            .seen
            .insert(project_id.to_owned(), (fingerprint(), revision.clone()));
        match previous {
            Some((_, old)) if old != revision => Some(revision),
            _ => None,
        }
    }
}

/// The primary listens for registrations on the same address as its HTTP
/// listener, so a loopback-only node never opens UDP on other interfaces.
async fn bind_primary_socket(http_ip: IpAddr, udp_port: u16) -> Result<UdpSocket, String> {
    UdpSocket::bind((http_ip, udp_port))
        .await
        .map_err(|e| format!("cannot bind UDP push listener: {e}"))
}

/// Primary: register only authenticated replicas. Never trust a discovered
/// primary or a claimed IP in packet JSON: use the UDP socket source address.
pub async fn primary(config: ServerConfig, http_ip: IpAddr, udp_port: u16) -> Result<(), String> {
    if config.role != NodeRole::Primary {
        return Err("UDP primary registration requires a primary node".into());
    }
    let socket = Arc::new(bind_primary_socket(http_ip, udp_port).await?);
    let registry: Registry = Arc::new(Mutex::new(HashMap::new()));
    let access_db_path = ax_global_db::global_db_path().map_err(|e| e.to_string())?;
    let access_db = ax_global_db::open_and_init(&access_db_path)
        .await
        .map_err(|e| e.to_string())?;
    let receive_socket = socket.clone();
    let receive_registry = registry.clone();
    let receive_config = config.clone();
    let receive_access_db = access_db.clone();
    let _receiver = tokio::spawn(async move {
        let mut buf = [0u8; MAX_MESSAGE_BYTES + 1];
        let mut replays = ReplayGuard::default();
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
            let Some(message) = decode(&buf[..length], &receive_config, "register", &token) else {
                continue;
            };
            if !replays.first_use(&message, now_seconds()) {
                continue;
            }
            if let Err(error) = crate::server_http::ensure_trusted_replica(
                &receive_access_db,
                &message.node_id,
                Some(&sender.to_string()),
                &message.project_id,
            )
            .await
            {
                tracing::warn!(
                    node = %message.node_id,
                    project = %message.project_id,
                    "authenticated replica registration rejected: {}",
                    error
                );
                continue;
            }
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
    let mut detector = ChangeDetector::default();
    let usage_db = ax_usage::usage_db_path();
    loop {
        for project in &config.projects {
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
            let Some(revision) = detector
                .poll(
                    &project.id,
                    !peers.is_empty(),
                    || change_fingerprint(&project.path, &usage_db),
                    || async {
                        server_snapshot::export(&config, &project.id)
                            .await
                            .map(|snapshot| snapshot.revision)
                    },
                )
                .await
            else {
                continue;
            };
            let Some(token) = token_for(&config, &project.id) else {
                continue;
            };
            let message = packet("changed", &config, &project.id, Some(revision), &token);
            let Ok(payload) = serde_json::to_vec(&message) else {
                continue;
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
    auto_apply_memories: bool,
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
        return Err(
            "UDP notifications are LAN-only; use periodic HTTPS sync for remote primary".into(),
        );
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
    let mut replays = ReplayGuard::default();
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
        if !replays.first_use(&message, now_seconds()) {
            continue;
        }
        let Some(revision) = message.revision else {
            continue;
        };
        if previous_change.get(&message.project_id) == Some(&revision) {
            continue;
        }
        previous_change.insert(message.project_id, revision);
        if let Err(error) = server_replica::sync_with_apply(&config_path, auto_apply_memories).await
        {
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
        let message = packet(
            "register",
            &config,
            "work",
            None,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        let bytes = serde_json::to_vec(&message).unwrap();
        let primary = ServerConfig {
            node_id: "other".into(),
            role: NodeRole::Primary,
            ..config
        };
        assert!(decode(
            &bytes,
            &primary,
            "register",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .is_some());
        assert!(decode(
            &bytes,
            &primary,
            "register",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        )
        .is_none());
        assert!(decode(
            &bytes,
            &primary,
            "changed",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .is_none());
    }

    #[test]
    fn rejects_forged_or_cross_cluster_notifications() {
        let sender = config(NodeRole::Primary);
        let msg = packet(
            "changed",
            &sender,
            "project",
            Some("a".repeat(64)),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        );
        let mut receiver = config(NodeRole::Replica);
        receiver.node_id = "replica".into();
        assert!(decode(
            &serde_json::to_vec(&msg).unwrap(),
            &receiver,
            "changed",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .is_some());
        receiver.cluster_id = "another".into();
        assert!(decode(
            &serde_json::to_vec(&msg).unwrap(),
            &receiver,
            "changed",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        )
        .is_none());
    }

    #[tokio::test]
    async fn primary_listens_on_the_http_bind_address_only() {
        let socket = bind_primary_socket(IpAddr::from([127, 0, 0, 1]), 0)
            .await
            .unwrap();
        assert_eq!(
            socket.local_addr().unwrap().ip(),
            IpAddr::from([127, 0, 0, 1])
        );
    }

    #[test]
    fn a_replayed_datagram_is_rejected() {
        let sender = config(NodeRole::Replica);
        let message = packet("register", &sender, "work", None, &"a".repeat(32));
        let mut seen = ReplayGuard::default();
        let now = message.timestamp;
        assert!(seen.first_use(&message, now));
        assert!(!seen.first_use(&message, now + 1));
        let fresh = packet("register", &sender, "work", None, &"a".repeat(32));
        assert!(seen.first_use(&fresh, now + 1));
        // A nonce is forgotten only after the message could no longer pass the clock check.
        assert!(!seen.first_use(&message, now + CLOCK_SKEW_SECS));
    }

    #[test]
    fn the_replay_guard_stays_bounded() {
        let sender = config(NodeRole::Replica);
        let mut seen = ReplayGuard::default();
        let now = now_seconds();
        for _ in 0..MAX_REMEMBERED_NONCES + 10 {
            let message = packet("register", &sender, "work", None, &"a".repeat(32));
            assert!(seen.first_use(&message, now));
        }
        assert!(seen.len() <= MAX_REMEMBERED_NONCES);
    }

    #[tokio::test]
    async fn primary_builds_a_snapshot_only_for_live_replicas_after_a_source_change() {
        use std::{cell::Cell, rc::Rc};
        let mut detector = ChangeDetector::default();
        let fingerprint = Rc::new(Cell::new(1u64));
        let exports = Rc::new(Cell::new(0u32));
        let revision = Rc::new(Cell::new("a"));
        let fp = || {
            let fingerprint = fingerprint.clone();
            move || {
                vec![(
                    PathBuf::from("ax.db"),
                    Some((fingerprint.get(), UNIX_EPOCH)),
                )]
            }
        };
        let export = || {
            let exports = exports.clone();
            let revision = revision.clone();
            move || {
                exports.set(exports.get() + 1);
                let value = revision.get().to_owned();
                async move { Ok::<_, String>(value) }
            }
        };
        // No registered replica: nothing is exported.
        for _ in 0..3 {
            assert_eq!(detector.poll("p", false, fp(), export()).await, None);
        }
        assert_eq!(exports.get(), 0);
        // First look records the baseline without a hint.
        assert_eq!(detector.poll("p", true, fp(), export()).await, None);
        assert_eq!(exports.get(), 1);
        // Unchanged source files: no export, however often the loop runs.
        for _ in 0..5 {
            assert_eq!(detector.poll("p", true, fp(), export()).await, None);
        }
        assert_eq!(exports.get(), 1);
        // A changed file with a new revision sends one hint.
        fingerprint.set(2);
        revision.set("b");
        assert_eq!(
            detector.poll("p", true, fp(), export()).await,
            Some("b".into())
        );
        assert_eq!(exports.get(), 2);
        // A changed file with the same revision exports but sends nothing.
        fingerprint.set(3);
        assert_eq!(detector.poll("p", true, fp(), export()).await, None);
        assert_eq!(exports.get(), 3);
    }

    #[test]
    fn fingerprint_follows_project_files_and_the_usage_database() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ax/server")).unwrap();
        let usage = dir.path().join("usage.db");
        std::fs::write(dir.path().join(".ax/ax.db"), "one").unwrap();
        let first = change_fingerprint(dir.path(), &usage);
        assert_eq!(first, change_fingerprint(dir.path(), &usage));
        // Staging under subfolders, shared memory and daemon state are not source changes.
        std::fs::write(dir.path().join(".ax/server/staged.json"), "x").unwrap();
        std::fs::write(dir.path().join(".ax/ax.db-shm"), "x").unwrap();
        std::fs::write(dir.path().join(".ax/daemon.json"), "x").unwrap();
        assert_eq!(first, change_fingerprint(dir.path(), &usage));
        std::fs::write(dir.path().join(".ax/ax.db-wal"), "frame").unwrap();
        assert_ne!(first, change_fingerprint(dir.path(), &usage));
        std::fs::write(dir.path().join(".ax/mcp-verbose-2026-10-12.log"), "line").unwrap();
        let second = change_fingerprint(dir.path(), &usage);
        assert_ne!(first, second);
        std::fs::write(&usage, "rows").unwrap();
        assert_ne!(second, change_fingerprint(dir.path(), &usage));
    }

    #[test]
    fn refuses_wan_address_for_unauthenticated_lan_routing() {
        assert!(!local_lan("8.8.8.8".parse().unwrap()));
        assert!(local_lan("192.168.1.10".parse().unwrap()));
    }
}
