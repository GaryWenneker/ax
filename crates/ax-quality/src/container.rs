//! Podman/Docker runtime discovery and SonarQube container lifecycle.

use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, BufReader as AsyncBufReader};
use tokio::process::Command as AsyncCommand;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

use crate::process::{run_with_timeout, CmdOutput};

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

/// Program to run for `runtime`. Tests point this at a fake CLI.
fn program(runtime: ContainerRuntime) -> std::ffi::OsString {
    #[cfg(test)]
    if let Some(dir) = test_hooks::cli_dir() {
        return dir.join(runtime.cli()).into_os_string();
    }
    runtime.cli().into()
}

/// Longest a read-only CLI call (`ps`, `inspect`, `--version`, `exec`) may take.
const CLI_QUERY_TIMEOUT: Duration = Duration::from_secs(30);
/// Longest `start`, `stop`, `rm` or `network create` may take.
const CLI_ACTION_TIMEOUT: Duration = Duration::from_secs(120);
/// Longest `run -d` may take (the image is pulled before).
const CLI_RUN_TIMEOUT: Duration = Duration::from_secs(300);
/// Longest an image pull may take.
const PULL_TIMEOUT: Duration = Duration::from_secs(30 * 60);

fn capped(timeout: Duration) -> Duration {
    #[cfg(test)]
    if let Some(cap) = test_hooks::timeout_cap() {
        return timeout.min(cap);
    }
    timeout
}

/// Runs the container CLI with `args`, output captured, within `timeout`.
fn run_cli(
    runtime: ContainerRuntime,
    args: &[&str],
    timeout: Duration,
) -> Result<CmdOutput, String> {
    let mut cmd = Command::new(program(runtime));
    cmd.args(args);
    run_with_timeout(&mut cmd, capped(timeout)).map_err(|e| {
        format!(
            "{} {}: {e}",
            runtime.cli(),
            args.first().copied().unwrap_or_default()
        )
    })
}

/// Like [`run_cli`], but a non-zero exit is an error naming the command.
fn run_cli_ok(runtime: ContainerRuntime, args: &[&str], timeout: Duration) -> Result<(), String> {
    let out = run_cli(runtime, args, timeout)?;
    if out.success() {
        return Ok(());
    }
    let mut msg = format!(
        "{} {} exited with {}",
        runtime.cli(),
        args.join(" "),
        out.status
    );
    if let Some(line) = out.stderr_tail_line() {
        msg.push_str(": ");
        msg.push_str(line);
    }
    Err(msg)
}

/// Stdout of a successful read-only CLI call.
fn cli_query(runtime: ContainerRuntime, args: &[&str]) -> Option<String> {
    run_cli(runtime, args, CLI_QUERY_TIMEOUT)
        .ok()
        .filter(CmdOutput::success)
        .map(|o| o.stdout)
}

/// Runs synchronous container work on the blocking pool, so an async caller
/// does not stall its runtime worker.
pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| format!("container task failed: {e}"))?
}

#[cfg(test)]
pub(crate) mod test_hooks {
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Duration;

