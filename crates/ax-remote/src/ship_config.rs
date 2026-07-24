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
    pub auto_commit: AutoCommitSection,
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
            auto_commit: AutoCommitSection::default(),
            reviewers: HashMap::new(),
        }
    }
}

/// Opt-in Aider-style checkpointing around `ax ship --evaluate`. Disabled by
/// default — this is a deliberate automation feature, not a silent behavior
/// change. When enabled, uncommitted working-tree changes are committed
/// before the quality gate runs (so diff/TIA/Sonar see them as part of
/// history); on failure with `revert_on_fail`, that specific commit is undone
/// via `git reset --mixed` — never `--hard` — so file contents are never
/// discarded, only un-committed.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AutoCommitSection {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_auto_commit_message")]
    pub message: String,
    #[serde(default)]
    pub revert_on_fail: bool,
}

fn default_auto_commit_message() -> String {
    "ax: auto-checkpoint before quality gate".into()
}

impl Default for AutoCommitSection {
    fn default() -> Self {
        Self {
            enabled: false,
            message: default_auto_commit_message(),
            revert_on_fail: false,
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
    /// Emit inbound/outbound/enrichment MCP traces to stderr (Cursor Output).
    #[serde(default = "default_verbose_mcp")]
    pub verbose_mcp: bool,
    /// IANA timezone for Logging Date/time display (e.g. `Europe/Amsterdam`).
    /// Empty / `local` = browser local timezone in the Command Center UI.
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_show_savings() -> bool {
    true
}

fn default_show_agent() -> bool {
    true
}

fn default_verbose_mcp() -> bool {
    false
}

fn default_timezone() -> String {
    String::new()
}

impl Default for UiSection {
    fn default() -> Self {
        Self {
            show_savings: true,
            show_agent_terminal: true,
            verbose_mcp: false,
            timezone: String::new(),
        }
    }
}

pub fn load_ship_config(project_root: &Path) -> ShipConfig {
    let path = crate::config::config_path(project_root);
    let mut config = if !path.exists() {
        ShipConfig::default()
    } else {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        toml::from_str(&text).unwrap_or_else(|_| ShipConfig::default())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_commit_is_opt_in_by_default() {
        let config = ShipConfig::default();
        assert!(!config.auto_commit.enabled, "auto-commit must default to disabled");
        assert!(!config.auto_commit.revert_on_fail);
        assert!(!config.auto_commit.message.is_empty());
    }

    #[test]
    fn auto_commit_survives_toml_round_trip() {
        let mut config = ShipConfig::default();
        config.auto_commit.enabled = true;
        config.auto_commit.revert_on_fail = true;
        config.auto_commit.message = "checkpoint!".into();

        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: ShipConfig = toml::from_str(&text).unwrap();
        assert!(parsed.auto_commit.enabled);
        assert!(parsed.auto_commit.revert_on_fail);
        assert_eq!(parsed.auto_commit.message, "checkpoint!");
    }

    #[test]
    fn missing_auto_commit_section_in_older_toml_defaults_to_disabled() {
        // Simulates an existing ship.toml written before this feature existed.
        let text = "[ship]\ntarget_branch = \"main\"\n";
        let parsed: ShipConfig = toml::from_str(text).unwrap();
        assert!(!parsed.auto_commit.enabled);
    }

    #[test]
    fn missing_timezone_in_older_toml_defaults_to_empty_local() {
        let text = "[ui]\nverbose_mcp = true\n";
        let parsed: ShipConfig = toml::from_str(text).unwrap();
        assert!(parsed.ui.timezone.is_empty());
    }

    #[test]
    fn timezone_survives_toml_round_trip() {
        let mut config = ShipConfig::default();
        config.ui.timezone = "Europe/Amsterdam".into();
        let text = toml::to_string_pretty(&config).unwrap();
        let parsed: ShipConfig = toml::from_str(&text).unwrap();
        assert_eq!(parsed.ui.timezone, "Europe/Amsterdam");
    }
}
