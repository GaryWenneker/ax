//! SonarScanner CLI wrapper and Quality Gate API client.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;
use tracing::{info, warn};

/// Maximum time a single sonar-scanner invocation may run before being killed.
const SCAN_TIMEOUT: Duration = Duration::from_secs(600);

/// Directories excluded from every scan (build artifacts, deps, VCS).
const DEFAULT_EXCLUSIONS: &str = "\
**/target-dev/**,\
**/target/**,\
**/node_modules/**,\
**/.git/**,\
**/dist/**,\
**/*.exe,\
**/*.dll,\
**/*.pdb,\
**/*.so,\
**/*.dylib";

use crate::bootstrap::{
    canonical_repo_project_key, read_sonar_token, scanner_available, sonar_reachable,
    workspace_sonar_key,
};
use crate::container::{
    find_container_any, resolve_runtime, start_sonar_stack, InstallLog, SONAR_SCANNER_IMAGE,
};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SonarConfig {
    pub enabled: bool,
    pub host: String,
    pub project_key: String,
    pub token_env: String,
    pub scanner_path: String,
    pub podman_container: Option<String>,
    #[serde(default = "default_container_runtime")]
    pub container_runtime: String,
    /// `incremental` (changed files only) or `full` (entire project).
    #[serde(default = "default_scan_mode")]
    pub scan_mode: String,
    /// SonarQube web UI username (for setup and project status checks).
    #[serde(default = "default_admin_user")]
    pub admin_user: String,
    /// SonarQube web UI password (stored in ship.toml for local dev).
    #[serde(default = "default_admin_password")]
    pub admin_password: String,
    /// Repos to exclude from SonarQube scanning (folder names).
    #[serde(default)]
    pub exclude_repos: Vec<String>,
    /// Treat existing `<folder>-<repo>` projects (made by ax versions that
    /// kept no record) as ax-created, so they are renamed to the current key.
    /// Off by default; nothing is ever deleted either way.
    #[serde(default)]
    pub migrate_legacy_keys: bool,
}

fn default_admin_user() -> String {
    "admin".into()
}

fn default_admin_password() -> String {
    "admin".into()
}

fn default_scan_mode() -> String {
    "incremental".into()
}

fn default_container_runtime() -> String {
    "auto".into()
}

/// Which repos and files one platform scan covers.
struct ScanScope<'a> {
    repo_names: &'a [String],
    dirty_files: &'a [String],
    full_repo: bool,
    workspace_repo_count: usize,
}

impl SonarConfig {
    /// The SonarQube container: `podman_container`, or `sonarqube` when it is
    /// unset or empty.
    pub fn sonar_container_name(&self) -> &str {
        self.podman_container
            .as_deref()
            .filter(|name| !name.is_empty())
            .unwrap_or("sonarqube")
    }
}

