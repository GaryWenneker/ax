//! Podman/Docker runtime discovery and SonarQube container lifecycle.

use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

/// SonarQube Community Build (monthly CalVer). Replaces legacy `sonarqube:lts-community` (9.9 LTS).
const SONAR_IMAGE: &str = "sonarqube:community";
const POSTGRES_IMAGE: &str = "postgres:16-alpine";
const SONAR_NETWORK: &str = "ax-sonar-net";
const POSTGRES_USER: &str = "sonar";
const POSTGRES_PASSWORD: &str = "sonar";
const POSTGRES_DB: &str = "sonar";
const PG_VOLUME: &str = "ax-sonarqube-pg";
const SONAR_DATA_VOLUME: &str = "ax-sonarqube-data";
const SONAR_EXTENSIONS_VOLUME: &str = "ax-sonarqube-extensions";
const SONAR_LOGS_VOLUME: &str = "ax-sonarqube-logs";
const SONAR_CONTAINER_MEMORY: &str = "4g";
const SONAR_SHM_SIZE: &str = "256m";
const SONAR_CE_JAVAOPTS: &str = "-Xmx2g -Xms512m -XX:+UseG1GC";
const SONAR_WEB_JAVAOPTS: &str = "-Xmx512m -Xms256m";

const SONAR_UPGRADE_HINT: &str = "Running an older SonarQube image (e.g. 9.9 LTS). \
To install Community Build fresh: stop the stack, remove containers, then Install again. \
For in-place migration see SonarSource: 9.9 → 24.12 → current.";
const SONAR_POSTGRES_UPGRADE_HINT: &str = "Upgrading from embedded H2 to PostgreSQL. \
The old SonarQube container is recreated; project data is not migrated automatically — \
run Setup project & token in Command Center afterward.";
pub const SONAR_SCANNER_IMAGE: &str = "sonarsource/sonar-scanner-cli";

/// Collects install/start log lines; optionally streams each line over SSE.
#[derive(Clone)]
pub struct InstallLog {
    lines: Arc<Mutex<Vec<String>>>,
    tx: Option<UnboundedSender<String>>,
}

impl InstallLog {
    pub fn new() -> Self {
        Self {
            lines: Arc::new(Mutex::new(Vec::new())),
            tx: None,
        }
    }

    pub fn with_stream(tx: UnboundedSender<String>) -> Self {
        Self {
            lines: Arc::new(Mutex::new(Vec::new())),
            tx: Some(tx),
        }
    }

    pub fn push(&self, line: impl Into<String>) {
        let line = line.into();
        if let Some(tx) = &self.tx {
            let payload = serde_json::json!({ "type": "log", "line": line }).to_string();
            let _ = tx.send(payload);
        }
        if let Ok(mut lines) = self.lines.lock() {
            lines.push(line);
        }
    }

    pub fn lines(&self) -> Vec<String> {
        self.lines.lock().map(|l| l.clone()).unwrap_or_default()
    }
}

impl Default for InstallLog {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerRuntime {
    Podman,
    Docker,
}

impl ContainerRuntime {
    pub fn cli(self) -> &'static str {
        match self {
            Self::Podman => "podman",
            Self::Docker => "docker",
        }
    }

