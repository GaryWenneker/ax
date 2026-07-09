//! SonarScanner CLI wrapper and Quality Gate API client.

use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use tracing::{info, warn};

use crate::bootstrap::{read_sonar_token, scanner_available, scanner_host_url};
use crate::container::{resolve_runtime, start_container, SONAR_SCANNER_IMAGE};

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
    metricKey: String,
    status: String,
    actualValue: String,
}

pub struct SonarClient {
    pub config: SonarConfig,
}

impl SonarClient {
    pub fn new(config: SonarConfig) -> Self {
        Self { config }
    }

    pub async fn ensure_running(&self) -> Result<(), String> {
        if !self.config.enabled {
            return Ok(());
        }
        let url = format!("{}/api/system/status", self.config.host.trim_end_matches('/'));
        let client = reqwest::Client::new();
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                return Ok(());
            }
        }
        let container = self
            .config
            .podman_container
            .as_deref()
            .unwrap_or("sonarqube");
        let pref = if self.config.container_runtime == "auto" {
            None
        } else {
            Some(self.config.container_runtime.as_str())
        };
        let runtime = resolve_runtime(pref)?;
        info!("Starting SonarQube container {container} via {}", runtime.cli());
        start_container(runtime, container)?;
        crate::container::wait_for_sonar(&self.config.host, 180).await
    }

    pub fn run_incremental_scan(
        &self,
        project_root: &Path,
        dirty_files: &[String],
    ) -> Result<(), String> {
        if !self.config.enabled || dirty_files.is_empty() {
            return Ok(());
        }
        let token = read_sonar_token(project_root, &self.config.token_env);
        let inclusions = dirty_files.join(",");

        if scanner_available(&self.config.scanner_path) {
            self.run_native_scan(project_root, &inclusions, token.as_deref())
        } else {
            self.run_container_scan(project_root, &inclusions, token.as_deref())
        }
    }

    fn run_native_scan(
        &self,
        project_root: &Path,
        inclusions: &str,
        token: Option<&str>,
    ) -> Result<(), String> {
        let mut cmd = Command::new(&self.config.scanner_path);
        cmd.current_dir(project_root)
            .arg(format!("-Dsonar.projectKey={}", self.config.project_key))
            .arg("-Dsonar.sources=.")
            .arg(format!("-Dsonar.inclusions={inclusions}"))
            .arg(format!("-Dsonar.host.url={}", self.config.host));
        if let Some(t) = token {
            cmd.arg(format!("-Dsonar.token={t}"));
        }
        let status = cmd
            .status()
            .map_err(|e| format!("sonar-scanner failed: {e}"))?;
        if !status.success() {
            return Err("sonar-scanner exited with error".into());
        }
        Ok(())
    }

    fn run_container_scan(
        &self,
        project_root: &Path,
        inclusions: &str,
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
            "-v".to_string(),
            format!("{}:/usr/src", mount.display()),
            "-w".to_string(),
            "/usr/src".to_string(),
            SONAR_SCANNER_IMAGE.to_string(),
            format!("-Dsonar.projectKey={}", self.config.project_key),
            "-Dsonar.sources=.".to_string(),
            format!("-Dsonar.inclusions={inclusions}"),
            format!("-Dsonar.host.url={host}"),
        ];
        if let Some(t) = token {
            args.push(format!("-Dsonar.token={t}"));
        }

        let status = Command::new(runtime.cli())
            .args(&args)
            .status()
            .map_err(|e| format!("container sonar-scanner failed: {e}"))?;
        if !status.success() {
            return Err(format!(
                "container sonar-scanner exited with error ({} run)",
                runtime.cli()
            ));
        }
        Ok(())
    }

    pub async fn fetch_quality_gate(&self) -> Result<QualityGateResult, String> {
        if !self.config.enabled {
            return Ok(QualityGateResult {
                status: "SKIPPED".into(),
                passed: true,
                conditions: vec![],
            });
        }
        let url = format!(
            "{}/api/qualitygates/project_status?projectKey={}",
            self.config.host, self.config.project_key
        );
        let client = reqwest::Client::new();
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            warn!("SonarQube quality gate API returned {}", resp.status());
            return Ok(QualityGateResult {
                status: "UNKNOWN".into(),
                passed: true,
                conditions: vec![],
            });
        }
        let body: SonarStatusResponse = resp.json().await.map_err(|e| e.to_string())?;
        let passed = body.project_status.status == "OK";
        Ok(QualityGateResult {
            status: body.project_status.status.clone(),
            passed,
            conditions: body
                .project_status
                .conditions
                .into_iter()
                .map(|c| SonarCondition {
                    metric_key: c.metricKey,
                    status: c.status,
                    actual_value: c.actualValue,
                })
                .collect(),
        })
    }
}
