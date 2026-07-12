//! Full ship.toml parsing (remote + quality gate + sonar).

use std::collections::HashMap;
use std::path::Path;

use ax_quality::SonarConfig;
use serde::{Deserialize, Serialize};

use crate::config::RemoteConfig;

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    pub ui: UiSection,
    #[serde(default)]
    pub reviewers: HashMap<String, String>,
}

impl Default for ShipConfig {
    fn default() -> Self {
        Self {
            ship: ShipSection::default(),
            quality_gate: QualityGateSection::default(),
            remote: RemoteConfig::default(),
            sonar: SonarConfig::default(),
            ui: UiSection::default(),
            reviewers: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShipSection {
    #[serde(default = "default_branch")]
    pub target_branch: String,
    #[serde(default = "default_port")]
    pub web_port: u16,
    /// Relative path from workspace root to a single git repo (legacy — prefer `git_roots`).
    #[serde(default)]
    pub git_root: Option<String>,
    /// Git repos under this workspace (folder names). Auto-discovered when empty.
    #[serde(default)]
    pub git_roots: Vec<String>,
    /// Per-repo diff base branch overrides (folder name → branch). Falls back to smart detection.
    #[serde(default)]
    pub target_branches: HashMap<String, String>,
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
            git_root: None,
            git_roots: Vec::new(),
            target_branches: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct QualityGateSection {
    #[serde(default = "default_steps")]
    pub steps: Vec<String>,
    #[serde(default)]
    pub tests: TestRunnerSection,
    /// `incremental` (sync dirty files) or `full` (re-index entire project).
    #[serde(default = "default_index_mode")]
    pub index_mode: String,
}

fn default_index_mode() -> String {
    "incremental".into()
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
            index_mode: default_index_mode(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UiSection {
    /// Show the Savings page in the sidebar.
    #[serde(default = "default_show_savings", alias = "show_tokens")]
    pub show_savings: bool,
    /// Show the Agent terminal page in the sidebar.
    #[serde(default = "default_show_agent")]
    pub show_agent_terminal: bool,
}

fn default_show_savings() -> bool {
    true
}

fn default_show_agent() -> bool {
    true
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            show_savings: true,
            show_agent_terminal: true,
        }
    }
}

pub fn load_ship_config(project_root: &Path) -> ShipConfig {
    let path = crate::config::config_path(project_root);
    let mut config = if !path.exists() {
        ShipConfig {
            ship: ShipSection::default(),
            quality_gate: QualityGateSection::default(),
            remote: RemoteConfig::default(),
            sonar: SonarConfig::default(),
            ui: UiSection::default(),
            reviewers: HashMap::new(),
        }
    } else {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        toml::from_str(&text).unwrap_or_else(|_| ShipConfig {
            ship: ShipSection::default(),
            quality_gate: QualityGateSection::default(),
            remote: RemoteConfig::default(),
            sonar: SonarConfig::default(),
            ui: UiSection::default(),
            reviewers: HashMap::new(),
        })
    };

    // Migrate legacy single git_root into git_roots when the list is empty.
    if config.ship.git_roots.is_empty() {
        if let Some(single) = config.ship.git_root.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            config.ship.git_roots = vec![single.to_string()];
        }
    }

    config
}

pub fn save_ship_config(project_root: &Path, config: &ShipConfig) -> Result<(), String> {
    let ax_dir = project_root.join(".ax");
    std::fs::create_dir_all(&ax_dir).map_err(|e| e.to_string())?;
    let path = crate::config::config_path(project_root);
    let text = toml::to_string_pretty(config).map_err(|e| e.to_string())?;
    std::fs::write(&path, text + "\n").map_err(|e| e.to_string())
}
