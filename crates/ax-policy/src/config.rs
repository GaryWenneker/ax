//! Policy storage mode — filesystem (default) or database-first.

use std::path::{Path, PathBuf};

use serde::Deserialize;

const CONFIG_FILENAME: &str = "ax.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyStorage {
    #[default]
    Files,
    Database,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyConfigFile {
    #[serde(default)]
    storage: Option<PolicyStorage>,
}

#[derive(Debug, Clone)]
pub struct PolicyConfig {
    pub storage: PolicyStorage,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            storage: PolicyStorage::Files,
        }
    }
}

/// Merge global `~/.ax/config.json` `"policy"` section with per-project `ax.json`.
pub fn load_policy_config(project_root: &Path) -> PolicyConfig {
    let global = read_policy_section(&global_config_path());
    let local = read_policy_section(&project_root.join(CONFIG_FILENAME));
    PolicyConfig {
        storage: local
            .storage
            .or(global.storage)
            .unwrap_or(PolicyStorage::Files),
    }
}

fn read_policy_section(path: &Path) -> PolicyConfigFile {
    let Ok(content) = std::fs::read_to_string(path) else {
        return PolicyConfigFile::default();
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) else {
        return PolicyConfigFile::default();
    };
    root.get("policy")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn global_config_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ax").join("config.json"))
        .unwrap_or_else(|| PathBuf::from(".ax/config.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_storage_is_files() {
        let cfg = load_policy_config(Path::new("/nonexistent"));
        assert_eq!(cfg.storage, PolicyStorage::Files);
    }
}
