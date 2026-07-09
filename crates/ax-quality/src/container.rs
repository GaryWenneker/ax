//! Podman/Docker runtime discovery and SonarQube container lifecycle.

use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

const SONAR_IMAGE: &str = "sonarqube:lts-community";
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
    pub reachable: bool,
    pub host: String,
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
    let reachable = sonar_reachable_sync(host);
    SonarDiscovery {
        runtimes,
        preferred: preferred_rt,
        container,
        reachable,
        host: host.to_string(),
    }
}

fn sonar_reachable_sync(host: &str) -> bool {
    let url = format!("{}/api/system/status", host.trim_end_matches('/'));
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok()
        .and_then(|c| c.get(&url).send().ok())
        .map(|r| r.status().is_success())
        .unwrap_or(false)
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

pub fn start_container(runtime: ContainerRuntime, name: &str) -> Result<(), String> {
    info!("Starting container {name} via {}", runtime.cli());
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

pub fn pull_image(runtime: ContainerRuntime, image: &str) -> Result<(), String> {
    info!("Pulling {image} via {}", runtime.cli());
    let status = Command::new(runtime.cli())
        .args(["pull", image])
        .status()
        .map_err(|e| format!("{} pull failed: {e}", runtime.cli()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} pull {image} failed", runtime.cli()))
    }
}

pub fn create_sonar_container(
    runtime: ContainerRuntime,
    name: &str,
    host_port: u16,
) -> Result<(), String> {
    if find_container(runtime, name).is_some() {
        return start_container(runtime, name);
    }
    pull_image(runtime, SONAR_IMAGE)?;
    info!("Creating SonarQube container {name} on port {host_port}");
    let status = Command::new(runtime.cli())
        .args([
            "run",
            "-d",
            "--name",
            name,
            "-p",
            &format!("{host_port}:9000"),
            "--restart",
            "unless-stopped",
            SONAR_IMAGE,
        ])
        .status()
        .map_err(|e| format!("{} run failed: {e}", runtime.cli()))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{} run {name} failed", runtime.cli()))
    }
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
        if !existing.running {
            start_container_logged(runtime, container_name, log)?;
        } else {
            log.push("Container already running.");
        }
    } else {
        log.push(format!("No container '{container_name}' — creating a new instance."));
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
    if !existing.running {
        start_container_logged(runtime, container_name, log)?;
    } else {
        log.push("Container already running.");
    }

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
        log.push(format!("$ {} stop {container_name}", existing.runtime.cli()));
        stop_container(existing.runtime, container_name).map_err(|e| {
            log.push(format!("ERROR: {e}"));
            e
        })?;
        log.push("Container stopped.");
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
        return start_container_logged(runtime, name, log);
    }
    pull_image_logged(runtime, SONAR_IMAGE, log).await?;
    log.push(format!(
        "$ {} run -d --name {name} -p {host_port}:9000 {SONAR_IMAGE}",
        runtime.cli()
    ));
    let status = Command::new(runtime.cli())
        .args([
            "run",
            "-d",
            "--name",
            name,
            "-p",
            &format!("{host_port}:9000"),
            "--restart",
            "unless-stopped",
            SONAR_IMAGE,
        ])
        .status()
        .map_err(|e| format!("{} run failed: {e}", runtime.cli()))?;
    if status.success() {
        log.push(format!("Container '{name}' created and started."));
        Ok(())
    } else {
        let msg = format!("{} run {name} failed with {status}", runtime.cli());
        log.push(format!("ERROR: {msg}"));
        Err(msg)
    }
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