impl Default for SonarConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            host: "http://localhost:9000".into(),
            project_key: "ax-project".into(),
            token_env: "SONAR_TOKEN".into(),
            scanner_path: "sonar-scanner".into(),
            podman_container: Some("sonarqube".into()),
            container_runtime: default_container_runtime(),
            scan_mode: default_scan_mode(),
            admin_user: default_admin_user(),
            admin_password: default_admin_password(),
            exclude_repos: Vec::new(),
            migrate_legacy_keys: false,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SonarCondition {
    pub metric_key: String,
    pub status: String,
    pub actual_value: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityGateResult {
    pub status: String,
    pub passed: bool,
    pub conditions: Vec<SonarCondition>,
}

#[derive(Deserialize, Debug)]
struct SonarStatusResponse {
    #[serde(rename = "projectStatus")]
    project_status: ProjectStatus,
}

#[derive(Deserialize, Debug)]
struct ProjectStatus {
    status: String,
    conditions: Vec<ConditionRaw>,
}

#[derive(Deserialize, Debug)]
struct ConditionRaw {
    #[serde(rename = "metricKey")]
    metric_key: String,
    status: String,
    #[serde(rename = "actualValue")]
    actual_value: String,
}

pub struct SonarClient {
    pub config: SonarConfig,
}

/// Per-repository Sonar scan progress (evaluate pipeline + manual scans).
#[derive(Debug, Clone)]
pub enum SonarScanProgressEvent {
    Started {
        index: usize,
        total: usize,
        project_key: String,
        repo_name: String,
    },
    Finished {
        project_key: String,
        repo_name: String,
        ok: bool,
        error: Option<String>,
    },
    Skipped {
        project_key: String,
        repo_name: String,
        reason: String,
    },
}

#[derive(Debug, Clone)]
struct ScanTarget {
    project_key: String,
    sources: String,
    inclusions: String,
}

impl SonarClient {
    pub fn new(config: SonarConfig) -> Self {
        Self { config }
    }

    pub async fn ensure_running(&self) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }
        let host = self.config.host.trim_end_matches('/');
        if sonar_reachable(host).await {
            return Ok(());
        }

        let container_name = self.config.sonar_container_name().to_string();
        let pref = if self.config.container_runtime == "auto" {
            None
        } else {
            Some(self.config.container_runtime.clone())
        };
        let offline =
            format!("SonarQube offline at {host} — install and start from the SonarQube page");
        let container = crate::container::blocking(move || {
            let Some(container) = find_container_any(&container_name) else {
                return Err(offline);
            };
            let runtime = resolve_runtime(pref.as_deref())?;
            let log = InstallLog::new();
            // Always try to start when a container exists (idempotent if already running).
            // `start_container` retries up to 3 times.
            info!(
                "SonarQube offline — starting existing container '{}' ({}) via {}",
                container.name,
                container.status,
                runtime.cli()
            );
            start_sonar_stack(runtime, &container_name, &log)?;
            Ok(container)
        })
        .await?;
        crate::container::wait_for_sonar(host, 120)
            .await
            .map_err(|e| {
                format!(
                    "SonarQube container '{}' started but API at {host} is not responding: {e}",
                    container.name
                )
            })
    }

    pub fn run_platform_scan(
        &self,
        project_root: &Path,
        repo_names: &[String],
        dirty_files: &[String],
        full_repo: bool,
    ) -> Result<(), String> {
        self.run_platform_scan_inner(
            project_root,
            repo_names,
            dirty_files,
            full_repo,
            None,
            &mut None,
        )
    }

    pub fn run_platform_scan_with_progress(
        &self,
        project_root: &Path,
        repo_names: &[String],
        dirty_files: &[String],
        full_repo: bool,
        progress: &mut Option<&mut dyn FnMut(SonarScanProgressEvent)>,
    ) -> Result<(), String> {
        self.run_platform_scan_inner(
            project_root,
            repo_names,
            dirty_files,
            full_repo,
            None,
            progress,
        )
    }

    pub fn run_full_scan_with_log(
        &self,
        project_root: &Path,
        repo_names: &[String],
        log: &crate::container::InstallLog,
    ) -> Result<(), String> {
        self.run_platform_scan_inner(project_root, repo_names, &[], true, Some(log), &mut None)
    }

    /// Full scan with explicit workspace repo count (for single-repo scans from multi-repo workspaces).
    pub fn run_full_scan_with_log_multi(
        &self,
        project_root: &Path,
        repo_names: &[String],
        log: &crate::container::InstallLog,
        workspace_repo_count: usize,
    ) -> Result<(), String> {
        let scope = ScanScope {
            repo_names,
            dirty_files: &[],
            full_repo: true,
            workspace_repo_count,
        };
        self.run_platform_scan_inner_counted(project_root, scope, Some(log), &mut None)
    }

    fn run_platform_scan_inner(
        &self,
        project_root: &Path,
        repo_names: &[String],
        dirty_files: &[String],
        full_repo: bool,
        log: Option<&crate::container::InstallLog>,
        progress: &mut Option<&mut dyn FnMut(SonarScanProgressEvent)>,
    ) -> Result<(), String> {
        let scope = ScanScope {
            repo_names,
            dirty_files,
            full_repo,
            workspace_repo_count: repo_names.len(),
        };
        self.run_platform_scan_inner_counted(project_root, scope, log, progress)
    }

    fn run_platform_scan_inner_counted(
        &self,
        project_root: &Path,
        scope: ScanScope,
        log: Option<&crate::container::InstallLog>,
        progress: &mut Option<&mut dyn FnMut(SonarScanProgressEvent)>,
    ) -> Result<(), String> {
        let ScanScope {
            repo_names,
            dirty_files,
            full_repo,
            workspace_repo_count,
        } = scope;
        if !self.config.enabled {
            return Ok(());
        }
        let scannable = filter_scannable_paths(project_root, dirty_files);

        if scannable.is_empty() && repo_names.is_empty() && !full_repo {
            return Ok(());
        }

        let token = read_sonar_token(project_root, &self.config.token_env)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                format!(
                    "Sonar token missing. Open Settings → Setup project & token, or set {} and save token to .ax/sonar.token",
                    self.config.token_env
                )
            })?;

        let workspace_key = workspace_sonar_key(&self.config.project_key, project_root);
        let filtered_dirty = filter_dirty_paths(dirty_files);
        let files_for_targets: &[String] = if repo_names.is_empty() {
            &scannable
        } else {
            &filtered_dirty
        };
        let targets = build_scan_targets_with_count(
            &workspace_key,
            repo_names,
            files_for_targets,
            full_repo,
            workspace_repo_count,
        );
        emit_skipped_repos(
            &workspace_key,
            repo_names,
            workspace_repo_count,
            &targets,
            full_repo,
            progress,
            log,
        );
        if targets.is_empty() {
            return Ok(());
        }

        let mut failures = Vec::new();
        let total = targets.len();
        for (index, target) in targets.iter().enumerate() {
            let repo_name = if target.sources == "." {
                target.project_key.clone()
            } else {
                target.sources.clone()
            };
            if let Some(cb) = progress.as_mut() {
                (*cb)(SonarScanProgressEvent::Started {
                    index: index + 1,
                    total,
                    project_key: target.project_key.clone(),
                    repo_name: repo_name.clone(),
                });
            }
            if let Some(log) = log {
                log.push(format!(
                    "[{}/{}] Scanning {} ({})…",
                    index + 1,
                    total,
                    repo_name,
                    target.project_key,
                ));
            }
            info!(
                project = %target.project_key,
                sources = %target.sources,
                inclusions = %target.inclusions,
                "Sonar scan"
            );
            if let Err(e) = self.run_scan(project_root, target, Some(&token)) {
                tracing::warn!(project = %target.project_key, error = %e, "Sonar scan failed");
                if let Some(log) = log {
                    log.push(format!("✕ {} — {e}", target.project_key));
                }
                if let Some(cb) = progress.as_mut() {
                    (*cb)(SonarScanProgressEvent::Finished {
                        project_key: target.project_key.clone(),
                        repo_name: repo_name.clone(),
                        ok: false,
                        error: Some(e.clone()),
                    });
                }
                failures.push(format!("{}: {e}", target.project_key));
            } else {
                if let Some(log) = log {
                    log.push(format!("✓ {} complete", target.project_key));
                }
                if let Some(cb) = progress.as_mut() {
                    (*cb)(SonarScanProgressEvent::Finished {
                        project_key: target.project_key.clone(),
                        repo_name,
                        ok: true,
                        error: None,
                    });
                }
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{} of {} Sonar scan(s) failed: {}",
                failures.len(),
                targets.len(),
                failures.join("; ")
            ))
        }
    }

    /// Backward-compatible alias — scans every git repo in the workspace.
    pub fn run_incremental_scan(
        &self,
        project_root: &Path,
        repo_names: &[String],
        dirty_files: &[String],
        _baseline_repos: &[String],
    ) -> Result<(), String> {
        self.run_platform_scan(project_root, repo_names, dirty_files, false)
    }

    pub fn run_full_scan(&self, project_root: &Path, repo_names: &[String]) -> Result<(), String> {
        self.run_platform_scan_inner(project_root, repo_names, &[], true, None, &mut None)
    }

    fn run_scan(
        &self,
        project_root: &Path,
        target: &ScanTarget,
        token: Option<&str>,
    ) -> Result<(), String> {
        if scanner_available(&self.config.scanner_path) {
            self.run_native_scan(project_root, target, token)
        } else {
            self.run_container_scan(project_root, target, token)
        }
    }

    /// Runs the scanner with the token in the child environment only
    /// (`SONAR_TOKEN`), never on the command line or in a file: process
    /// arguments are readable by every local user. SonarScanner CLI versions
    /// before 5.0 do not read `SONAR_TOKEN`, so with a token such a scanner is
    /// refused instead of scanning unauthenticated.
    fn run_native_scan(
        &self,
        project_root: &Path,
        target: &ScanTarget,
        token: Option<&str>,
    ) -> Result<(), String> {
        if token.is_some() {
            let mut version = Command::new(&self.config.scanner_path);
            version.arg("--version").current_dir(project_root);
            if let Ok(out) = crate::process::run_with_timeout(&mut version, VERSION_TIMEOUT) {
                check_scanner_supports_env_token(&format!("{}\n{}", out.stdout, out.stderr))
                    .map_err(|e| format!("{e} ({})", self.config.scanner_path))?;
            }
        }
        let mut cmd = self.native_scan_command(project_root, target, token);
        run_command_with_timeout(&mut cmd, SCAN_TIMEOUT)
    }

    fn native_scan_command(
        &self,
        project_root: &Path,
        target: &ScanTarget,
        token: Option<&str>,
    ) -> Command {
        let mut cmd = Command::new(&self.config.scanner_path);
        cmd.current_dir(project_root);
        cmd.args(scan_property_args(target, &self.config.host));
        if let Some(t) = token {
            cmd.env("SONAR_TOKEN", t);
        }
        cmd
    }

    fn run_container_scan(
        &self,
        project_root: &Path,
        target: &ScanTarget,
        token: Option<&str>,
    ) -> Result<(), String> {
        let (runtime, network_args, host) = crate::container::scanner_target(&self.config)?;
        let mount = project_root
            .canonicalize()
            .map_err(|e| format!("project path: {e}"))?;

        info!(
            "Running SonarScanner via {} ({}) — native scanner not found on PATH",
            runtime.cli(),
            SONAR_SCANNER_IMAGE
        );

        let mut cmd =
            container_scan_command(runtime.cli(), network_args, &mount, &host, target, token);
        run_command_with_timeout(&mut cmd, SCAN_TIMEOUT)
            .map_err(|e| format!("container sonar-scanner ({} run): {e}", runtime.cli()))
    }

    pub async fn fetch_quality_gate(
        &self,
        project_root: &Path,
        repo_names: &[String],
    ) -> Result<QualityGateResult, String> {
        self.fetch_quality_gate_inner(project_root, repo_names, true)
            .await
    }

    /// Gate for a scan made with [`Self::run_full_scan_with_log_multi`]: the
    /// project keys follow the same `workspace_repo_count` the scan used.
    pub async fn fetch_quality_gate_multi(
        &self,
        project_root: &Path,
        repo_names: &[String],
        workspace_repo_count: usize,
    ) -> Result<QualityGateResult, String> {
        self.fetch_quality_gate_counted(project_root, repo_names, workspace_repo_count, true)
            .await
    }

    /// Fetch without waiting for the Compute Engine (for status checks / dashboards).
    pub async fn fetch_quality_gate_snapshot(
        &self,
        project_root: &Path,
        repo_names: &[String],
    ) -> Result<QualityGateResult, String> {
        self.fetch_quality_gate_inner(project_root, repo_names, false)
            .await
    }

    async fn fetch_quality_gate_inner(
        &self,
        project_root: &Path,
        repo_names: &[String],
        wait_for_ce: bool,
    ) -> Result<QualityGateResult, String> {
        self.fetch_quality_gate_counted(project_root, repo_names, repo_names.len(), wait_for_ce)
            .await
    }

    async fn fetch_quality_gate_counted(
        &self,
        project_root: &Path,
        repo_names: &[String],
        workspace_repo_count: usize,
        wait_for_ce: bool,
    ) -> Result<QualityGateResult, String> {
        if !self.config.enabled {
            return Ok(QualityGateResult {
                status: "SKIPPED".into(),
                passed: true,
                conditions: vec![],
            });
        }

        let workspace_key = workspace_sonar_key(&self.config.project_key, project_root);
        let keys = scan_project_keys(&workspace_key, repo_names, workspace_repo_count);

        let mut combined = QualityGateResult {
            status: "OK".into(),
            passed: true,
            conditions: vec![],
        };

        for key in keys {
            let result = self
                .fetch_quality_gate_for_key(project_root, &key, wait_for_ce)
                .await?;
            if !result.passed {
                combined.passed = false;
                combined.status = result.status.clone();
            } else if combined.passed && combined.status == "OK" {
                // A passing WARN (or a neutral NONE) is still reported, so a
                // warning is not shown as a clean OK.
                combined.status = result.status.clone();
            }
            combined.conditions.extend(result.conditions);
        }

        Ok(combined)
    }

    async fn fetch_quality_gate_for_key(
        &self,
        project_root: &Path,
        project_key: &str,
        wait_for_ce: bool,
    ) -> Result<QualityGateResult, String> {
        let host = self.config.host.trim_end_matches('/');
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let token = read_sonar_token(project_root, &self.config.token_env);
        let auth = token.as_deref().map(token_basic_auth);

        // Right after a scan, project_status still describes the previous
        // analysis until the Compute Engine has processed the new report.
        // Wait for the project's CE queue to drain and read the gate of the
        // analysis it produced.
        let mut query = format!("projectKey={project_key}");
        let mut retry_none = false;
        if wait_for_ce {
            match wait_for_ce_queue(
                &client,
                host,
                auth.as_deref(),
                project_key,
                CE_POLL_INTERVAL,
                CE_WAIT_LIMIT,
            )
            .await
            {
                CeWait::Done {
                    analysis_id: Some(id),
                } => query = format!("analysisId={id}"),
                CeWait::Done { analysis_id: None } => {}
                CeWait::Failed(reason) => {
                    warn!(project = %project_key, "SonarQube analysis failed: {reason}");
                    return Ok(QualityGateResult {
                        status: "ANALYSIS_FAILED".into(),
                        passed: false,
                        conditions: vec![],
                    });
                }
                CeWait::TimedOut => {
                    warn!(project = %project_key, "SonarQube analysis still queued at the deadline");
                    return Ok(QualityGateResult {
                        status: "PENDING".into(),
                        passed: false,
                        conditions: vec![],
                    });
                }
                CeWait::Unavailable(reason) => {
                    warn!(
                        project = %project_key,
                        "SonarQube Compute Engine status unavailable ({reason}); polling the gate"
                    );
                    retry_none = true;
                }
            }
        }
        let url = format!("{host}/api/qualitygates/project_status?{query}");

        // Without the CE status, NONE is the only sign that the analysis is
        // still being processed: poll a while for a real result.
        const MAX_RETRIES: u32 = 10;
        let mut attempt = 0;
        loop {
            let mut req = client.get(&url);
            if let Some(ref a) = auth {
                req = req.header("Authorization", a.clone());
            }
            let resp = req.send().await.map_err(|e| e.to_string())?;
            if !resp.status().is_success() {
                warn!("SonarQube quality gate API returned {}", resp.status());
                return Ok(QualityGateResult {
                    status: "UNKNOWN".into(),
                    passed: false,
                    conditions: vec![],
                });
            }
            let body: SonarStatusResponse = resp.json().await.map_err(|e| e.to_string())?;
            let status = body.project_status.status.clone();

            if status == "NONE" && retry_none && attempt < MAX_RETRIES {
                attempt += 1;
                info!(
                    project = %project_key,
                    attempt,
                    "Quality gate NONE — retrying in {}s",
                    CE_POLL_INTERVAL.as_secs_f32()
                );
                tokio::time::sleep(CE_POLL_INTERVAL).await;
                continue;
            }

            // NONE means no gate was computed. A caller that waited for the
            // analysis gets a failed gate; a snapshot keeps it neutral.
            let passed = match status.as_str() {
                "OK" | "WARN" => true,
                "NONE" => !wait_for_ce,
                _ => false,
            };
            return Ok(QualityGateResult {
                status,
                passed,
                conditions: body
                    .project_status
                    .conditions
                    .into_iter()
                    .map(|c| SonarCondition {
                        metric_key: c.metric_key,
                        status: c.status,
                        actual_value: c.actual_value,
                    })
                    .collect(),
            });
        }
    }
}