    pub fn from_pref(preferred: Option<&str>) -> Option<Self> {
        match preferred.map(str::to_lowercase).as_deref() {
            Some("podman") => Some(Self::Podman),
            Some("docker") => Some(Self::Docker),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeInfo {
    pub runtime: ContainerRuntime,
    pub version: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContainerInfo {
    pub name: String,
    pub runtime: ContainerRuntime,
    pub status: String,
    pub running: bool,
    pub ports: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SonarDiscovery {
    pub runtimes: Vec<RuntimeInfo>,
    pub preferred: Option<ContainerRuntime>,
    pub container: Option<ContainerInfo>,
    /// PostgreSQL sidecar when the stack uses an external database (not embedded H2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<ContainerInfo>,
    pub reachable: bool,
    pub host: String,
    /// `true` when SonarQube runs with the embedded H2 database (discouraged).
    #[serde(default)]
    pub embedded_database: bool,
}

fn runtime_version(runtime: ContainerRuntime) -> Option<String> {
    let out = Command::new(runtime.cli())
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Some(text.lines().next().unwrap_or("unknown").trim().to_string())
}

pub fn discover_runtimes() -> Vec<RuntimeInfo> {
    let mut out = Vec::new();
    for rt in [ContainerRuntime::Podman, ContainerRuntime::Docker] {
        if let Some(version) = runtime_version(rt) {
            out.push(RuntimeInfo {
                runtime: rt,
                version,
                available: true,
            });
        }
    }
    out
}

pub fn resolve_runtime(preferred: Option<&str>) -> Result<ContainerRuntime, String> {
    if let Some(rt) = ContainerRuntime::from_pref(preferred) {
        if runtime_version(rt).is_some() {
            return Ok(rt);
        }
        return Err(format!("{} is not installed or not on PATH", rt.cli()));
    }
    if runtime_version(ContainerRuntime::Podman).is_some() {
        return Ok(ContainerRuntime::Podman);
    }
    if runtime_version(ContainerRuntime::Docker).is_some() {
        return Ok(ContainerRuntime::Docker);
    }
    Err("Neither podman nor docker found on PATH".into())
}

pub fn find_container(runtime: ContainerRuntime, name: &str) -> Option<ContainerInfo> {
    let out = Command::new(runtime.cli())
        .args([
            "ps",
            "-a",
            "--filter",
            &format!("name=^{name}$"),
            "--format",
            "{{.Names}}\t{{.Status}}\t{{.Ports}}",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    let parts: Vec<&str> = line.split('\t').collect();
    let cname = parts.first()?.trim();
    if cname.is_empty() {
        return None;
    }
    let status = parts.get(1).unwrap_or(&"").trim().to_string();
    let ports = parts.get(2).unwrap_or(&"").trim().to_string();
    let running = status.to_ascii_lowercase().starts_with("up");
    Some(ContainerInfo {
        name: cname.to_string(),
        runtime,
        status,
        running,
        ports,
    })
}

pub fn discover_sonar(host: &str, container_name: &str, preferred: Option<&str>) -> SonarDiscovery {
    let runtimes = discover_runtimes();
    let preferred_rt = resolve_runtime(preferred).ok();
    let container = preferred_rt
        .and_then(|rt| find_container(rt, container_name))
        .or_else(|| {
            for rt in [ContainerRuntime::Podman, ContainerRuntime::Docker] {
                if let Some(c) = find_container(rt, container_name) {
                    return Some(c);
                }
            }
            None
        });
    let database = container.as_ref().and_then(|c| {
        find_container(c.runtime, &db_container_name(container_name))
    });
    let embedded_database = container
        .as_ref()
        .is_some_and(|c| !container_uses_postgres(c.runtime, container_name));
    let reachable = sonar_reachable_sync(host);
    let mut effective_host = host.to_string();
    if !reachable {
        if let Some(fallback) = sonar_localhost_fallback(host, container.as_ref()) {
            effective_host = fallback;
        }
    }
    let reachable = if reachable {
        true
    } else {
        sonar_reachable_sync(&effective_host)
    };
    SonarDiscovery {
        runtimes,
        preferred: preferred_rt,
        container,
        database,
        reachable,
        host: effective_host,
        embedded_database,
    }
}

/// When the configured host (e.g. `http://sonarqube.vsc:9000`) is offline but the container
/// is running with a local port bind, fall back to localhost.
fn sonar_localhost_fallback(host: &str, container: Option<&ContainerInfo>) -> Option<String> {
    let container = container?;
    if !container.running {
        return None;
    }
    let port = sonar_host_port(host);
    for candidate in [
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
    ] {
        let base = host.trim_end_matches('/');
        if candidate == base {
            continue;
        }
        if sonar_reachable_sync(&candidate) {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod discover_fallback_tests {
    use super::*;

    #[test]
    fn localhost_fallback_skips_same_host() {
        assert!(sonar_localhost_fallback("http://localhost:9000", None).is_none());
    }
}

/// Sidecar PostgreSQL container name for a SonarQube stack.
pub fn db_container_name(sonar_name: &str) -> String {
    format!("{sonar_name}-db")
}

/// Host port from a SonarQube URL such as `http://localhost:9000` (defaults to 9000).
pub fn sonar_host_port(host: &str) -> u16 {
    let trimmed = host.trim_end_matches('/');
    let after_scheme = trimmed.split("//").nth(1).unwrap_or(trimmed);
    after_scheme
        .split(':')
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(9000)
}

/// Install or start the PostgreSQL-backed SonarQube stack when the API is offline.
pub async fn ensure_sonar_stack_online(
    config: &crate::sonar::SonarConfig,
    log: &InstallLog,
) -> Result<SonarDiscovery, String> {
    let host = config.host.trim_end_matches('/');
    let name = config
        .podman_container
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("sonarqube");
    let pref = if config.container_runtime == "auto" {
        None
    } else {
        Some(config.container_runtime.as_str())
    };
    if sonar_reachable_sync(host) {
        return Ok(discover_sonar(host, name, pref));
    }
    log.push(format!(
        "SonarQube is offline — provisioning PostgreSQL stack ({POSTGRES_IMAGE} + {SONAR_IMAGE})."
    ));
    ensure_sonar_live_with_log(host, name, pref, sonar_host_port(host), log).await
}

fn sonar_blocking_client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_millis(800))
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    })
}

fn sonar_reachable_sync(host: &str) -> bool {
    sonar_ping_fast(host)
}

/// Fast HTTP ping to SonarQube `/api/system/status` (shared client, ~800ms timeout).
pub fn sonar_ping_fast(host: &str) -> bool {
    let url = format!("{}/api/system/status", host.trim_end_matches('/'));
    sonar_blocking_client()
        .get(&url)
        .send()
        .ok()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Localhost variants for the same port — used when the configured hostname does not resolve locally.
pub fn sonar_localhost_candidates(host: &str) -> Vec<String> {
    let port = sonar_host_port(host);
    let base = host.trim_end_matches('/');
    ["http://127.0.0.1", "http://localhost"]
        .into_iter()
        .map(|scheme| format!("{scheme}:{port}"))
        .filter(|c| c != base)
        .collect()
}

pub async fn wait_for_sonar(host: &str, timeout_secs: u64) -> Result<(), String> {
    let url = format!("{}/api/system/status", host.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        if tokio::time::Instant::now() > deadline {
            return Err(format!("SonarQube did not become ready at {host} within {timeout_secs}s"));
        }
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

const START_CONTAINER_ATTEMPTS: u32 = 3;
const START_CONTAINER_RETRY_SECS: u64 = 2;

pub fn start_container(runtime: ContainerRuntime, name: &str) -> Result<(), String> {
    let mut last_err = String::new();
    for attempt in 1..=START_CONTAINER_ATTEMPTS {
        info!(
            "Starting container {name} via {} (attempt {attempt}/{START_CONTAINER_ATTEMPTS})",
            runtime.cli()
        );
        match start_container_once(runtime, name) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e;
                if attempt < START_CONTAINER_ATTEMPTS {
                    std::thread::sleep(Duration::from_secs(START_CONTAINER_RETRY_SECS));
                }
            }
        }
    }
    Err(format!(
        "{last_err} (after {START_CONTAINER_ATTEMPTS} start attempts)"
    ))
}

fn start_container_once(runtime: ContainerRuntime, name: &str) -> Result<(), String> {
    let status = Command::new(runtime.cli())
        .args(["start", name])
        .status()
        .map_err(|e| format!("{} start failed: {e}", runtime.cli()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} start {name} exited with {status}", runtime.cli()))
    }
}

pub fn stop_container(runtime: ContainerRuntime, name: &str) -> Result<(), String> {
    info!("Stopping container {name} via {}", runtime.cli());
    let status = Command::new(runtime.cli())
        .args(["stop", name])
        .status()
        .map_err(|e| format!("{} stop failed: {e}", runtime.cli()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} stop {name} exited with {status}", runtime.cli()))
    }
}

pub fn find_container_any(name: &str) -> Option<ContainerInfo> {
    for rt in [ContainerRuntime::Podman, ContainerRuntime::Docker] {
        if let Some(c) = find_container(rt, name) {
            return Some(c);
        }
    }
    None
}

fn container_image(runtime: ContainerRuntime, name: &str) -> Option<String> {
    let output = Command::new(runtime.cli())
        .args(["inspect", name, "--format", "{{.Config.Image}}"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let img = String::from_utf8(output.stdout).ok()?;
    let trimmed = img.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn is_legacy_sonar_image(image: &str) -> bool {
    let lower = image.to_ascii_lowercase();
    lower.contains("lts-community") || lower.contains("sonarqube:9.") || lower.contains("9.9.")
}

fn container_env(runtime: ContainerRuntime, name: &str) -> Vec<String> {
    let output = Command::new(runtime.cli())
        .args(["inspect", name, "--format", "{{range .Config.Env}}{{println .}}{{end}}"])
        .output()
        .ok();
    let Some(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect()
}

fn container_uses_postgres(runtime: ContainerRuntime, sonar_name: &str) -> bool {
    if find_container(runtime, &db_container_name(sonar_name)).is_some() {
        return true;
    }
    container_env(runtime, sonar_name)
        .iter()
        .any(|e| e.starts_with("SONAR_JDBC_URL=") && e.contains("postgresql"))
}

fn sonar_stack_needs_postgres_upgrade(runtime: ContainerRuntime, sonar_name: &str) -> bool {
    find_container(runtime, sonar_name).is_some() && !container_uses_postgres(runtime, sonar_name)
}

fn ensure_network(runtime: ContainerRuntime, log: &InstallLog) -> Result<(), String> {
    if Command::new(runtime.cli())
        .args(["network", "inspect", SONAR_NETWORK])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
    {
        return Ok(());
    }
    log.push(format!("$ {} network create {SONAR_NETWORK}", runtime.cli()));
    let status = Command::new(runtime.cli())
        .args(["network", "create", SONAR_NETWORK])
        .status()
        .map_err(|e| format!("{} network create failed: {e}", runtime.cli()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} network create {SONAR_NETWORK} failed", runtime.cli()))
    }
}

fn remove_container(runtime: ContainerRuntime, name: &str) -> Result<(), String> {
    if let Some(existing) = find_container(runtime, name) {
        if existing.running {
            let _ = stop_container(runtime, name);
        }
    }
    let status = Command::new(runtime.cli())
        .args(["rm", "-f", name])
        .status()
        .map_err(|e| format!("{} rm failed: {e}", runtime.cli()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} rm -f {name} failed", runtime.cli()))
    }
}

fn remove_container_logged(runtime: ContainerRuntime, name: &str, log: &InstallLog) -> Result<(), String> {
    log.push(format!("$ {} rm -f {name}", runtime.cli()));
    remove_container(runtime, name).map_err(|e| {
        log.push(format!("ERROR: {e}"));
        e
    })
}

fn jdbc_url(db_name: &str) -> String {
    format!("jdbc:postgresql://{db_name}:5432/{POSTGRES_DB}")
}

fn wait_for_postgres_sync(
    runtime: ContainerRuntime,
    db_name: &str,
    timeout_secs: u64,
    log: &InstallLog,
) -> Result<(), String> {
    let deadline = std::time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut waited = 0u64;
    loop {
        if std::time::Instant::now() > deadline {
            let msg = format!("PostgreSQL ({db_name}) did not become ready within {timeout_secs}s");
            log.push(format!("ERROR: {msg}"));
            return Err(msg);
        }
        let ready = Command::new(runtime.cli())
            .args(["exec", db_name, "pg_isready", "-U", POSTGRES_USER, "-d", POSTGRES_DB])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ready {
            return Ok(());
        }
        if waited > 0 && waited % 9 == 0 {
            log.push(format!("Waiting for PostgreSQL ({db_name})… ({waited}s)"));
        }
        waited += 3;
        std::thread::sleep(Duration::from_secs(3));
    }
}

fn create_postgres_container(runtime: ContainerRuntime, db_name: &str, log: &InstallLog) -> Result<(), String> {
    if find_container(runtime, db_name).is_some() {
        return Ok(());
    }
    ensure_network(runtime, log)?;
    log.push(format!(
        "$ {} run -d --name {db_name} --network {SONAR_NETWORK} {POSTGRES_IMAGE}",
        runtime.cli()
    ));
    let status = Command::new(runtime.cli())
        .args([
            "run",
            "-d",
            "--name",
            db_name,
            "--network",
            SONAR_NETWORK,
            "--restart",
            "unless-stopped",
            "-e",
            &format!("POSTGRES_USER={POSTGRES_USER}"),
            "-e",
            &format!("POSTGRES_PASSWORD={POSTGRES_PASSWORD}"),
            "-e",
            &format!("POSTGRES_DB={POSTGRES_DB}"),
            "-v",
            &format!("{PG_VOLUME}:/var/lib/postgresql/data"),
            POSTGRES_IMAGE,
        ])
        .status()
        .map_err(|e| format!("{} run postgres failed: {e}", runtime.cli()))?;
    if !status.success() {
        let msg = format!("{} run {db_name} failed with {status}", runtime.cli());
        log.push(format!("ERROR: {msg}"));
        return Err(msg);
    }
    log.push(format!("PostgreSQL container '{db_name}' created."));
    wait_for_postgres_sync(runtime, db_name, 60, log)
}

fn sonar_run_args(host_port: u16, sonar_name: &str, db_name: &str) -> Vec<String> {
    vec![
        "run".into(),
        "-d".into(),
        "--name".into(),
        sonar_name.into(),
        "--network".into(),
        SONAR_NETWORK.into(),
        "-p".into(),
        format!("{host_port}:9000"),
        "--shm-size".into(),
        SONAR_SHM_SIZE.into(),
        "--memory".into(),
        SONAR_CONTAINER_MEMORY.into(),
        "--restart".into(),
        "unless-stopped".into(),
        "-e".into(),
        format!("SONAR_JDBC_URL={}", jdbc_url(db_name)),
        "-e".into(),
        format!("SONAR_JDBC_USERNAME={POSTGRES_USER}"),
        "-e".into(),
        format!("SONAR_JDBC_PASSWORD={POSTGRES_PASSWORD}"),
        "-e".into(),
        format!("SONAR_CE_JAVAOPTS={SONAR_CE_JAVAOPTS}"),
        "-e".into(),
        format!("SONAR_WEB_JAVAOPTS={SONAR_WEB_JAVAOPTS}"),
        "-v".into(),
        format!("{SONAR_DATA_VOLUME}:/opt/sonarqube/data"),
        "-v".into(),
        format!("{SONAR_EXTENSIONS_VOLUME}:/opt/sonarqube/extensions"),
        "-v".into(),
        format!("{SONAR_LOGS_VOLUME}:/opt/sonarqube/logs"),
        SONAR_IMAGE.into(),
    ]
}

fn create_sonar_app_container(
    runtime: ContainerRuntime,
    sonar_name: &str,
    db_name: &str,
    host_port: u16,
    log: &InstallLog,
) -> Result<(), String> {
    if find_container(runtime, sonar_name).is_some() {
        return Ok(());
    }
    ensure_network(runtime, log)?;
    create_postgres_container(runtime, db_name, log)?;
    log.push(format!(
        "$ {} run -d --name {sonar_name} --network {SONAR_NETWORK} -p {host_port}:9000 --memory {SONAR_CONTAINER_MEMORY} {SONAR_IMAGE}",
        runtime.cli()
    ));
    let args = sonar_run_args(host_port, sonar_name, db_name);
    let status = Command::new(runtime.cli())
        .args(&args)
        .status()
        .map_err(|e| format!("{} run failed: {e}", runtime.cli()))?;
    if status.success() {
        log.push(format!(
            "SonarQube container '{sonar_name}' created (PostgreSQL backend, {} RAM, CE heap 2g).",
            SONAR_CONTAINER_MEMORY
        ));
        Ok(())
    } else {
        let msg = format!("{} run {sonar_name} failed with {status}", runtime.cli());
        log.push(format!("ERROR: {msg}"));
        Err(msg)
    }
}

/// Start PostgreSQL sidecar (when present) then SonarQube.
pub fn start_sonar_stack(runtime: ContainerRuntime, sonar_name: &str, log: &InstallLog) -> Result<(), String> {
    let db_name = db_container_name(sonar_name);
    if let Some(db) = find_container(runtime, &db_name) {
        if !db.running {
            start_container_logged(runtime, &db_name, log)?;
            wait_for_postgres_sync(runtime, &db_name, 60, log)?;
        }
    } else if find_container(runtime, sonar_name).is_some() {
        return Err(format!(
            "SonarQube container '{sonar_name}' exists without PostgreSQL sidecar — run Install to upgrade the stack"
        ));
    }
    start_container_logged(runtime, sonar_name, log)
}

fn stop_sonar_stack(runtime: ContainerRuntime, sonar_name: &str, log: &InstallLog) -> Result<(), String> {
    let db_name = db_container_name(sonar_name);
    if let Some(sonar) = find_container(runtime, sonar_name) {
        if sonar.running {
            log.push(format!("$ {} stop {sonar_name}", runtime.cli()));
            stop_container(runtime, sonar_name).map_err(|e| {
                log.push(format!("ERROR: {e}"));
                e
            })?;
            log.push("SonarQube container stopped.");
        }
    }
    if let Some(db) = find_container(runtime, &db_name) {
        if db.running {
            log.push(format!("$ {} stop {db_name}", runtime.cli()));
            stop_container(runtime, &db_name).map_err(|e| {
                log.push(format!("ERROR: {e}"));
                e
            })?;
            log.push("PostgreSQL container stopped.");
        }
    }
    Ok(())
}

pub async fn ensure_sonar_live(
    host: &str,
    container_name: &str,
    preferred: Option<&str>,
    host_port: u16,
) -> Result<SonarDiscovery, String> {
    ensure_sonar_live_with_log(host, container_name, preferred, host_port, &InstallLog::new()).await
}

pub async fn ensure_sonar_live_with_log(
    host: &str,
    container_name: &str,
    preferred: Option<&str>,
    host_port: u16,
    log: &InstallLog,
) -> Result<SonarDiscovery, String> {
    log.push(format!(
        "Resolving container runtime (preference: {})…",
        preferred.unwrap_or("auto")
    ));
    let runtime = match resolve_runtime(preferred) {
        Ok(rt) => {
            log.push(format!("Using {}", rt.cli()));
            rt
        }
        Err(e) => {
            log.push(format!("ERROR: {e}"));
            return Err(e);
        }
    };

    if let Some(existing) = find_container(runtime, container_name) {
        log.push(format!(
            "Container '{}' found — {}",
            existing.name, existing.status
        ));
        if let Some(img) = container_image(runtime, container_name) {
            log.push(format!("Container image: {img}"));
            if is_legacy_sonar_image(&img) {
                log.push(format!("NOTE: {SONAR_UPGRADE_HINT}"));
            }
        }
        if sonar_stack_needs_postgres_upgrade(runtime, container_name) {
            log.push(SONAR_POSTGRES_UPGRADE_HINT.to_string());
            remove_container_logged(runtime, container_name, log)?;
            create_sonar_container_logged(runtime, container_name, host_port, log).await?;
        } else {
            // Always attempt start when the container exists (idempotent if already up; 3 retries).
            if existing.running {
                log.push("Container reported running — ensuring stack is started…");
            }
            start_sonar_stack(runtime, container_name, log)?;
        }
    } else {
        log.push(format!(
            "No container '{container_name}' — creating PostgreSQL-backed stack with {SONAR_IMAGE}."
        ));
        create_sonar_container_logged(runtime, container_name, host_port, log).await?;
    }

    if !sonar_reachable_sync(host) {
        log.push(format!("Waiting for SonarQube API at {host} (up to 180s)…"));
        wait_for_sonar_logged(host, 180, log).await?;
        log.push("SonarQube API is reachable.");
    } else {
        log.push("SonarQube API already reachable.");
    }

    log.push("Done.");
    Ok(discover_sonar(host, container_name, Some(runtime.cli())))
}

pub async fn start_sonar_container_with_log(
    host: &str,
    container_name: &str,
    preferred: Option<&str>,
    log: &InstallLog,
) -> Result<SonarDiscovery, String> {
    log.push("Starting SonarQube container…");
    let runtime = match resolve_runtime(preferred) {
        Ok(rt) => {
            log.push(format!("Using {}", rt.cli()));
            rt
        }
        Err(e) => {
            log.push(format!("ERROR: {e}"));
            return Err(e);
        }
    };

    let Some(existing) = find_container(runtime, container_name) else {
        let msg = format!("Container '{container_name}' not found — run Install first");
        log.push(format!("ERROR: {msg}"));
        return Err(msg);
    };
    log.push(format!("Container '{}' — {}", existing.name, existing.status));
    if sonar_stack_needs_postgres_upgrade(runtime, container_name) {
        let msg = format!(
            "Container '{container_name}' uses embedded H2 — run Install to upgrade to PostgreSQL"
        );
        log.push(format!("ERROR: {msg}"));
        return Err(msg);
    }
    // Always attempt start when the container exists (idempotent if already up; 3 retries).
    if existing.running {
        log.push("Container reported running — ensuring stack is started…");
    }
    start_sonar_stack(runtime, container_name, log)?;

    if !sonar_reachable_sync(host) {
        log.push(format!("Waiting for SonarQube API at {host}…"));
        wait_for_sonar_logged(host, 180, log).await?;
    } else {
        log.push("SonarQube API already reachable.");
    }

    log.push("Done.");
    Ok(discover_sonar(host, container_name, Some(runtime.cli())))
}

pub async fn stop_sonar_container_with_log(
    host: &str,
    container_name: &str,
    log: &InstallLog,
) -> Result<SonarDiscovery, String> {
    log.push("Stopping SonarQube container…");
    let Some(existing) = find_container_any(container_name) else {
        let msg = format!("Container '{container_name}' not found");
        log.push(format!("ERROR: {msg}"));
        return Err(msg);
    };
    log.push(format!(
        "Container '{}' on {} — {}",
        existing.name,
        existing.runtime.cli(),
        existing.status
    ));
    if existing.running {
        stop_sonar_stack(existing.runtime, container_name, log)?;
    } else {
        log.push("Container is not running.");
    }
    log.push("Done.");
    Ok(discover_sonar(
        host,
        container_name,
        Some(existing.runtime.cli()),
    ))
}

fn start_container_logged(
    runtime: ContainerRuntime,
    name: &str,
    log: &InstallLog,
) -> Result<(), String> {
    log.push(format!("$ {} start {name}", runtime.cli()));
    start_container(runtime, name).map_err(|e| {
        log.push(format!("ERROR: {e}"));
        e
    })
}

async fn pull_image_logged(
    runtime: ContainerRuntime,
    image: &str,
    log: &InstallLog,
) -> Result<(), String> {
    log.push(format!("$ {} pull {image}", runtime.cli()));
    let mut child = AsyncCommand::new(runtime.cli())
        .args(["pull", image])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("{} pull failed: {e}", runtime.cli()))?;

    if let Some(stdout) = child.stdout.take() {
        let log = log.clone();
        tokio::spawn(async move {
            let mut lines = AsyncBufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    log.push(trimmed.to_string());
                }
            }
        });
    }

    if let Some(stderr) = child.stderr.take() {
        let log = log.clone();
        tokio::spawn(async move {
            let mut lines = AsyncBufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    log.push(trimmed.to_string());
                }
            }
        });
    }

    let status = child
        .wait()
        .await
        .map_err(|e| format!("{} pull failed: {e}", runtime.cli()))?;
    if status.success() {
        log.push(format!("Image pull finished ({image})."));
        Ok(())
    } else {
        let msg = format!("{} pull {image} failed with {status}", runtime.cli());
        log.push(format!("ERROR: {msg}"));
        Err(msg)
    }
}

async fn create_sonar_container_logged(
    runtime: ContainerRuntime,
    name: &str,
    host_port: u16,
    log: &InstallLog,
) -> Result<(), String> {
    if find_container(runtime, name).is_some() {
        return start_sonar_stack(runtime, name, log);
    }
    pull_image_logged(runtime, POSTGRES_IMAGE, log).await?;
    pull_image_logged(runtime, SONAR_IMAGE, log).await?;
    let db_name = db_container_name(name);
    create_sonar_app_container(runtime, name, &db_name, host_port, log)
}

async fn wait_for_sonar_logged(host: &str, timeout_secs: u64, log: &InstallLog) -> Result<(), String> {
    let url = format!("{}/api/system/status", host.trim_end_matches('/'));
    let client = reqwest::Client::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs);
    let mut waited = 0u64;
    loop {
        if tokio::time::Instant::now() > deadline {
            let msg = format!("SonarQube did not become ready at {host} within {timeout_secs}s");
            log.push(format!("ERROR: {msg}"));
            return Err(msg);
        }
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        if waited > 0 && waited % 15 == 0 {
            log.push(format!("Still waiting… ({waited}s)"));
        }
        waited += 3;
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_sidecar_name_derived_from_sonar_name() {
        assert_eq!(db_container_name("sonarqube"), "sonarqube-db");
    }

    #[test]
    fn jdbc_url_points_at_sidecar() {
        assert_eq!(
            jdbc_url("sonarqube-db"),
            "jdbc:postgresql://sonarqube-db:5432/sonar"
        );
    }

    #[test]
    fn sonar_host_port_parsed_from_url() {
        assert_eq!(sonar_host_port("http://localhost:9000"), 9000);
        assert_eq!(sonar_host_port("http://127.0.0.1:9100/"), 9100);
        assert_eq!(sonar_host_port("http://localhost"), 9000);
    }
}