    static CLI_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);
    static TIMEOUT_CAP: Mutex<Option<Duration>> = Mutex::new(None);
    /// Serializes tests that install a fake CLI.
    pub static LOCK: Mutex<()> = Mutex::new(());

    pub fn cli_dir() -> Option<PathBuf> {
        CLI_DIR.lock().ok()?.clone()
    }
    pub fn set_cli_dir(dir: Option<PathBuf>) {
        *CLI_DIR.lock().unwrap() = dir;
    }
    pub fn timeout_cap() -> Option<Duration> {
        *TIMEOUT_CAP.lock().ok()?
    }
    pub fn set_timeout_cap(cap: Option<Duration>) {
        *TIMEOUT_CAP.lock().unwrap() = cap;
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
    let text = cli_query(runtime, &["--version"])?;
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
    let stdout = cli_query(
        runtime,
        &[
            "ps",
            "-a",
            "--filter",
            &format!("name=^{name}$"),
            "--format",
            "{{.Names}}\t{{.Status}}\t{{.Ports}}",
        ],
    )?;
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
    let database = container
        .as_ref()
        .and_then(|c| find_container(c.runtime, &db_container_name(container_name)));
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
    let (h, n, p) = (host.to_string(), name.to_string(), pref.map(str::to_string));
    let online =
        blocking(
            move || Ok(sonar_reachable_sync(&h).then(|| discover_sonar(&h, &n, p.as_deref()))),
        )
        .await?;
    if let Some(discovery) = online {
        return Ok(discovery);
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

/// Fast liveness ping to SonarQube `/api/system/status` (shared client,
/// ~800ms timeout): true for any successful answer, including `STARTING`.
/// Use [`wait_for_sonar`] or `sonar_reachable` when the API must be ready.
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

/// Longest time one readiness request may take.
const SONAR_STATUS_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// Pause between readiness requests.
const SONAR_STATUS_POLL: Duration = Duration::from_secs(3);

/// Whether the host part of a SonarQube URL is a loopback name
/// (`localhost`, `127.x.x.x` or `::1`).
pub fn is_loopback_sonar_host(host: &str) -> bool {
    let rest = host.split_once("://").map_or(host, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit_once('@').map_or(authority, |(_, a)| a);
    let name = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next().unwrap_or("")
    } else {
        authority.split(':').next().unwrap_or("")
    };
    let name = name.to_ascii_lowercase();
    name == "localhost"
        || name == "::1"
        || name
            .parse::<std::net::Ipv4Addr>()
            .is_ok_and(|ip| ip.is_loopback())
}

/// Extra `run` arguments and the SonarQube URL for a scanner container.
/// When SonarQube is the ax-managed container on [`SONAR_NETWORK`]
/// (`sonar_on_network` names it), the scanner joins that network and talks
/// to the container directly, because the port is published on the host's
/// loopback only. Otherwise the configured host is used, with `localhost`
/// mapped to `host.containers.internal`.
pub fn scanner_container_target(
    host: &str,
    sonar_on_network: Option<&str>,
) -> (Vec<String>, String) {
    match sonar_on_network {
        Some(name) if is_loopback_sonar_host(host) => (
            vec!["--network".into(), SONAR_NETWORK.into()],
            format!("http://{name}:9000"),
        ),
        _ => (Vec::new(), crate::bootstrap::scanner_host_url(host)),
    }
}

/// Whether container `name` is attached to [`SONAR_NETWORK`].
pub fn container_on_sonar_network(runtime: ContainerRuntime, name: &str) -> bool {
    cli_query(
        runtime,
        &[
            "inspect",
            "--format",
            "{{json .NetworkSettings.Networks}}",
            name,
        ],
    )
    .is_some_and(|out| out.contains(SONAR_NETWORK))
}

/// `-p` value for the SonarQube container: loopback only when the
/// configured host is a loopback name, so the instance (which ax drives with
/// the default admin login) is not reachable from the network. Configure a
/// non-loopback `host` to publish it on all interfaces.
fn sonar_publish_arg(host: &str, host_port: u16) -> String {
    if is_loopback_sonar_host(host) {
        format!("127.0.0.1:{host_port}:9000")
    } else {
        format!("{host_port}:9000")
    }
}

/// Whether a `/api/system/status` body says the server is `UP`. SonarQube
/// answers HTTP 200 with `STARTING` or `DB_MIGRATION_NEEDED` before its API
/// can serve requests, so the status code alone does not mean ready.
pub fn sonar_status_body_is_up(body: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("status")?.as_str().map(|s| s == "UP"))
        .unwrap_or(false)
}

/// Asks `/api/system/status` once and reports whether SonarQube is `UP`.
pub async fn sonar_status_up(client: &reqwest::Client, host: &str) -> bool {
    let url = format!("{}/api/system/status", host.trim_end_matches('/'));
    let Ok(resp) = client.get(&url).send().await else {
        return false;
    };
    if !resp.status().is_success() {
        return false;
    }
    resp.text()
        .await
        .map(|body| sonar_status_body_is_up(&body))
        .unwrap_or(false)
}

fn sonar_status_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(SONAR_STATUS_REQUEST_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Polls until SonarQube reports `UP` or `timeout_secs` pass. Each request
/// is cut off at the request timeout and at the deadline, so a hung request
/// cannot outlive it. `tick` gets the seconds waited before each pause.
async fn wait_until_up(
    host: &str,
    timeout_secs: u64,
    mut tick: impl FnMut(u64),
) -> Result<(), String> {
    let client = sonar_status_client();
    let started = tokio::time::Instant::now();
    let deadline = started + Duration::from_secs(timeout_secs);
    let timed_out = || format!("SonarQube did not become ready at {host} within {timeout_secs}s");
    loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err(timed_out());
        }
        let up = tokio::time::timeout(deadline - now, sonar_status_up(&client, host))
            .await
            .unwrap_or(false);
        if up {
            return Ok(());
        }
        tick(started.elapsed().as_secs());
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err(timed_out());
        }
        tokio::time::sleep(SONAR_STATUS_POLL.min(deadline - now)).await;
    }
}