/// How often the Compute Engine queue and a NONE gate are polled.
const CE_POLL_INTERVAL: Duration = if cfg!(test) {
    Duration::from_millis(20)
} else {
    Duration::from_secs(3)
};
/// How long to wait for the Compute Engine to process a submitted report.
const CE_WAIT_LIMIT: Duration = if cfg!(test) {
    Duration::from_secs(2)
} else {
    Duration::from_secs(600)
};

#[derive(Deserialize, Debug, Default)]
struct CeComponentResponse {
    #[serde(default)]
    queue: Vec<serde_json::Value>,
    #[serde(default)]
    current: Option<CeTask>,
}

#[derive(Deserialize, Debug)]
struct CeTask {
    status: String,
    #[serde(rename = "analysisId", default)]
    analysis_id: Option<String>,
    #[serde(rename = "errorMessage", default)]
    error_message: Option<String>,
}

#[derive(Debug, PartialEq)]
enum CeWait {
    /// The queue is empty; the last task succeeded (with its analysis, when known).
    Done { analysis_id: Option<String> },
    /// The last task did not succeed.
    Failed(String),
    /// Tasks were still queued at the deadline.
    TimedOut,
    /// The Compute Engine status could not be read.
    Unavailable(String),
}

async fn wait_for_ce_queue(
    client: &reqwest::Client,
    host: &str,
    auth: Option<&str>,
    project_key: &str,
    interval: Duration,
    limit: Duration,
) -> CeWait {
    let url = format!("{host}/api/ce/component?component={project_key}");
    let deadline = tokio::time::Instant::now() + limit;
    loop {
        let mut req = client.get(&url);
        if let Some(a) = auth {
            req = req.header("Authorization", a);
        }
        let resp = match req.send().await {
            Ok(r) => r,
            Err(e) => return CeWait::Unavailable(e.to_string()),
        };
        if !resp.status().is_success() {
            return CeWait::Unavailable(format!("HTTP {}", resp.status()));
        }
        let body: CeComponentResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => return CeWait::Unavailable(e.to_string()),
        };
        if body.queue.is_empty() {
            return match body.current {
                None => CeWait::Done { analysis_id: None },
                Some(task) if task.status == "SUCCESS" => CeWait::Done {
                    analysis_id: task.analysis_id.filter(|id| !id.is_empty()),
                },
                Some(task) => CeWait::Failed(
                    task.error_message
                        .unwrap_or_else(|| format!("task status {}", task.status)),
                ),
            };
        }
        if tokio::time::Instant::now() + interval > deadline {
            return CeWait::TimedOut;
        }
        tokio::time::sleep(interval).await;
    }
}

