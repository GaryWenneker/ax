//! Podman/Docker runtime discovery and SonarQube container lifecycle.

use std::process::Command;
use std::time::Duration;

use serde::Serialize;
use tracing::info;

const SONAR_IMAGE: &str = "sonarqube:lts-community";

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
    let runtime = resolve_runtime(preferred)?;
    if let Some(existing) = find_container(runtime, container_name) {
        if !existing.running {
            start_container(runtime, container_name)?;
        }
    } else {
        create_sonar_container(runtime, container_name, host_port)?;
    }
    if !sonar_reachable_sync(host) {
        wait_for_sonar(host, 180).await?;
    }
    Ok(discover_sonar(host, container_name, Some(runtime.cli())))
}