/// Waits until SonarQube reports `UP`, for at most `timeout_secs`.
pub async fn wait_for_sonar(host: &str, timeout_secs: u64) -> Result<(), String> {
    wait_until_up(host, timeout_secs, |_| {}).await
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
                    std::thread::sleep(capped(Duration::from_secs(START_CONTAINER_RETRY_SECS)));
                }
            }
        }
    }
    Err(format!(
        "{last_err} (after {START_CONTAINER_ATTEMPTS} start attempts)"
    ))
}

fn start_container_once(runtime: ContainerRuntime, name: &str) -> Result<(), String> {
    run_cli_ok(runtime, &["start", name], CLI_ACTION_TIMEOUT)
}

pub fn stop_container(runtime: ContainerRuntime, name: &str) -> Result<(), String> {
    info!("Stopping container {name} via {}", runtime.cli());
    run_cli_ok(runtime, &["stop", name], CLI_ACTION_TIMEOUT)
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
    let img = cli_query(runtime, &["inspect", name, "--format", "{{.Config.Image}}"])?;
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
    let Some(out) = cli_query(
        runtime,
        &[
            "inspect",
            name,
            "--format",
            "{{range .Config.Env}}{{println .}}{{end}}",
        ],
    ) else {
        return Vec::new();
    };
    out.lines()
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
    if cli_query(runtime, &["network", "inspect", SONAR_NETWORK]).is_some() {
        return Ok(());
    }
    log.push(format!(
        "$ {} network create {SONAR_NETWORK}",
        runtime.cli()
    ));
    run_cli_ok(
        runtime,
        &["network", "create", SONAR_NETWORK],
        CLI_ACTION_TIMEOUT,
    )
}

fn remove_container(runtime: ContainerRuntime, name: &str) -> Result<(), String> {
    if let Some(existing) = find_container(runtime, name) {
        if existing.running {
            let _ = stop_container(runtime, name);
        }
    }
    run_cli_ok(runtime, &["rm", "-f", name], CLI_ACTION_TIMEOUT)
}

fn remove_container_logged(
    runtime: ContainerRuntime,
    name: &str,
    log: &InstallLog,
) -> Result<(), String> {
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
        let ready = cli_query(
            runtime,
            &[
                "exec",
                db_name,
                "pg_isready",
                "-U",
                POSTGRES_USER,
                "-d",
                POSTGRES_DB,
            ],
        )
        .is_some();
        if ready {
            return Ok(());
        }
        if waited > 0 && waited.is_multiple_of(9) {
            log.push(format!("Waiting for PostgreSQL ({db_name})… ({waited}s)"));
        }
        waited += 3;
        std::thread::sleep(capped(Duration::from_secs(3)));
    }
}

