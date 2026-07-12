//! Per-agent account profiles (ax-managed isolated directories).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{load_agents_config, save_agents_config, AgentsConfig};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatus {
    Authenticated,
    NeedsAuth,
    Unknown,
}

impl Default for AuthStatus {
    fn default() -> Self {
        Self::NeedsAuth
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileEntry {
    pub id: String,
    pub label: String,
    pub data_dir: String,
    #[serde(default)]
    pub auth_status: AuthStatus,
    /// Builtin profile: LLM provider id from ax-reasoning
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

pub fn profiles_base() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".ax").join("agent-profiles"))
}

pub fn profile_data_dir(agent: &str, profile_id: &str) -> Option<PathBuf> {
    profiles_base().map(|b| b.join(agent).join(profile_id))
}

pub fn list_profiles(agent: &str) -> Vec<ProfileEntry> {
    load_agents_config()
        .profiles
        .get(agent)
        .cloned()
        .unwrap_or_default()
}

pub fn active_profile_id(agent: &str) -> Option<String> {
    load_agents_config().active_profile.get(agent).cloned()
}

pub fn set_active_profile(agent: &str, profile_id: &str) -> Result<(), String> {
    let mut cfg = load_agents_config();
    let profiles = cfg.profiles.get(agent).cloned().unwrap_or_default();
    if !profiles.iter().any(|p| p.id == profile_id) {
        return Err(format!("Profile '{profile_id}' not found for agent '{agent}'"));
    }
    cfg.active_profile.insert(agent.to_string(), profile_id.to_string());
    save_agents_config(&cfg)
}

pub fn create_profile(
    agent: &str,
    id: &str,
    label: &str,
    provider: Option<&str>,
    key_env: Option<&str>,
    model: Option<&str>,
) -> Result<ProfileEntry, String> {
    if id.chars().any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_') {
        return Err("Profile id may only contain letters, numbers, hyphens, and underscores".into());
    }
    let mut cfg = load_agents_config();
    let list = cfg.profiles.entry(agent.to_string()).or_default();
    if list.iter().any(|p| p.id == id) {
        return Err(format!("Profile '{id}' already exists"));
    }

    let data_dir = if agent == "builtin" {
        String::new()
    } else {
        let dir = profile_data_dir(agent, id).ok_or("no home dir")?;
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        dir.to_string_lossy().into_owned()
    };

    let entry = ProfileEntry {
        id: id.to_string(),
        label: label.to_string(),
        data_dir,
        auth_status: if agent == "builtin" {
            AuthStatus::Authenticated
        } else {
            AuthStatus::NeedsAuth
        },
        provider: provider.map(str::to_string),
        key_env: key_env.map(str::to_string),
        model: model.map(str::to_string),
    };
    list.push(entry.clone());
    if !cfg.active_profile.contains_key(agent) {
        cfg.active_profile.insert(agent.to_string(), id.to_string());
    }
    save_agents_config(&cfg)?;
    Ok(entry)
}

pub fn remove_profile(agent: &str, id: &str, keep_dir: bool) -> Result<(), String> {
    let mut cfg = load_agents_config();
    let list = cfg.profiles.entry(agent.to_string()).or_default();
    let idx = list.iter().position(|p| p.id == id);
    let Some(idx) = idx else {
        return Err(format!("Profile '{id}' not found"));
    };
    let entry = list.remove(idx);
    if cfg.active_profile.get(agent) == Some(&id.to_string()) {
        cfg.active_profile.remove(agent);
        if let Some(first) = list.first() {
            cfg.active_profile.insert(agent.to_string(), first.id.clone());
        }
    }
    save_agents_config(&cfg)?;
    if !keep_dir && !entry.data_dir.is_empty() {
        let path = PathBuf::from(&entry.data_dir);
        if path.exists() {
            let _ = fs::remove_dir_all(path);
        }
    }
    Ok(())
}

