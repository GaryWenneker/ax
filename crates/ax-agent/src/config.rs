//! Agent + workspace preferences in `~/.ax/config.json`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentsConfig {
    #[serde(default)]
    pub preferred_external: Option<String>,
    /// Last agent picked in the Agent terminal dropdown (persists across reloads).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_terminal_agent: Option<String>,
    #[serde(default)]
    pub enabled_targets: Vec<String>,
    #[serde(default = "default_terminal_mode")]
    pub terminal_mode: String,
    #[serde(default)]
    pub active_profile: HashMap<String, String>,
    #[serde(default)]
    pub profiles: HashMap<String, Vec<super::profiles::ProfileEntry>>,
}

fn default_terminal_mode() -> String {
    "auto".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceConfig {
    #[serde(default)]
    pub recent: Vec<RecentProject>,
    #[serde(default)]
    pub browse_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: String,
    pub label: String,
    #[serde(default)]
    pub last_opened: u64,
    #[serde(default)]
    pub initialized: bool,
}

pub fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ax").join("config.json"))
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalAxConfig {
    #[serde(default)]
    pub agents: AgentsConfig,
    #[serde(default)]
    pub workspace: WorkspaceConfig,
}

fn read_config_root(path: &Path) -> serde_json::Value {
    if !path.exists() {
        return serde_json::json!({});
    }
    let text = fs::read_to_string(path).unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    if value.is_object() {
        value
    } else {
        serde_json::json!({})
    }
}

pub fn load_global_config() -> GlobalAxConfig {
    let Some(path) = config_path() else {
        return GlobalAxConfig::default();
    };
    if !path.exists() {
        return GlobalAxConfig::default();
    }
    let value = read_config_root(&path);
    serde_json::from_value(value).unwrap_or_default()
}

pub fn save_global_config(cfg: &GlobalAxConfig) -> Result<(), String> {
    let path = config_path().ok_or("no home dir")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Merge with existing to preserve index/policy keys
    let mut merged = read_config_root(&path);
    let agents = serde_json::to_value(&cfg.agents).map_err(|e| e.to_string())?;
    let workspace = serde_json::to_value(&cfg.workspace).map_err(|e| e.to_string())?;
    let obj = merged
        .as_object_mut()
        .expect("read_config_root always returns an object");
    obj.insert("agents".into(), agents);
    obj.insert("workspace".into(), workspace);
    fs::write(
        &path,
        serde_json::to_string_pretty(&merged).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())
}

pub fn load_agents_config() -> AgentsConfig {
    load_global_config().agents
}

pub fn save_agents_config(agents: &AgentsConfig) -> Result<(), String> {
    let mut cfg = load_global_config();
    cfg.agents = agents.clone();
    save_global_config(&cfg)
}

pub fn load_workspace_config() -> WorkspaceConfig {
    load_global_config().workspace
}

pub fn save_workspace_config(workspace: &WorkspaceConfig) -> Result<(), String> {
    let mut cfg = load_global_config();
    cfg.workspace = workspace.clone();
    save_global_config(&cfg)
}

pub fn touch_recent_project(path: &Path, initialized: bool) -> Result<(), String> {
    let abs = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf());
    let path_str = abs.to_string_lossy().into_owned();
    let label = abs
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut cfg = load_global_config();
    cfg.workspace.recent.retain(|p| p.path != path_str);
    cfg.workspace.recent.insert(
        0,
        RecentProject {
            path: path_str,
            label,
            last_opened: now,
            initialized,
        },
    );
    cfg.workspace.recent.truncate(20);
    save_global_config(&cfg)
}

pub fn default_browse_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        roots.push(home.clone());
        for sub in ["projects", "dev", "code", "src", "gary"] {
            let p = home.join(sub);
            if p.is_dir() {
                roots.push(p);
            }
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_config_root_treats_null_as_empty_object() {
        let path = std::env::temp_dir().join(format!("ax-config-test-{}.json", std::process::id()));
        std::fs::write(&path, "null").unwrap();
        let root = read_config_root(&path);
        assert!(root.is_object());
        assert!(root.as_object().unwrap().is_empty());
        let _ = std::fs::remove_file(path);
    }
}