fn create_postgres_container(
    runtime: ContainerRuntime,
    db_name: &str,
    log: &InstallLog,
) -> Result<(), String> {
    if find_container(runtime, db_name).is_some() {
        return Ok(());
    }
    ensure_network(runtime, log)?;
    log.push(format!(
        "$ {} run -d --name {db_name} --network {SONAR_NETWORK} {POSTGRES_IMAGE}",
        runtime.cli()
    ));
    run_cli_ok(
        runtime,
        &[
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
        ],
        CLI_RUN_TIMEOUT,
    )
    .inspect_err(|msg| log.push(format!("ERROR: {msg}")))?;
    log.push(format!("PostgreSQL container '{db_name}' created."));
    wait_for_postgres_sync(runtime, db_name, 60, log)
}

fn sonar_run_args(host: &str, host_port: u16, sonar_name: &str, db_name: &str) -> Vec<String> {
    vec![
        "run".into(),
        "-d".into(),
        "--name".into(),
        sonar_name.into(),
        "--network".into(),
        SONAR_NETWORK.into(),
        "-p".into(),
        sonar_publish_arg(host, host_port),
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
    host: &str,
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
        "$ {} run -d --name {sonar_name} --network {SONAR_NETWORK} -p {} --memory {SONAR_CONTAINER_MEMORY} {SONAR_IMAGE}",
        runtime.cli(),
        sonar_publish_arg(host, host_port)
    ));
    let args = sonar_run_args(host, host_port, sonar_name, db_name);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    run_cli_ok(runtime, &args, CLI_RUN_TIMEOUT)
        .inspect_err(|msg| log.push(format!("ERROR: {msg}")))?;
    log.push(format!(
        "SonarQube container '{sonar_name}' created (PostgreSQL backend, {} RAM, CE heap 2g).",
        SONAR_CONTAINER_MEMORY
    ));
    Ok(())
}

