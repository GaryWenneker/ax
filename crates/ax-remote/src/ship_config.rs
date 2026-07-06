//! Full ship.toml parsing (remote + quality gate + sonar).

use std::collections::HashMap;
use std::path::Path;

use ax_quality::SonarConfig;
use serde::Deserialize;

use crate::config::RemoteConfig;

#[derive(Debug, Clone, Deserialize)]
pub struct ShipConfig {
    #[serde(default)]
    pub ship: ShipSection,
    #[serde(default)]
    pub quality_gate: QualityGateSection,
    #[serde(default)]
    pub remote: RemoteConfig,
    #[serde(default)]
    pub sonar: SonarConfig,
    #[serde(default)]
    pub reviewers: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShipSection {
    #[serde(default = "default_branch")]
    pub target_branch: String,
    #[serde(default = "default_port")]
    pub web_port: u16,
}

fn default_branch() -> String {
    "main".into()
}

fn default_port() -> u16 {
    7070
}

impl Default for ShipSection {
    fn default() -> Self {
        Self {
            target_branch: default_branch(),
            web_port: default_port(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QualityGateSection {
    #[serde(default = "default_steps")]
    pub steps: Vec<String>,
    #[serde(default)]
    pub tests: TestRunnerSection,
}

fn default_steps() -> Vec<String> {
    vec![
        "index".into(),
        "tia".into(),
        "tests".into(),
        "sonar".into(),
        "policy".into(),
    ]
}

impl Default for QualityGateSection {
    fn default() -> Self {
        Self {
            steps: default_steps(),
            tests: TestRunnerSection::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TestRunnerSection {
    #[serde(default = "default_runner")]
    pub runner: String,
}

fn default_runner() -> String {
    "cargo test".into()
}

impl Default for TestRunnerSection {
    fn default() -> Self {
        Self {
            runner: default_runner(),
        }
    }
}

pub fn load_ship_config(project_root: &Path) -> ShipConfig {
    let path = project_root.join(".ax").join("ship.toml");
    if !path.exists() {
        return ShipConfig {
            ship: ShipSection::default(),
            quality_gate: QualityGateSection::default(),
            remote: RemoteConfig::default(),
            sonar: SonarConfig::default(),
            reviewers: HashMap::new(),
        };
    }
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    toml::from_str(&text).unwrap_or_else(|_| ShipConfig {
        ship: ShipSection::default(),
        quality_gate: QualityGateSection::default(),
        remote: RemoteConfig::default(),
        sonar: SonarConfig::default(),
        reviewers: HashMap::new(),
    })
}
