//! SonarScanner CLI wrapper and Quality Gate API client.

use std::collections::BTreeMap;
use std::io::{Read as _, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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

use crate::bootstrap::{canonical_repo_project_key, read_sonar_token, scanner_available, scanner_host_url, sonar_reachable, workspace_sonar_key};
use crate::container::{find_container_any, resolve_runtime, start_sonar_stack, InstallLog, SONAR_SCANNER_IMAGE};

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

        let container_name = self
            .config
            .podman_container
            .as_deref()
            .unwrap_or("sonarqube");
        let Some(container) = find_container_any(container_name) else {
            return Err(format!(
                "SonarQube offline at {host} — install and start from the SonarQube page"
            ));
        };

        let pref = if self.config.container_runtime == "auto" {
            None
        } else {
            Some(self.config.container_runtime.as_str())
        };
        let runtime = resolve_runtime(pref)?;
        let log = InstallLog::new();
        // Always try to start when a container exists (idempotent if already running).
        // `start_container` retries up to 3 times.
        info!(
            "SonarQube offline — starting existing container '{}' ({}) via {}",
            container.name,
            container.status,
            runtime.cli()
        );
        start_sonar_stack(runtime, container_name, &log)?;
        crate::container::wait_for_sonar(host, 120).await.map_err(|e| {
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
        self.run_platform_scan_inner(project_root, repo_names, dirty_files, full_repo, None, &mut None)
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
        self.run_platform_scan_inner_counted(project_root, repo_names, &[], true, Some(log), &mut None, workspace_repo_count)
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
        self.run_platform_scan_inner_counted(project_root, repo_names, dirty_files, full_repo, log, progress, repo_names.len())
    }

    fn run_platform_scan_inner_counted(
        &self,
        project_root: &Path,
        repo_names: &[String],
        dirty_files: &[String],
        full_repo: bool,
        log: Option<&crate::container::InstallLog>,
        progress: &mut Option<&mut dyn FnMut(SonarScanProgressEvent)>,
        workspace_repo_count: usize,
    ) -> Result<(), String> {
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
        let targets = build_scan_targets_with_count(&workspace_key, repo_names, files_for_targets, full_repo, workspace_repo_count);
        emit_skipped_repos(
            &workspace_key,
            repo_names,
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

    fn append_scanner_auth(cmd: &mut Command, token: Option<&str>) {
        if let Some(t) = token {
            // SonarScanner 6.x uses sonar.token; older versions use sonar.login.
            // Provide both for backward compatibility.
            cmd.arg(format!("-Dsonar.token={t}"));
            cmd.arg(format!("-Dsonar.login={t}"));
        }
    }

    fn run_native_scan(
        &self,
        project_root: &Path,
        target: &ScanTarget,
        token: Option<&str>,
    ) -> Result<(), String> {
        let mut cmd = Command::new(&self.config.scanner_path);
        cmd.current_dir(project_root);
        if use_properties_file(&target.inclusions) {
            write_scan_properties(
                project_root,
                &target.project_key,
                &target.sources,
                &self.config.host,
                &target.inclusions,
                token,
            )?;
            cmd.arg("-Dproject.settings=.ax/sonar-scan.properties");
        } else {
            cmd.arg(format!("-Dsonar.projectKey={}", target.project_key))
                .arg(format!("-Dsonar.sources={}", target.sources))
                .arg(format!("-Dsonar.inclusions={}", target.inclusions))
                .arg(format!("-Dsonar.exclusions={DEFAULT_EXCLUSIONS}"))
                .arg(format!("-Dsonar.host.url={}", self.config.host));
            Self::append_scanner_auth(&mut cmd, token);
        }
        run_command_with_timeout(&mut cmd, SCAN_TIMEOUT)
    }

    fn run_container_scan(
        &self,
        project_root: &Path,
        target: &ScanTarget,
        token: Option<&str>,
    ) -> Result<(), String> {
        let pref = if self.config.container_runtime == "auto" {
            None
        } else {
            Some(self.config.container_runtime.as_str())
        };
        let runtime = resolve_runtime(pref)?;
        let mount = project_root
            .canonicalize()
            .map_err(|e| format!("project path: {e}"))?;
        let host = scanner_host_url(&self.config.host);

        info!(
            "Running SonarScanner via {} ({}) — native scanner not found on PATH",
            runtime.cli(),
            SONAR_SCANNER_IMAGE
        );

        let mut args = vec![
            "run".to_string(),
            "--rm".to_string(),
        ];
        if let Some(t) = token {
            args.push("-e".to_string());
            args.push(format!("SONAR_TOKEN={t}"));
        }
        args.extend([
            "-v".to_string(),
            format!("{}:/usr/src", mount.display()),
            "-w".to_string(),
            "/usr/src".to_string(),
            SONAR_SCANNER_IMAGE.to_string(),
        ]);

        if use_properties_file(&target.inclusions) {
            write_scan_properties(
                project_root,
                &target.project_key,
                &target.sources,
                &host,
                &target.inclusions,
                None,
            )?;
            args.push("-Dproject.settings=.ax/sonar-scan.properties".to_string());
        } else {
            args.extend([
                format!("-Dsonar.projectKey={}", target.project_key),
                format!("-Dsonar.sources={}", target.sources),
                format!("-Dsonar.inclusions={}", target.inclusions),
                format!("-Dsonar.exclusions={DEFAULT_EXCLUSIONS}"),
                format!("-Dsonar.host.url={host}"),
            ]);
        }

        let mut cmd = Command::new(runtime.cli());
        cmd.args(&args);
        run_command_with_timeout(&mut cmd, SCAN_TIMEOUT)
            .map_err(|e| format!("container sonar-scanner ({} run): {e}", runtime.cli()))
    }

    pub async fn fetch_quality_gate(
        &self,
        project_root: &Path,
        repo_names: &[String],
    ) -> Result<QualityGateResult, String> {
        self.fetch_quality_gate_inner(project_root, repo_names, true).await
    }

    /// Fetch without waiting for the Compute Engine (for status checks / dashboards).
    pub async fn fetch_quality_gate_snapshot(
        &self,
        project_root: &Path,
        repo_names: &[String],
    ) -> Result<QualityGateResult, String> {
        self.fetch_quality_gate_inner(project_root, repo_names, false).await
    }

    async fn fetch_quality_gate_inner(
        &self,
        project_root: &Path,
        repo_names: &[String],
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
        let keys: Vec<String> = if repo_names.len() <= 1 {
            vec![workspace_key]
        } else {
            repo_names
                .iter()
                .map(|repo| canonical_repo_project_key(&workspace_key, repo, true))
                .collect()
        };

        let mut combined = QualityGateResult {
            status: "OK".into(),
            passed: true,
            conditions: vec![],
        };

        for key in keys {
            let result = self.fetch_quality_gate_for_key(project_root, &key, wait_for_ce).await?;
            if !result.passed {
                combined.passed = false;
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
        let url = format!(
            "{}/api/qualitygates/project_status?projectKey={}",
            self.config.host.trim_end_matches('/'),
            project_key
        );
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let token = read_sonar_token(project_root, &self.config.token_env);
        let auth = token.as_deref().map(token_basic_auth);

        // SonarQube returns "NONE" while the Compute Engine is still processing
        // the analysis. Poll with back-off until we get a real result.
        const MAX_RETRIES: u32 = 10;
        const RETRY_DELAY: Duration = Duration::from_secs(3);

        for attempt in 0..=MAX_RETRIES {
            let mut req = client.get(&url);
            if let Some(ref a) = auth {
                req = req.header("Authorization", a.clone());
            }
            let resp = match req.send().await {
                Ok(r) => r,
                Err(e) => return Err(e.to_string()),
            };
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

            if status == "NONE" && wait_for_ce && attempt < MAX_RETRIES {
                info!(
                    project = %project_key,
                    attempt = attempt + 1,
                    "Quality gate NONE — CE task still pending, retrying in {}s",
                    RETRY_DELAY.as_secs()
                );
                tokio::time::sleep(RETRY_DELAY).await;
                continue;
            }

            let passed = matches!(status.as_str(), "OK" | "NONE" | "WARN");
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

        unreachable!()
    }
}

fn run_command_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<(), String> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("failed to start scanner: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    let mut stderr_buf = String::new();
                    if let Some(mut se) = child.stderr.take() {
                        let _ = se.read_to_string(&mut stderr_buf);
                    }
                    let detail = stderr_buf.trim();
                    let msg = if detail.is_empty() {
                        "exited with error".to_string()
                    } else {
                        detail.lines().last().unwrap_or(detail).to_string()
                    };
                    return Err(msg);
                }
                return Ok(());
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!(
                        "timed out after {}s — killed",
                        timeout.as_secs()
                    ));
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            Err(e) => return Err(format!("error waiting for scanner process: {e}")),
        }
    }
}

fn token_basic_auth(token: &str) -> String {
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    format!("Basic {}", STANDARD.encode(format!("{token}:")))
}

/// Windows CreateProcess limits (~8k); keep a margin for podman/docker args.
const CMDLINE_INCLUSIONS_LIMIT: usize = 4_000;
/// Above this file count, collapse to `{repo}/**` globs instead of listing every path.
const REPO_GLOB_THRESHOLD: usize = 200;

fn use_properties_file(inclusions: &str) -> bool {
    inclusions.len() > CMDLINE_INCLUSIONS_LIMIT
}

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
    targets: &[ScanTarget],
    full_repo: bool,
    progress: &mut Option<&mut dyn FnMut(SonarScanProgressEvent)>,
    log: Option<&crate::container::InstallLog>,
) {
    if repo_names.is_empty() {
        return;
    }
    let multi_repo = repo_names.len() > 1;
    let target_sources: std::collections::HashSet<&str> =
        targets.iter().map(|t| t.sources.as_str()).collect();
    for repo in repo_names {
        if target_sources.contains(repo.as_str()) {
            continue;
        }
        let project_key = canonical_repo_project_key(workspace_key, repo, multi_repo);
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

#[cfg(test)]
fn build_scan_targets(
    workspace_key: &str,
    repo_names: &[String],
    dirty_files: &[String],
    full_repo: bool,
) -> Vec<ScanTarget> {
    build_scan_targets_with_count(workspace_key, repo_names, dirty_files, full_repo, repo_names.len())
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
        let rel = file.strip_prefix(&format!("{repo}/")).unwrap_or(file);
        grouped.entry(repo.to_string()).or_default().push(rel.to_string());
    }

    // Use workspace_repo_count for multi-repo detection so that scanning a single repo
    // from a multi-repo workspace still uses the correct prefixed project key.
    let multi_repo = workspace_repo_count > 1;
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
                project_key: canonical_repo_project_key(workspace_key, repo, multi_repo),
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

fn write_scan_properties(
    project_root: &Path,
    project_key: &str,
    sources: &str,
    host: &str,
    inclusions: &str,
    token: Option<&str>,
) -> Result<PathBuf, String> {
    let ax_dir = project_root.join(".ax");
    std::fs::create_dir_all(&ax_dir).map_err(|e| e.to_string())?;
    let path = ax_dir.join("sonar-scan.properties");
    let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    writeln!(file, "sonar.projectKey={project_key}").map_err(|e| e.to_string())?;
    writeln!(file, "sonar.sources={sources}").map_err(|e| e.to_string())?;
    writeln!(file, "sonar.inclusions={inclusions}").map_err(|e| e.to_string())?;
    writeln!(file, "sonar.exclusions={DEFAULT_EXCLUSIONS}").map_err(|e| e.to_string())?;
    writeln!(file, "sonar.host.url={host}").map_err(|e| e.to_string())?;
    if let Some(t) = token {
        writeln!(file, "sonar.token={t}").map_err(|e| e.to_string())?;
        writeln!(file, "sonar.login={t}").map_err(|e| e.to_string())?;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let files: Vec<String> = (0..300)
            .map(|i| format!("src/File{i}.cs"))
            .collect();
        let inc = build_inclusions(&files);
        assert_eq!(inc, "**/*");
        assert!(!use_properties_file(&inc));
    }

    #[test]
    fn resolve_rel_path_splits_forward_slashes() {
        let root = Path::new(if cfg!(windows) { r"C:\workspace" } else { "/workspace" });
        let resolved = resolve_rel_path(root, "Mijn-Pf/src/Foo.cs");
        assert_eq!(resolved, root.join("Mijn-Pf").join("src").join("Foo.cs"));
    }

    #[test]
    fn groups_dirty_files_per_repo() {
        let repos = vec![
            "Mijn-Pf".into(),
            "Klantbeeld".into(),
            "Teamanalyse".into(),
        ];
        let files = vec![
            "Mijn-Pf/src/Foo.cs".into(),
            "Klantbeeld/src/Bar.cs".into(),
        ];
        let targets = build_scan_targets("VfPf", &repos, &files, false);
        assert_eq!(targets.len(), 2);
        assert!(targets.iter().all(|t| t.sources != "Teamanalyse"));
        let mijn = targets.iter().find(|t| t.sources == "Mijn-Pf").unwrap();
        assert!(mijn.inclusions.contains("Foo.cs"));
        let klant = targets.iter().find(|t| t.sources == "Klantbeeld").unwrap();
        assert!(klant.inclusions.contains("Bar.cs"));
    }

    #[test]
    fn single_repo_from_multi_workspace_keeps_prefixed_key() {
        let repos = vec!["Adviseurportaal".into()];
        let targets = build_scan_targets_with_count("ax", &repos, &[], true, 10);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].project_key, "ax-Adviseurportaal");
        assert_eq!(targets[0].sources, "Adviseurportaal");
    }

    #[test]
    fn single_repo_workspace_uses_workspace_key() {
        let repos = vec!["my-repo".into()];
        let targets = build_scan_targets_with_count("ax", &repos, &[], true, 1);
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].project_key, "ax");
    }
}