/// Start PostgreSQL sidecar (when present) then SonarQube.
pub fn start_sonar_stack(
    runtime: ContainerRuntime,
    sonar_name: &str,
    log: &InstallLog,
) -> Result<(), String> {
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

fn stop_sonar_stack(
    runtime: ContainerRuntime,
    sonar_name: &str,
    log: &InstallLog,
) -> Result<(), String> {
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
    ensure_sonar_live_with_log(
        host,
        container_name,
        preferred,
        host_port,
        &InstallLog::new(),
    )
    .await
}

/// What install does after looking at the existing container.
enum InstallStep {
    /// The stack was started; nothing to create.
    Started,
    /// Pull the images and create the stack.
    Create,
}

pub async fn ensure_sonar_live_with_log(
    host: &str,
    container_name: &str,
    preferred: Option<&str>,
    host_port: u16,
    log: &InstallLog,
) -> Result<SonarDiscovery, String> {
    let (name, pref, lg) = (
        container_name.to_string(),
        preferred.map(str::to_string),
        log.clone(),
    );
    let (runtime, step) = blocking(move || {
        let log = &lg;
        let runtime = resolve_runtime_logged(pref.as_deref(), log)?;
        let Some(existing) = find_container(runtime, &name) else {
            log.push(format!(
                "No container '{name}' — creating PostgreSQL-backed stack with {SONAR_IMAGE}."
            ));
            return Ok((runtime, InstallStep::Create));
        };
        log.push(format!(
            "Container '{}' found — {}",
            existing.name, existing.status
        ));
        if let Some(img) = container_image(runtime, &name) {
            log.push(format!("Container image: {img}"));
            if is_legacy_sonar_image(&img) {
                log.push(format!("NOTE: {SONAR_UPGRADE_HINT}"));
            }
        }
        if sonar_stack_needs_postgres_upgrade(runtime, &name) {
            log.push(SONAR_POSTGRES_UPGRADE_HINT.to_string());
            remove_container_logged(runtime, &name, log)?;
            return Ok((runtime, InstallStep::Create));
        }
        // Always attempt start when the container exists (idempotent if already up; 3 retries).
        if existing.running {
            log.push("Container reported running — ensuring stack is started…");
        }
        start_sonar_stack(runtime, &name, log)?;
        Ok((runtime, InstallStep::Started))
    })
    .await?;
    if matches!(step, InstallStep::Create) {
        create_sonar_container_logged(runtime, host, container_name, host_port, log).await?;
    }
    finish_with_api(host, container_name, runtime, " (up to 180s)", log).await
}

/// Waits for the API when it is not up yet, then reports the stack.
async fn finish_with_api(
    host: &str,
    container_name: &str,
    runtime: ContainerRuntime,
    wait_note: &str,
    log: &InstallLog,
) -> Result<SonarDiscovery, String> {
    let h = host.to_string();
    if !blocking(move || Ok(sonar_reachable_sync(&h))).await? {
        log.push(format!("Waiting for SonarQube API at {host}{wait_note}…"));
        wait_for_sonar_logged(host, 180, log).await?;
        log.push("SonarQube API is reachable.");
    } else {
        log.push("SonarQube API already reachable.");
    }
    log.push("Done.");
    let (h, n) = (host.to_string(), container_name.to_string());
    blocking(move || Ok(discover_sonar(&h, &n, Some(runtime.cli())))).await
}

fn resolve_runtime_logged(
    preferred: Option<&str>,
    log: &InstallLog,
) -> Result<ContainerRuntime, String> {
    log.push(format!(
        "Resolving container runtime (preference: {})…",
        preferred.unwrap_or("auto")
    ));
    resolve_runtime(preferred)
        .inspect(|rt| log.push(format!("Using {}", rt.cli())))
        .inspect_err(|e| log.push(format!("ERROR: {e}")))
}

pub async fn start_sonar_container_with_log(
    host: &str,
    container_name: &str,
    preferred: Option<&str>,
    log: &InstallLog,
) -> Result<SonarDiscovery, String> {
    log.push("Starting SonarQube container…");
    let (name, pref, lg) = (
        container_name.to_string(),
        preferred.map(str::to_string),
        log.clone(),
    );
    let runtime = blocking(move || {
        let log = &lg;
        let runtime = resolve_runtime_logged(pref.as_deref(), log)?;
        let Some(existing) = find_container(runtime, &name) else {
            let msg = format!("Container '{name}' not found — run Install first");
            log.push(format!("ERROR: {msg}"));
            return Err(msg);
        };
        log.push(format!(
            "Container '{}' — {}",
            existing.name, existing.status
        ));
        if sonar_stack_needs_postgres_upgrade(runtime, &name) {
            let msg = format!(
                "Container '{name}' uses embedded H2 — run Install to upgrade to PostgreSQL"
            );
            log.push(format!("ERROR: {msg}"));
            return Err(msg);
        }
        // Always attempt start when the container exists (idempotent if already up; 3 retries).
        if existing.running {
            log.push("Container reported running — ensuring stack is started…");
        }
        start_sonar_stack(runtime, &name, log)?;
        Ok(runtime)
    })
    .await?;
    finish_with_api(host, container_name, runtime, "", log).await
}

pub async fn stop_sonar_container_with_log(
    host: &str,
    container_name: &str,
    log: &InstallLog,
) -> Result<SonarDiscovery, String> {
    let (host, name, log) = (host.to_string(), container_name.to_string(), log.clone());
    blocking(move || {
        log.push("Stopping SonarQube container…");
        let Some(existing) = find_container_any(&name) else {
            let msg = format!("Container '{name}' not found");
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
            stop_sonar_stack(existing.runtime, &name, &log)?;
        } else {
            log.push("Container is not running.");
        }
        log.push("Done.");
        Ok(discover_sonar(&host, &name, Some(existing.runtime.cli())))
    })
    .await
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
    let mut child = AsyncCommand::new(program(runtime))
        .args(["pull", image])
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
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

    let status = match tokio::time::timeout(capped(PULL_TIMEOUT), child.wait()).await {
        Ok(status) => status.map_err(|e| format!("{} pull failed: {e}", runtime.cli()))?,
        Err(_) => {
            let _ = child.kill().await;
            let msg = format!(
                "{} pull {image} timed out after {}s — killed",
                runtime.cli(),
                PULL_TIMEOUT.as_secs()
            );
            log.push(format!("ERROR: {msg}"));
            return Err(msg);
        }
    };
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
    host: &str,
    name: &str,
    host_port: u16,
    log: &InstallLog,
) -> Result<(), String> {
    let (n, lg) = (name.to_string(), log.clone());
    let exists = blocking(move || {
        if find_container(runtime, &n).is_some() {
            start_sonar_stack(runtime, &n, &lg)?;
            return Ok(true);
        }
        Ok(false)
    })
    .await?;
    if exists {
        return Ok(());
    }
    pull_image_logged(runtime, POSTGRES_IMAGE, log).await?;
    pull_image_logged(runtime, SONAR_IMAGE, log).await?;
    let (h, n, lg) = (host.to_string(), name.to_string(), log.clone());
    blocking(move || {
        let db_name = db_container_name(&n);
        create_sonar_app_container(runtime, &h, &n, &db_name, host_port, &lg)
    })
    .await
}

async fn wait_for_sonar_logged(
    host: &str,
    timeout_secs: u64,
    log: &InstallLog,
) -> Result<(), String> {
    let mut last_report = 0u64;
    wait_until_up(host, timeout_secs, |waited| {
        if waited >= last_report + 15 {
            last_report = waited - waited % 15;
            log.push(format!("Still waiting… ({waited}s)"));
        }
    })
    .await
    .inspect_err(|msg| log.push(format!("ERROR: {msg}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn publish_arg(args: &[String]) -> String {
        let i = args.iter().position(|a| a == "-p").expect("-p");
        args[i + 1].clone()
    }

    #[test]
    fn sonar_is_published_on_loopback_for_a_local_host() {
        for host in [
            "http://localhost:9000",
            "http://127.0.0.1:9000/",
            "http://[::1]:9000",
            "http://LOCALHOST:9000",
        ] {
            let args = sonar_run_args(host, 9000, "sonarqube", "sonarqube-db");
            assert_eq!(publish_arg(&args), "127.0.0.1:9000:9000", "{host}");
        }
    }

    #[test]
    fn sonar_is_published_on_all_interfaces_only_for_a_named_host() {
        let args = sonar_run_args("http://sonarqube.vsc:9100", 9100, "sonarqube", "db");
        assert_eq!(publish_arg(&args), "9100:9000");
    }

    #[test]
    fn scanner_reaches_an_ax_managed_sonar_over_its_network() {
        let (extra, host) = scanner_container_target("http://localhost:9000", Some("sonarqube"));
        assert_eq!(
            extra,
            vec!["--network".to_string(), SONAR_NETWORK.to_string()]
        );
        assert_eq!(host, "http://sonarqube:9000");
        let (extra, host) = scanner_container_target("http://localhost:9000", None);
        assert!(extra.is_empty());
        assert_eq!(host, "http://host.containers.internal:9000");
        let (extra, host) =
            scanner_container_target("http://sonarqube.vsc:9000", Some("sonarqube"));
        assert!(extra.is_empty());
        assert_eq!(host, "http://sonarqube.vsc:9000");
    }

    #[test]
    fn status_body_is_up_only_for_up() {
        assert!(sonar_status_body_is_up(
            r#"{"id":"x","version":"10","status":"UP"}"#
        ));
        for body in [
            r#"{"status":"STARTING"}"#,
            r#"{"status":"DB_MIGRATION_NEEDED"}"#,
            r#"{"status":"up"}"#,
            r#"{}"#,
            "",
            "<html>proxy</html>",
        ] {
            assert!(!sonar_status_body_is_up(body), "{body}");
        }
    }

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

#[cfg(all(test, unix))]
mod fake_cli_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::MutexGuard;

    const FAKE_PODMAN: &str = r#"#!/bin/sh
d="$(dirname "$0")"
printf '%s\n' "$*" >> "$d/calls.log"
if [ -f "$d/hang-$1" ]; then exec sleep 30; fi
case "$1" in
  --version) echo "podman version 5.0.0" ;;
  ps) n="${4#name=^}"; n="${n%\$}"; if [ -f "$d/ps-$n" ]; then cat "$d/ps-$n"; fi ;;
  inspect) for a in "$@"; do if [ -f "$d/inspect-$a" ]; then cat "$d/inspect-$a"; fi; done ;;
  start|stop) echo "$2" ;;
  *) exit 1 ;;
