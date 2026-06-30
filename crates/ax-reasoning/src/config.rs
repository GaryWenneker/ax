//! Global configuration (`~/.ax/config.json` + `AX_OFFLOAD_*` env).
//!
//! Two sections are supported:
//!
//! ```json
//! {
//!   "index": {
//!     "extensions": { ".vue": "typescript" },
//!     "exclude":    ["**/coverage/**"],
//!     "includeIgnored": []
//!   },
//!   "offload": {
//!     "url": "https://api.openai.com/v1",
//!     "model": "gpt-4o",
//!     "key_env": "OPENAI_API_KEY"
//!   }
//! }
//! ```
//!
//! `index` provides global defaults that every project inherits.
//! Per-project `ax.json` values are merged on top and take precedence.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Index defaults (global)
// ---------------------------------------------------------------------------

/// Global index defaults stored in `~/.ax/config.json` under `"index"`.
/// These are applied to every project and can be overridden per-project in `ax.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalIndexConfig {
    /// Extra file-extension → language mappings applied to every project.
    #[serde(default)]
    pub extensions: HashMap<String, String>,
    /// Glob patterns excluded from indexing in every project (tracked or not).
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Gitignored directories to index in every project.
    #[serde(default)]
    pub include_ignored: Vec<String>,
}

/// Read the `"index"` section from `~/.ax/config.json`, or return defaults.
pub fn read_global_index_config() -> GlobalIndexConfig {
    let Ok(content) = fs::read_to_string(config_path()) else {
        return GlobalIndexConfig::default();
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) else {
        return GlobalIndexConfig::default();
    };
    root.get("index")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Offload config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OffloadConfig {
    pub url: Option<String>,
    pub model: Option<String>,
    pub key_env: Option<String>,
    pub effort: Option<String>,
    pub style: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedOffload {
    pub enabled: bool,
    pub url: Option<String>,
    pub model: String,
    pub api_key: Option<String>,
    pub key_source: Option<String>,
    pub effort: String,
    pub style: String,
    pub timeout_ms: u64,
    pub max_tokens: u32,
    pub strip: bool,
    pub debug: bool,
    pub origin: String,
}

pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ax"))
        .unwrap_or_else(|| PathBuf::from(".ax"))
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn read_offload_config() -> OffloadConfig {
    let Ok(content) = fs::read_to_string(config_path()) else {
        return OffloadConfig::default();
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) else {
        return OffloadConfig::default();
    };
    root.get("offload")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

pub fn write_offload_config(offload: Option<OffloadConfig>) -> Result<(), String> {
    let path = config_path();
    let mut root: serde_json::Value = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if offload.is_none() {
        if let Some(obj) = root.as_object_mut() {
            obj.remove("offload");
        }
    } else if let Some(cfg) = offload {
        root["offload"] = serde_json::to_value(cfg).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(config_dir()).map_err(|e| e.to_string())?;
    fs::write(path, serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n")
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn trimmed(s: Option<&str>) -> Option<String> {
    s.map(str::trim)
        .filter(|t| !t.is_empty())
        .map(|t| t.to_string())
}

pub fn resolve_offload() -> ResolvedOffload {
    if std::env::var("AX_OFFLOAD_DISABLE").ok().as_deref() == Some("1") {
        return ResolvedOffload {
            enabled: false,
            url: None,
            model: "gpt-oss-120b".into(),
            api_key: None,
            key_source: None,
            effort: "low".into(),
            style: "plain".into(),
            timeout_ms: 20000,
            max_tokens: 12000,
            strip: false,
            debug: std::env::var("AX_OFFLOAD_DEBUG").ok().as_deref() == Some("1"),
            origin: "none".into(),
        };
    }

    let c = read_offload_config();
    let env_url = trimmed(std::env::var("AX_OFFLOAD_URL").ok().as_deref());
    let env_key = trimmed(std::env::var("AX_OFFLOAD_KEY").ok().as_deref());
    let had_env_url = env_url.is_some();

    let url = env_url.or_else(|| trimmed(c.url.as_deref()));
    let model = trimmed(std::env::var("AX_OFFLOAD_MODEL").ok().as_deref())
        .or_else(|| trimmed(c.model.as_deref()))
        .unwrap_or_else(|| "gpt-oss-120b".into());

    let (api_key, key_source) = if let Some(k) = env_key {
        (Some(k), Some("AX_OFFLOAD_KEY".into()))
    } else if let Some(key_env) = trimmed(c.key_env.as_deref()) {
        if let Some(k) = trimmed(std::env::var(&key_env).ok().as_deref()) {
            (Some(k), Some(key_env))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    let origin = if had_env_url {
        "env"
    } else if trimmed(c.url.as_deref()).is_some() {
        "config"
    } else {
        "none"
    };

    ResolvedOffload {
        enabled: url.is_some(),
        url,
        model,
        api_key,
        key_source,
        effort: trimmed(std::env::var("AX_OFFLOAD_EFFORT").ok().as_deref())
            .or_else(|| trimmed(c.effort.as_deref()))
            .unwrap_or_else(|| "low".into()),
        style: trimmed(std::env::var("AX_OFFLOAD_STYLE").ok().as_deref())
            .or_else(|| trimmed(c.style.as_deref()))
            .unwrap_or_else(|| "plain".into()),
        timeout_ms: std::env::var("AX_OFFLOAD_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(20000),
        max_tokens: std::env::var("AX_OFFLOAD_MAXTOKENS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(12000),
        strip: std::env::var("AX_OFFLOAD_STRIP").ok().as_deref() == Some("1"),
        debug: std::env::var("AX_OFFLOAD_DEBUG").ok().as_deref() == Some("1"),
        origin: origin.to_string(),
    }
}

pub fn is_offload_enabled() -> bool {
    resolve_offload().enabled
}
