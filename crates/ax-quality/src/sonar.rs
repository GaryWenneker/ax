//! SonarScanner CLI wrapper and Quality Gate API client.

use std::path::Path;
use std::process::Command;

use serde::Deserialize;
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SonarConfig {
    pub enabled: bool,
    pub host: String,
    pub project_key: String,
    pub token_env: String,
    pub scanner_path: String,
    pub podman_container: Option<String>,
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
        let url = format!("{}/api/system/status", self.config.host);
        let client = reqwest::Client::new();
        match client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => Ok(()),
            _ => {
                if let Some(container) = &self.config.podman_container {
                    info!("Starting SonarQube container {container}");
                    let status = Command::new("podman")
                        .args(["start", container])
                        .status()
                        .map_err(|e| format!("podman start failed: {e}"))?;
                    if !status.success() {
                        return Err(format!("podman start {container} failed"));
                    }
                    Ok(())
                } else {
                    Err(format!("SonarQube not reachable at {}", self.config.host))
                }
            }
        }
    }

    pub fn run_incremental_scan(
        &self,
        project_root: &Path,
        dirty_files: &[String],
    ) -> Result<(), String> {
        if !self.config.enabled || dirty_files.is_empty() {
            return Ok(());
        }
        let token = std::env::var(&self.config.token_env).ok();
        let inclusions = dirty_files.join(",");
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