esac
"#;

    /// A fake `podman` on a private path; dropped, it restores the real CLI.
    pub(super) struct FakeCli {
        pub dir: tempfile::TempDir,
        _lock: MutexGuard<'static, ()>,
    }

    impl FakeCli {
        pub fn new() -> Self {
            let lock = test_hooks::LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let dir = tempfile::tempdir().unwrap();
            let script = dir.path().join("podman");
            std::fs::write(&script, FAKE_PODMAN).unwrap();
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
            test_hooks::set_cli_dir(Some(dir.path().to_path_buf()));
            Self { dir, _lock: lock }
        }
        pub fn file(&self, name: &str, body: &str) {
            std::fs::write(self.dir.path().join(name), body).unwrap();
        }
        pub fn calls(&self) -> String {
            std::fs::read_to_string(self.dir.path().join("calls.log")).unwrap_or_default()
        }
        #[allow(dead_code)]
        pub fn path(&self) -> PathBuf {
            Path::new(self.dir.path()).to_path_buf()
        }
    }

    impl Drop for FakeCli {
        fn drop(&mut self) {
            test_hooks::set_cli_dir(None);
            test_hooks::set_timeout_cap(None);
        }
    }

    /// Runs `fut` on a one-thread runtime next to a ticker; returns the
    /// result and how often the ticker ran, or `None` after `limit`.
    fn run_with_ticker<T: Send + 'static>(
        limit: Duration,
        fut: impl std::future::Future<Output = T> + Send + 'static,
    ) -> Option<(T, u64)> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let ticks = Arc::new(std::sync::atomic::AtomicU64::new(0));
                let t = ticks.clone();
                tokio::spawn(async move {
                    loop {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        t.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                });
                let out = fut.await;
                let _ = tx.send((out, ticks.load(std::sync::atomic::Ordering::Relaxed)));
            });
        });
        rx.recv_timeout(limit).ok()
    }

    #[test]
    fn a_stuck_cli_neither_hangs_install_nor_blocks_the_runtime() {
        let fake = FakeCli::new();
        fake.file("hang-ps", "");
        test_hooks::set_timeout_cap(Some(Duration::from_millis(500)));
        let log = InstallLog::new();
        let outcome = run_with_ticker(Duration::from_secs(20), async move {
            ensure_sonar_live_with_log("http://127.0.0.1:1", "sonarqube", Some("podman"), 1, &log)
                .await
        });
        let (result, ticks) = outcome.expect("install returned within 20s");
        assert!(result.is_err(), "fake pull fails, so install fails");
        // At least ~1s of CLI time passed; a blocked runtime would show ~0 ticks.
        assert!(ticks >= 30, "runtime was blocked: only {ticks} ticks");
        assert!(fake.calls().contains("ps -a"));
    }
}