/// Deadline for `sonar-scanner --version`.
const VERSION_TIMEOUT: Duration = Duration::from_secs(60);

/// Oldest SonarScanner CLI major version that reads the `SONAR_TOKEN`
/// environment variable.
const MIN_SCANNER_MAJOR: u32 = 5;

/// Major version from a `sonar-scanner --version` banner such as
/// `INFO: SonarScanner 4.8.0.2856` or `SonarScanner CLI 6.2.1.4610`. The
/// older banners `SonarQube Scanner 3.3.0.1492` and `SonarQube Runner 2.4`
/// are read too; a `SonarQube Runner` banner without a number counts as 2.
fn scanner_major_version(output: &str) -> Option<u32> {
    fn leading_number(rest: &str) -> Option<u32> {
        let digits: String = rest
            .trim_start()
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().ok()
    }
    if let Some(at) = output.find("SonarScanner") {
        let rest = output[at + "SonarScanner".len()..].trim_start();
        return leading_number(rest.strip_prefix("CLI").unwrap_or(rest));
    }
    if let Some(at) = output.find("SonarQube Scanner") {
        return leading_number(&output[at + "SonarQube Scanner".len()..]);
    }
    if let Some(at) = output.find("SonarQube Runner") {
        return Some(leading_number(&output[at + "SonarQube Runner".len()..]).unwrap_or(2));
    }
    None
}

/// Refuses a scanner older than 5.0 (including the older SonarQube Scanner
/// and SonarQube Runner), which ignores `SONAR_TOKEN`. A banner without a
/// version is not refused.
fn check_scanner_supports_env_token(version_output: &str) -> Result<(), String> {
    match scanner_major_version(version_output) {
        Some(major) if major < MIN_SCANNER_MAJOR => Err(format!(
            "sonar-scanner {major}.x does not read the SONAR_TOKEN environment variable; \
             install SonarScanner CLI {MIN_SCANNER_MAJOR}.0 or newer, or remove it from PATH so ax uses the container scanner"
        )),
        _ => Ok(()),
    }
}

fn run_command_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<(), String> {
    let out = crate::process::run_with_timeout(cmd, timeout).map_err(|e| match e {
        crate::process::RunError::Spawn(e) => format!("failed to start scanner: {e}"),
        other => other.to_string(),
    })?;
    if out.success() {
        return Ok(());
    }
    Err(out
        .stderr_tail_line()
        .unwrap_or("exited with error")
        .to_string())
}

fn token_basic_auth(token: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    format!("Basic {}", STANDARD.encode(format!("{token}:")))
}

/// Windows CreateProcess limits (~8k); keep a margin for podman/docker args.
const CMDLINE_INCLUSIONS_LIMIT: usize = 4_000;
/// Above this file count, collapse to `{repo}/**` globs instead of listing every path.
const REPO_GLOB_THRESHOLD: usize = 200;

/// Join a repo-relative path on Windows and Unix (`repo/src/Foo.cs` → nested segments).
fn resolve_rel_path(project_root: &Path, rel: &str) -> PathBuf {
    rel.split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .fold(project_root.to_path_buf(), |acc, part| acc.join(part))
}

fn filter_dirty_paths(dirty_files: &[String]) -> Vec<String> {
    dirty_files
        .iter()
        .filter(|f| !f.contains('\t') && !f.contains("web-ui/dist/"))
        .cloned()
        .collect()
}

fn filter_scannable_paths(project_root: &Path, dirty_files: &[String]) -> Vec<String> {
    filter_dirty_paths(dirty_files)
        .into_iter()
        .filter(|f| resolve_rel_path(project_root, f).is_file())
        .collect()
}

