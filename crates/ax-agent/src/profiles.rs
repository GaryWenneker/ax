//! Per-agent account profiles (ax-managed isolated directories).

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::{load_agents_config, save_agents_config, AgentsConfig};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum AuthStatus {
    Authenticated,
    #[default]
    NeedsAuth,
    Unknown,
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
    ax_utils::paths::home_dir().map(|h| h.join(".ax").join("agent-profiles"))
}

pub fn profile_data_dir(agent: &str, profile_id: &str) -> Option<PathBuf> {
    profiles_base().map(|b| b.join(agent).join(profile_id))
}

/// Agent and profile ids become folder names under `profiles_base()`, so they
/// must be one plain path segment: 1-64 letters, digits, `-` or `_`.
fn validate_segment(kind: &str, value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > 64
        || value
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
    {
        return Err(format!(
            "{kind} must be 1-64 letters, numbers, hyphens, or underscores"
        ));
    }
    Ok(())
}

/// A stored profile folder may only be deleted when it sits at least two
/// levels inside `profiles_base()` (`<base>/<agent>/<id>`), also after
/// following links. Anything else (an old or hand-edited config pointing at
/// `~/Documents` or at a whole agent folder) is refused.
fn deletable_profile_dir(data_dir: &Path) -> Result<(), String> {
    let base = profiles_base().ok_or("no home dir")?;
    let refuse = || {
        format!(
            "Refusing to delete {} because it is not a profile folder under {}; remove the profile with keep_dir to keep the folder",
            data_dir.display(),
            base.display()
        )
    };
    let rel = data_dir.strip_prefix(&base).map_err(|_| refuse())?;
    if rel.components().count() < 2 {
        return Err(refuse());
    }
    ax_utils::ensure_within(&base, data_dir).map_err(|_| refuse())?;
    let real_base = base.canonicalize().map_err(|_| refuse())?;
    let real = data_dir.canonicalize().map_err(|_| refuse())?;
    match real.strip_prefix(&real_base) {
        Ok(r) if r.components().count() >= 2 => Ok(()),
        _ => Err(refuse()),
    }
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
        return Err(format!(
            "Profile '{profile_id}' not found for agent '{agent}'"
        ));
    }
    cfg.active_profile
        .insert(agent.to_string(), profile_id.to_string());
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
    validate_segment("Agent id", agent)?;
    validate_segment("Profile id", id)?;
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
    if !keep_dir && !list[idx].data_dir.is_empty() {
        let path = PathBuf::from(&list[idx].data_dir);
        if path.exists() {
            deletable_profile_dir(&path)?;
        }
    }
    let entry = list.remove(idx);
    if cfg.active_profile.get(agent) == Some(&id.to_string()) {
        cfg.active_profile.remove(agent);
        if let Some(first) = list.first() {
            cfg.active_profile
                .insert(agent.to_string(), first.id.clone());
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
    let entry = list
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or("profile not found")?;
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

fn profile_env_for_entry(
    agent: &str,
    entry: &ProfileEntry,
) -> Result<Vec<(String, String)>, String> {
    match agent {
        "claude" => Ok(vec![("CLAUDE_CONFIG_DIR".into(), entry.data_dir.clone())]),
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
            if path.join("User").join("globalStorage").exists() || path.join("machineid").exists() {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_home;

    #[test]
    fn create_profile_refuses_an_empty_id() {
        let _home = test_home::fresh("empty-id");
        assert!(create_profile("claude", "", "x", None, None, None).is_err());
        assert!(create_profile("claude", "  ", "x", None, None, None).is_err());
    }

    #[test]
    fn create_profile_refuses_an_agent_that_leaves_the_profiles_folder() {
        let home = test_home::fresh("bad-agent");
        let victim = home.dir.join("Documents");
        fs::create_dir_all(&victim).unwrap();
        for agent in ["../..", "..", "", "a/b", "/abs", "c\\d"] {
            assert!(
                create_profile(agent, "Documents", "x", None, None, None).is_err(),
                "agent {agent:?} must be refused"
            );
        }
        assert!(list_profiles("../..").is_empty());
    }

    #[test]
    fn remove_profile_never_deletes_a_folder_outside_the_profiles_base() {
        let home = test_home::fresh("remove-outside");
        let victim = home.dir.join("Documents");
        fs::create_dir_all(&victim).unwrap();
        fs::write(victim.join("keep.txt"), b"keep").unwrap();
        // A config written by an older version (or by hand) that points a
        // profile at a folder outside ~/.ax/agent-profiles.
        let mut cfg = load_agents_config();
        cfg.profiles.insert(
            "claude".into(),
            vec![ProfileEntry {
                id: "evil".into(),
                label: "evil".into(),
                data_dir: victim.to_string_lossy().into_owned(),
                auth_status: AuthStatus::NeedsAuth,
                provider: None,
                key_env: None,
                model: None,
            }],
        );
        save_agents_config(&cfg).unwrap();
        let _ = remove_profile("claude", "evil", false);
        assert!(
            victim.join("keep.txt").exists(),
            "outside folder was deleted"
        );
    }

    #[test]
    fn remove_profile_never_deletes_the_agent_folder_itself() {
        let _home = test_home::fresh("remove-agent-dir");
        create_profile("claude", "work", "Work", None, None, None).unwrap();
        let agent_dir = profiles_base().unwrap().join("claude");
        let mut cfg = load_agents_config();
        cfg.profiles.get_mut("claude").unwrap().push(ProfileEntry {
            id: "whole".into(),
            label: "whole".into(),
            data_dir: agent_dir.to_string_lossy().into_owned(),
            auth_status: AuthStatus::NeedsAuth,
            provider: None,
            key_env: None,
            model: None,
        });
        save_agents_config(&cfg).unwrap();
        let _ = remove_profile("claude", "whole", false);
        assert!(
            agent_dir.join("work").exists(),
            "sibling profile was deleted"
        );
    }

    #[test]
    fn remove_profile_deletes_its_own_folder() {
        let _home = test_home::fresh("remove-own");
        let entry = create_profile("claude", "work", "Work", None, None, None).unwrap();
        assert!(Path::new(&entry.data_dir).exists());
        remove_profile("claude", "work", false).unwrap();
        assert!(!Path::new(&entry.data_dir).exists());
        assert!(list_profiles("claude").is_empty());
    }
}