pub fn update_profile(
    agent: &str,
    id: &str,
    label: Option<&str>,
    provider: Option<&str>,
    key_env: Option<&str>,
    model: Option<&str>,
) -> Result<ProfileEntry, String> {
    let mut cfg = load_agents_config();
    let list = cfg.profiles.get_mut(agent).ok_or("agent not found")?;
    let entry = list
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or("profile not found")?;
    if let Some(l) = label {
        if l.trim().is_empty() {
            return Err("Label cannot be empty".into());
        }
        entry.label = l.trim().to_string();
    }
    if provider.is_some() {
        entry.provider = provider.filter(|p| !p.is_empty()).map(str::to_string);
    }
    if key_env.is_some() {
        entry.key_env = key_env.filter(|k| !k.is_empty()).map(str::to_string);
    }
    if model.is_some() {
        entry.model = model.filter(|m| !m.is_empty()).map(str::to_string);
    }
    let updated = entry.clone();
    save_agents_config(&cfg)?;
    Ok(updated)
}

pub fn mark_authenticated(agent: &str, id: &str) -> Result<(), String> {
    update_auth_status(agent, id, AuthStatus::Authenticated)
}

pub fn update_auth_status(agent: &str, id: &str, status: AuthStatus) -> Result<(), String> {
    let mut cfg = load_agents_config();
    let list = cfg.profiles.get_mut(agent).ok_or("agent not found")?;
    let entry = list.iter_mut().find(|p| p.id == id).ok_or("profile not found")?;
    entry.auth_status = status;
    save_agents_config(&cfg)
}

pub fn profile_env(agent: &str, profile_id: &str) -> Result<Vec<(String, String)>, String> {
    let profiles = list_profiles(agent);
    let entry = profiles
        .iter()
        .find(|p| p.id == profile_id)
        .ok_or("profile not found")?;
    profile_env_for_entry(agent, entry)
}

fn profile_env_for_entry(agent: &str, entry: &ProfileEntry) -> Result<Vec<(String, String)>, String> {
    match agent {
        "claude" => Ok(vec![(
            "CLAUDE_CONFIG_DIR".into(),
            entry.data_dir.clone(),
        )]),
        "cursor" => Ok(vec![(
            "CURSOR_USER_DATA_DIR".into(),
            entry.data_dir.clone(),
        )]),
        _ => Ok(vec![]),
    }
}

/// Resolve profile env, auto-creating `default` when the UI sends that id but no profile exists yet.
pub fn ensure_profile_env(agent: &str, profile_id: &str) -> Result<Vec<(String, String)>, String> {
    if agent == "builtin" {
        return Ok(vec![]);
    }
    let profiles = list_profiles(agent);
    if let Some(entry) = profiles.iter().find(|p| p.id == profile_id) {
        return profile_env_for_entry(agent, entry);
    }
    if profile_id == "default" {
        let entry = create_profile(agent, "default", "Default", None, None, None)?;
        return profile_env_for_entry(agent, &entry);
    }
    Err(format!(
        "Profile '{profile_id}' not found for {agent} — create one in Settings → AI Agents"
    ))
}

pub fn detect_auth_status(agent: &str, data_dir: &str) -> AuthStatus {
    let path = Path::new(data_dir);
    if !path.exists() {
        return AuthStatus::NeedsAuth;
    }
    match agent {
        "claude" => {
            if path.join(".credentials.json").exists()
                || path.join("settings.json").exists()
                || path.join(".claude.json").exists()
            {
                AuthStatus::Authenticated
            } else {
                AuthStatus::NeedsAuth
            }
        }
        "cursor" => {
            if path.join("User").join("globalStorage").exists()
                || path.join("machineid").exists()
            {
                AuthStatus::Authenticated
            } else {
                AuthStatus::NeedsAuth
            }
        }
        "builtin" => AuthStatus::Authenticated,
        _ => AuthStatus::Unknown,
    }
}

pub fn refresh_auth_statuses() -> Result<AgentsConfig, String> {
    let mut cfg = load_agents_config();
    for (agent, profiles) in cfg.profiles.iter_mut() {
        for p in profiles.iter_mut() {
            if agent == "builtin" {
                p.auth_status = AuthStatus::Authenticated;
            } else if !p.data_dir.is_empty() {
                p.auth_status = detect_auth_status(agent, &p.data_dir);
            }
        }
    }
    save_agents_config(&cfg)?;
    Ok(cfg)
}