/// Build `sonar.inclusions` — explicit paths for small diffs, scan entire source tree for large ones.
pub(crate) fn build_inclusions(files: &[String]) -> String {
    let joined = files.join(",");
    if files.len() <= REPO_GLOB_THRESHOLD && joined.len() <= CMDLINE_INCLUSIONS_LIMIT {
        return joined;
    }
    "**/*".into()
}

fn emit_skipped_repos(
    workspace_key: &str,
    repo_names: &[String],
    workspace_repo_count: usize,
    targets: &[ScanTarget],
    full_repo: bool,
    progress: &mut Option<&mut dyn FnMut(SonarScanProgressEvent)>,
    log: Option<&crate::container::InstallLog>,
) {
    if repo_names.is_empty() {
        return;
    }
    let target_sources: std::collections::HashSet<&str> =
        targets.iter().map(|t| t.sources.as_str()).collect();
    for repo in repo_names {
        if target_sources.contains(repo.as_str()) {
            continue;
        }
        let project_key = repo_project_key(workspace_key, repo, workspace_repo_count);
        let reason: String = if full_repo {
            "not selected".into()
        } else {
            "no changed files".into()
        };
        if let Some(cb) = progress.as_mut() {
            (*cb)(SonarScanProgressEvent::Skipped {
                project_key: project_key.clone(),
                repo_name: repo.clone(),
                reason: reason.clone(),
            });
        }
        if let Some(log) = log {
            log.push(format!("– {repo} skipped ({reason})"));
        }
    }
}

/// Sonar project key a scan of `repo` writes to. A repo of a multi-repo
/// workspace gets its own prefixed key even when it is scanned alone.
fn repo_project_key(workspace_key: &str, repo: &str, workspace_repo_count: usize) -> String {
    canonical_repo_project_key(workspace_key, repo, workspace_repo_count > 1)
}

