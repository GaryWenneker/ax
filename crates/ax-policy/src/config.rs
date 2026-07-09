//! Policy storage mode — filesystem (default) or database-first.

use std::path::{Path, PathBuf};

use serde::Deserialize;

const CONFIG_FILENAME: &str = "ax.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PolicyStorage {
    #[default]
    Files,
    Database,
}

impl PolicyStorage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Database => "database",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "files" | "file" => Some(Self::Files),
            "database" | "db" => Some(Self::Database),
            _ => None,
        }
    }
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

/// Where the effective storage mode comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyStorageSource {
    Project,
    Global,
    Default,
}

impl PolicyStorageSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Global => "global",
            Self::Default => "default",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicyStorageStatus {
    pub effective: String,
    pub source: String,
    pub config_path: String,
    pub global_config_path: String,
    pub project_value: Option<String>,
    pub global_value: Option<String>,
}

/// Resolved storage mode and which config layer set it.
pub fn policy_storage_status(project_root: &Path) -> PolicyStorageStatus {
    let global = read_policy_section(&global_config_path());
    let project_path = project_root.join(CONFIG_FILENAME);
    let local = read_policy_section(&project_path);

    let (effective, source) = match local.storage {
        Some(s) => (s, PolicyStorageSource::Project),
        None => match global.storage {
            Some(s) => (s, PolicyStorageSource::Global),
            None => (PolicyStorage::Files, PolicyStorageSource::Default),
        },
    };

    PolicyStorageStatus {
        effective: effective.as_str().into(),
        source: source.label().into(),
        config_path: project_path.display().to_string(),
        global_config_path: global_config_path().display().to_string(),
        project_value: local.storage.map(|s| s.as_str().into()),
        global_value: global.storage.map(|s| s.as_str().into()),
    }
}

/// Set `policy.storage` in per-project `ax.json` (merges other keys).
pub fn write_project_policy_storage(project_root: &Path, storage: PolicyStorage) -> Result<PathBuf, String> {
    let path = project_root.join(CONFIG_FILENAME);
    write_policy_storage_at(&path, storage)?;
    Ok(path)
}

/// Set `policy.storage` in global `~/.ax/config.json` (merges other keys).
pub fn write_global_policy_storage(storage: PolicyStorage) -> Result<PathBuf, String> {
    let path = global_config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_policy_storage_at(&path, storage)?;
    Ok(path)
}

fn write_policy_storage_at(path: &Path, storage: PolicyStorage) -> Result<(), String> {
    let mut root: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let policy = root
        .as_object_mut()
        .ok_or_else(|| "config root must be a JSON object".to_string())?;

    let mut policy_obj = policy
        .remove("policy")
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    policy_obj.insert("storage".into(), serde_json::Value::String(storage.as_str().into()));
    policy.insert("policy".into(), serde_json::Value::Object(policy_obj));

    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n";
    std::fs::write(path, text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_storage_is_files() {
        let cfg = load_policy_config(Path::new("/nonexistent"));
        assert_eq!(cfg.storage, PolicyStorage::Files);
    }

    #[test]
    fn write_project_storage_preserves_other_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILENAME);
        std::fs::write(
            &path,
            r#"{"index":{"exclude":["**/tmp/**"]},"policy":{"storage":"files"}}"#,
        )
        .unwrap();

        write_project_policy_storage(dir.path(), PolicyStorage::Database).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        let root: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(root["policy"]["storage"], "database");
        assert_eq!(root["index"]["exclude"][0], "**/tmp/**");
    }

    #[test]
    fn policy_storage_parse_aliases() {
        assert_eq!(PolicyStorage::parse("db"), Some(PolicyStorage::Database));
        assert_eq!(PolicyStorage::parse("file"), Some(PolicyStorage::Files));
    }
}