/// Every project key a scan of `repo_names` writes to (the workspace key when
/// no repos are listed).
fn scan_project_keys(
    workspace_key: &str,
    repo_names: &[String],
    workspace_repo_count: usize,
) -> Vec<String> {
    if repo_names.is_empty() {
        return vec![workspace_key.to_string()];
    }
    let mut keys: Vec<String> = Vec::new();
    for repo in repo_names {
        let key = repo_project_key(workspace_key, repo, workspace_repo_count);
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    keys
}

#[cfg(test)]
fn build_scan_targets(
    workspace_key: &str,
    repo_names: &[String],
    dirty_files: &[String],
    full_repo: bool,
) -> Vec<ScanTarget> {
    build_scan_targets_with_count(
        workspace_key,
        repo_names,
        dirty_files,
        full_repo,
        repo_names.len(),
    )
}

fn build_scan_targets_with_count(
    workspace_key: &str,
    repo_names: &[String],
    dirty_files: &[String],
    full_repo: bool,
    workspace_repo_count: usize,
) -> Vec<ScanTarget> {
    if repo_names.is_empty() {
        let inclusions = if full_repo {
            "**/*".into()
        } else {
            build_inclusions(dirty_files)
        };
        if !full_repo && dirty_files.is_empty() {
            return Vec::new();
        }
        return vec![ScanTarget {
            project_key: workspace_key.to_string(),
            sources: ".".into(),
            inclusions,
        }];
    }

    let mut grouped: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in dirty_files {
        let Some(repo) = file.split('/').next().filter(|s| !s.is_empty()) else {
            continue;
        };
        if !repo_names.iter().any(|r| r == repo) {
            continue;
        }
        // Keep the `{repo}/` prefix: the scanner's base directory is the
        // workspace root, and inclusion patterns are relative to it.
        grouped
            .entry(repo.to_string())
            .or_default()
            .push(file.clone());
    }

    let mut targets: Vec<ScanTarget> = repo_names
        .iter()
        .filter_map(|repo| {
            let files = grouped.get(repo).cloned().unwrap_or_default();
            if !full_repo && files.is_empty() {
                return None;
            }
            let inclusions = if full_repo || files.len() > REPO_GLOB_THRESHOLD {
                "**/*".into()
            } else {
                build_inclusions(&files)
            };
            Some(ScanTarget {
                project_key: repo_project_key(workspace_key, repo, workspace_repo_count),
                sources: repo.clone(),
                inclusions,
            })
        })
        .collect();

    targets.sort_by(|a, b| {
        let a_changed = grouped.contains_key(&a.sources);
        let b_changed = grouped.contains_key(&b.sources);
        b_changed.cmp(&a_changed)
    });

    targets
}

/// The `-Dsonar.*` arguments that describe one scan (no credentials).
fn scan_property_args(target: &ScanTarget, host: &str) -> Vec<String> {
    vec![
        format!("-Dsonar.projectKey={}", target.project_key),
        format!("-Dsonar.sources={}", target.sources),
        format!("-Dsonar.inclusions={}", target.inclusions),
        format!("-Dsonar.exclusions={DEFAULT_EXCLUSIONS}"),
        format!("-Dsonar.host.url={host}"),
    ]
}

/// `<cli> run` for the scanner image. The token travels as `-e SONAR_TOKEN`
/// (name only), with its value in the container CLI's own environment.
fn container_scan_command(
    cli: &str,
    network_args: Vec<String>,
    mount: &Path,
    host: &str,
    target: &ScanTarget,
    token: Option<&str>,
) -> Command {
    let mut args = vec!["run".to_string(), "--rm".to_string()];
    args.extend(network_args);
    if token.is_some() {
        args.push("-e".to_string());
        args.push("SONAR_TOKEN".to_string());
    }
    args.extend([
        "-v".to_string(),
        format!("{}:/usr/src", mount.display()),
        "-w".to_string(),
        "/usr/src".to_string(),
        SONAR_SCANNER_IMAGE.to_string(),
    ]);
    args.extend(scan_property_args(target, host));
    let mut cmd = Command::new(cli);
    cmd.args(&args);
    if let Some(t) = token {
        cmd.env("SONAR_TOKEN", t);
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    type Handler = dyn Fn(&str, usize) -> (u16, String) + Send + Sync;

    /// A minimal SonarQube stand-in. `handler` gets the request path with its
    /// query and how many times that path was requested before.
    struct MockSonar {
        host: String,
        hits: Arc<Mutex<Vec<String>>>,
    }

    impl MockSonar {
        async fn start(
            handler: impl Fn(&str, usize) -> (u16, String) + Send + Sync + 'static,
        ) -> Self {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let host = format!("http://{}", listener.local_addr().unwrap());
            let hits: Arc<Mutex<Vec<String>>> = Arc::default();
            let handler: Arc<Handler> = Arc::new(handler);
            let seen = hits.clone();
            tokio::spawn(async move {
                loop {
                    let Ok((mut sock, _)) = listener.accept().await else {
                        return;
                    };
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 1024];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match sock.read(&mut chunk).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let text = String::from_utf8_lossy(&buf);
                    let path = text.split_whitespace().nth(1).unwrap_or("").to_string();
                    let before = {
                        let mut seen = seen.lock().unwrap();
                        let n = seen.iter().filter(|p| **p == path).count();
                        seen.push(path.clone());
                        n
                    };
                    let (code, body) = handler(&path, before);
                    let resp = format!(
                        "HTTP/1.1 {code} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = sock.write_all(resp.as_bytes()).await;
                    let _ = sock.shutdown().await;
                }
            });
            Self { host, hits }
        }

        fn hits(&self) -> Vec<String> {
            self.hits.lock().unwrap().clone()
        }
    }

    fn client_for(host: &str) -> SonarClient {
        SonarClient::new(SonarConfig {
            enabled: true,
            host: host.to_string(),
            project_key: "ax".into(),
            token_env: "AX_TEST_SONAR_TOKEN_UNSET".into(),
            ..SonarConfig::default()
        })
    }

    fn gate_body(status: &str) -> String {
        format!("{{\"projectStatus\":{{\"status\":\"{status}\",\"conditions\":[]}}}}")
    }

    #[test]
    fn skipped_repo_of_a_multi_repo_workspace_reports_its_prefixed_key() {
        let repos = vec!["AcmeAdvisor".to_string()];
        let mut keys = Vec::new();
        let mut record = |e: SonarScanProgressEvent| {
            if let SonarScanProgressEvent::Skipped { project_key, .. } = e {
                keys.push(project_key);
            }
        };
        let mut progress: Option<&mut dyn FnMut(SonarScanProgressEvent)> = Some(&mut record);
        emit_skipped_repos("ax", &repos, 10, &[], false, &mut progress, None);
        assert_eq!(keys, vec!["ax-AcmeAdvisor".to_string()]);
    }

    #[test]
    fn scan_and_gate_keys_agree() {
        let one = vec!["AcmeAdvisor".to_string()];
        assert_eq!(scan_project_keys("ax", &one, 10), vec!["ax-AcmeAdvisor"]);
        assert_eq!(scan_project_keys("ax", &one, 1), vec!["ax"]);
        assert_eq!(scan_project_keys("ax", &[], 0), vec!["ax"]);
        let two = vec!["a".to_string(), "b".to_string()];
        assert_eq!(scan_project_keys("ax", &two, 2), vec!["ax-a", "ax-b"]);
    }

    #[tokio::test]
    async fn gate_waits_for_the_submitted_analysis_and_reads_its_status() {
        // The previous analysis passed; the report just uploaded is still
        // queued in the Compute Engine and will fail the gate.
        let sonar = MockSonar::start(|path, before| {
            if path.starts_with("/api/ce/component") {
                if before < 2 {
                    (200, "{\"queue\":[{\"id\":\"T2\",\"status\":\"PENDING\"}],\"current\":{\"id\":\"T1\",\"status\":\"SUCCESS\",\"analysisId\":\"A1\"}}".into())
                } else {
                    (200, "{\"queue\":[],\"current\":{\"id\":\"T2\",\"status\":\"SUCCESS\",\"analysisId\":\"A2\"}}".into())
                }
            } else if path.contains("analysisId=A2") {
                (200, gate_body("ERROR"))
            } else {
                (200, gate_body("OK"))
            }
        })
        .await;
        let root = tempfile::tempdir().unwrap();
        let gate = client_for(&sonar.host)
            .fetch_quality_gate(root.path(), &[])
            .await
            .unwrap();
        assert_eq!(gate.status, "ERROR", "{:?}", sonar.hits());
        assert!(!gate.passed);
    }

    #[tokio::test]
    async fn gate_does_not_pass_when_no_status_was_computed() {
        let sonar = MockSonar::start(|path, _| {
            if path.starts_with("/api/ce/component") {
                (200, "{\"queue\":[]}".into())
            } else {
                (200, gate_body("NONE"))
            }
        })
        .await;
        let root = tempfile::tempdir().unwrap();
        let gate = client_for(&sonar.host)
            .fetch_quality_gate(root.path(), &[])
            .await
            .unwrap();
        assert_eq!(gate.status, "NONE");
        assert!(!gate.passed);
    }

    #[tokio::test]
    async fn gate_is_pending_when_the_analysis_stays_queued() {
        let sonar = MockSonar::start(|path, _| {
            if path.starts_with("/api/ce/component") {
                (
                    200,
                    "{\"queue\":[{\"id\":\"T2\",\"status\":\"IN_PROGRESS\"}]}".into(),
                )
            } else {
                (200, gate_body("OK"))
            }
        })
        .await;
        let root = tempfile::tempdir().unwrap();
        let gate = client_for(&sonar.host)
            .fetch_quality_gate(root.path(), &[])
            .await
            .unwrap();
        assert_eq!(gate.status, "PENDING");
        assert!(!gate.passed);
        assert!(!sonar
            .hits()
            .iter()
            .any(|p| p.starts_with("/api/qualitygates/")));
    }

    #[tokio::test]
    async fn without_compute_engine_status_the_gate_polls_past_none() {
        let sonar = MockSonar::start(|path, before| {
            if path.starts_with("/api/ce/component") {
                (403, "{}".into())
            } else if before < 2 {
                (200, gate_body("NONE"))
            } else {
                (200, gate_body("ERROR"))
            }
        })
        .await;
        let root = tempfile::tempdir().unwrap();
        let gate = client_for(&sonar.host)
            .fetch_quality_gate(root.path(), &[])
            .await
            .unwrap();
        assert_eq!(gate.status, "ERROR");
        assert!(!gate.passed);
    }

    #[tokio::test]
    async fn snapshot_does_not_wait_and_keeps_none_neutral() {
        let sonar = MockSonar::start(|_, _| (200, gate_body("NONE"))).await;
        let root = tempfile::tempdir().unwrap();
        let gate = client_for(&sonar.host)
            .fetch_quality_gate_snapshot(root.path(), &[])
            .await
            .unwrap();
        assert!(gate.passed);
        assert_eq!(
            sonar.hits(),
            vec!["/api/qualitygates/project_status?projectKey=ax".to_string()]
        );
    }

    #[tokio::test]
    async fn gate_fails_when_the_analysis_task_failed() {
        let sonar = MockSonar::start(|path, _| {
            if path.starts_with("/api/ce/component") {
                (200, "{\"queue\":[],\"current\":{\"id\":\"T2\",\"status\":\"FAILED\",\"errorMessage\":\"boom\"}}".into())
            } else {
                (200, gate_body("OK"))
            }
        })
        .await;
        let root = tempfile::tempdir().unwrap();
        let gate = client_for(&sonar.host)
            .fetch_quality_gate(root.path(), &[])
            .await
            .unwrap();
        assert!(!gate.passed, "{gate:?}");
    }

    #[tokio::test]
    async fn gate_for_one_repo_of_a_multi_repo_workspace_reads_the_scanned_key() {
        let sonar = MockSonar::start(|path, _| {
            if path.starts_with("/api/qualitygates/project_status") {
                (200, gate_body("OK"))
            } else {
                (200, "{\"queue\":[]}".into())
            }
        })
        .await;
        let root = tempfile::tempdir().unwrap();
        let repos = vec!["AcmeAdvisor".to_string()];
        // The scan for this selection writes to `ax-AcmeAdvisor`.
        let targets = build_scan_targets_with_count("ax", &repos, &[], true, 10);
        assert_eq!(targets[0].project_key, "ax-AcmeAdvisor");

        client_for(&sonar.host)
            .fetch_quality_gate_multi(root.path(), &repos, 10)
            .await
            .unwrap();
        let gate_calls: Vec<String> = sonar
            .hits()
            .into_iter()
            .filter(|p| p.starts_with("/api/qualitygates/project_status"))
            .collect();
        assert_eq!(gate_calls.len(), 1, "{gate_calls:?}");
        assert!(
            gate_calls[0].contains("projectKey=ax-AcmeAdvisor"),
            "{gate_calls:?}"
        );
    }

    #[tokio::test]
    async fn a_warn_gate_passes_and_keeps_its_conditions() {
        let sonar = MockSonar::start(|path, _| {
            if path.starts_with("/api/ce/component") {
                (200, "{\"queue\":[]}".into())
            } else {
                (
                    200,
                    "{\"projectStatus\":{\"status\":\"WARN\",\"conditions\":[{\"metricKey\":\"coverage\",\"status\":\"WARN\",\"actualValue\":\"71.0\"}]}}".into(),
                )
            }
        })
        .await;
        let root = tempfile::tempdir().unwrap();
        let gate = client_for(&sonar.host)
            .fetch_quality_gate(root.path(), &[])
            .await
            .unwrap();
        assert_eq!(gate.status, "WARN");
        assert!(gate.passed);
        assert_eq!(gate.conditions.len(), 1);
        assert_eq!(gate.conditions[0].metric_key, "coverage");
        assert_eq!(gate.conditions[0].status, "WARN");
        assert_eq!(gate.conditions[0].actual_value, "71.0");
    }

    #[tokio::test]
    async fn a_gate_api_error_is_unknown_and_not_passed() {
        for snapshot in [false, true] {
            let sonar = MockSonar::start(|path, _| {
                if path.starts_with("/api/ce/component") {
                    (200, "{\"queue\":[]}".into())
                } else {
                    (500, "{}".into())
                }
            })
            .await;
            let root = tempfile::tempdir().unwrap();
            let client = client_for(&sonar.host);
            let gate = if snapshot {
                client.fetch_quality_gate_snapshot(root.path(), &[]).await
            } else {
                client.fetch_quality_gate(root.path(), &[]).await
            }
            .unwrap();
            assert_eq!(gate.status, "UNKNOWN", "snapshot={snapshot}");
            assert!(!gate.passed, "snapshot={snapshot}");
        }
    }

    #[tokio::test]
    async fn a_multi_repo_gate_reads_every_repo_key_and_fails_on_one_error() {
        let sonar = MockSonar::start(|path, _| {
            if path.starts_with("/api/ce/component") {
                (200, "{\"queue\":[]}".into())
            } else if path.contains("projectKey=ax-b") {
                (200, gate_body("ERROR"))
            } else {
                (200, gate_body("OK"))
            }
        })
        .await;
        let root = tempfile::tempdir().unwrap();
        let repos = vec!["a".to_string(), "b".to_string()];
        let gate = client_for(&sonar.host)
            .fetch_quality_gate(root.path(), &repos)
            .await
            .unwrap();
        assert_eq!(gate.status, "ERROR");
        assert!(!gate.passed);
        let gate_calls: Vec<String> = sonar
            .hits()
            .into_iter()
            .filter(|p| p.starts_with("/api/qualitygates/project_status"))
            .collect();
        assert_eq!(
            gate_calls,
            vec![
                "/api/qualitygates/project_status?projectKey=ax-a".to_string(),
                "/api/qualitygates/project_status?projectKey=ax-b".to_string(),
            ]
        );
    }

    #[test]
    fn a_disabled_client_skips_the_gate() {
        let client = SonarClient::new(SonarConfig {
            enabled: false,
            ..SonarConfig::default()
        });
        let root = tempfile::tempdir().unwrap();
        let gate = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(client.fetch_quality_gate(root.path(), &[]))
            .unwrap();
        assert_eq!(gate.status, "SKIPPED");
        assert!(gate.passed);
    }

    #[cfg(unix)]
    #[test]
    fn a_scanner_that_writes_a_lot_of_output_still_finishes() {
        // 1 MiB on stdout and on stderr is far more than an OS pipe buffer.
        let mut cmd = Command::new("sh");
        cmd.args([
            "-c",
            "head -c 1048576 /dev/zero; head -c 1048576 /dev/zero >&2; exit 0",
        ]);
        let started = Instant::now();
        let result = run_command_with_timeout(&mut cmd, Duration::from_secs(5));
        assert_eq!(result, Ok(()));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_scanner_reports_the_last_stderr_line() {
        let mut cmd = Command::new("sh");
        cmd.args([
            "-c",
            "head -c 1048576 /dev/zero >&2; echo >&2; echo 'ERROR: bad token' >&2; exit 3",
        ]);
        let result = run_command_with_timeout(&mut cmd, Duration::from_secs(5));
        assert_eq!(result, Err("ERROR: bad token".to_string()));
    }

    #[test]
    fn small_diff_lists_explicit_paths() {
        let files = vec!["src/Foo.cs".into(), "src/Bar.cs".into()];
        let inc = build_inclusions(&files);
        assert!(inc.contains("src/Foo.cs"));
        assert!(inc.contains("src/Bar.cs"));
        assert!(!inc.contains("/**"));
    }

    #[test]
    fn large_diff_collapses_to_repo_globs() {
        let files: Vec<String> = (0..300).map(|i| format!("src/File{i}.cs")).collect();
        let inc = build_inclusions(&files);
        assert_eq!(inc, "**/*");
    }

    #[test]
    fn resolve_rel_path_splits_forward_slashes() {
        let root = Path::new(if cfg!(windows) {
            r"C:\workspace"
        } else {
            "/workspace"
        });
        let resolved = resolve_rel_path(root, "Contoso-Web/src/Foo.cs");
        assert_eq!(
            resolved,
            root.join("Contoso-Web").join("src").join("Foo.cs")
        );
    }

    #[test]
    fn groups_dirty_files_per_repo() {
        let repos = vec![
            "Contoso-Web".into(),
            "ContosoPortal".into(),
            "FabrikamTeams".into(),
        ];
        let files = vec![
            "Contoso-Web/src/Foo.cs".into(),
            "ContosoPortal/src/Bar.cs".into(),
        ];
        let targets = build_scan_targets("Contoso", &repos, &files, false);
        let summary: Vec<(&str, &str, &str)> = targets
            .iter()
            .map(|t| {
                (
                    t.project_key.as_str(),
                    t.sources.as_str(),
                    t.inclusions.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                (
                    "Contoso-Contoso-Web",
                    "Contoso-Web",
                    "Contoso-Web/src/Foo.cs"
                ),
                (
                    "Contoso-ContosoPortal",
                    "ContosoPortal",
                    "ContosoPortal/src/Bar.cs"
                ),
            ]
        );
    }

    #[test]
    fn multi_repo_inclusions_are_relative_to_the_scan_base_directory() {
        // The scanner runs from the workspace root, so inclusion patterns are
        // matched against `<repo>/<path>`, not against the repo-relative path.
        let repos = vec!["Contoso-Web".into(), "ContosoPortal".into()];
        let files = vec![
            "Contoso-Web/src/Foo.cs".into(),
            "Contoso-Web/src/Baz.cs".into(),
            "ContosoPortal/src/Bar.cs".into(),
        ];
        let targets = build_scan_targets("Contoso", &repos, &files, false);
        let web = targets.iter().find(|t| t.sources == "Contoso-Web").unwrap();
        assert_eq!(
            web.inclusions,
            "Contoso-Web/src/Foo.cs,Contoso-Web/src/Baz.cs"
        );
        let portal = targets
            .iter()
            .find(|t| t.sources == "ContosoPortal")
            .unwrap();
        assert_eq!(portal.inclusions, "ContosoPortal/src/Bar.cs");
    }

    #[test]
    fn single_repo_from_multi_workspace_keeps_prefixed_key() {
        let repos = vec!["AcmeAdvisor".into()];
        let targets = build_scan_targets_with_count("ax", &repos, &[], true, 10);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].project_key, "ax-AcmeAdvisor");
        assert_eq!(targets[0].sources, "AcmeAdvisor");
    }

    #[test]
    fn single_repo_workspace_uses_workspace_key() {
        let repos = vec!["my-repo".into()];
        let targets = build_scan_targets_with_count("ax", &repos, &[], true, 1);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].project_key, "ax");
    }

    fn token_target() -> ScanTarget {
        ScanTarget {
            project_key: "ax".into(),
            sources: ".".into(),
            inclusions: "src/a.rs".into(),
        }
    }

    fn assert_token_only_in_env(cmd: &Command, token: &str) {
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.iter().all(|a| !a.contains(token)),
            "token on the command line: {args:?}"
        );
        let env = cmd
            .get_envs()
            .find(|(k, _)| *k == "SONAR_TOKEN")
            .and_then(|(_, v)| v)
            .map(|v| v.to_string_lossy().into_owned());
        assert_eq!(env.as_deref(), Some(token));
    }

    #[test]
    fn native_scan_passes_the_token_in_the_environment_only() {
        let client = SonarClient::new(SonarConfig::default());
        let cmd = client.native_scan_command(Path::new("."), &token_target(), Some("squ_secret"));
        assert_token_only_in_env(&cmd, "squ_secret");
        assert!(cmd.get_args().any(|a| a == "-Dsonar.projectKey=ax"));
    }

    #[test]
    fn container_scan_passes_the_token_name_only() {
        let cmd = container_scan_command(
            "podman",
            vec![],
            Path::new("/p"),
            "http://h",
            &token_target(),
            Some("squ_secret"),
        );
        assert_token_only_in_env(&cmd, "squ_secret");
        let args: Vec<_> = cmd.get_args().collect();
        let i = args.iter().position(|a| *a == "-e").expect("-e");
        assert_eq!(args[i + 1], "SONAR_TOKEN");
    }

    #[test]
    fn the_scanner_version_is_read_from_its_banner() {
        for (out, major) in [
            ("INFO: SonarScanner 4.8.0.2856\nINFO: Java 17", Some(4)),
            ("INFO: SonarScanner 5.0.1.3006\n", Some(5)),
            ("17:02:11.120 INFO  SonarScanner CLI 6.2.1.4610\n", Some(6)),
            ("SonarScanner CLI 7.0.2.4839", Some(7)),
            (
                "INFO: SonarQube Scanner 3.3.0.1492\nINFO: Java 1.8",
                Some(3),
            ),
            ("INFO: SonarQube Scanner 2.8", Some(2)),
            ("INFO: SonarQube Runner 2.4", Some(2)),
            ("SonarQube Runner", Some(2)),
            ("no banner here", None),
        ] {
            assert_eq!(scanner_major_version(out), major, "{out}");
        }
        assert!(
            check_scanner_supports_env_token("INFO: SonarScanner 4.8.0.2856")
                .unwrap_err()
                .contains("5.0")
        );
        for old in [
            "INFO: SonarQube Scanner 3.3.0.1492",
            "INFO: SonarQube Runner 2.4",
        ] {
            assert!(
                check_scanner_supports_env_token(old).is_err(),
                "{old} must be refused"
            );
        }
        assert!(check_scanner_supports_env_token("INFO: SonarScanner 5.0.1.3006").is_ok());
        // An unknown banner is not refused.
        assert!(check_scanner_supports_env_token("something else").is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn an_old_native_scanner_is_refused_before_it_runs_without_the_token() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("scanned");
        let script = dir.path().join("sonar-scanner");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo 'INFO: SonarScanner 4.8.0.2856'; exit 0; fi\ntouch {}\n",
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let client = SonarClient::new(SonarConfig {
            scanner_path: script.display().to_string(),
            ..SonarConfig::default()
        });
        let err = client
            .run_native_scan(dir.path(), &token_target(), Some("squ_secret"))
            .unwrap_err();
        assert!(err.contains("5.0"), "{err}");
        assert!(!marker.exists(), "the scanner must not run");
        // Without a token there is nothing to pass, so the scan runs.
        client
            .run_native_scan(dir.path(), &token_target(), None)
            .unwrap();
        assert!(marker.exists());
    }

    #[test]
    fn a_scan_without_a_token_sets_no_token_variable() {
        let client = SonarClient::new(SonarConfig::default());
        let cmd = client.native_scan_command(Path::new("."), &token_target(), None);
        assert!(cmd.get_envs().all(|(k, _)| k != "SONAR_TOKEN"));
    }
}
