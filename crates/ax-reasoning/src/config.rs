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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffloadProviderEntry {
    pub id: String,
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OffloadProviderSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub key_env: Option<&'static str>,
    pub default_model: Option<&'static str>,
}

/// Supported OpenAI-compatible offload providers (priority order).
pub const OFFLOAD_PROVIDERS: &[OffloadProviderSpec] = &[
    OffloadProviderSpec {
        id: "openai",
        name: "OpenAI",
        url: "https://api.openai.com/v1",
        key_env: Some("OPENAI_API_KEY"),
        default_model: Some("gpt-4o"),
    },
    OffloadProviderSpec {
        id: "cerebras",
        name: "Cerebras",
        url: "https://api.cerebras.ai/v1",
        key_env: Some("CEREBRAS_API_KEY"),
        default_model: Some("gpt-oss-120b"),
    },
    OffloadProviderSpec {
        id: "groq",
        name: "Groq",
        url: "https://api.groq.com/openai/v1",
        key_env: Some("GROQ_API_KEY"),
        default_model: Some("llama-3.3-70b-versatile"),
    },
    OffloadProviderSpec {
        id: "together",
        name: "Together AI",
        url: "https://api.together.xyz/v1",
        key_env: Some("TOGETHER_API_KEY"),
        default_model: None,
    },
    OffloadProviderSpec {
        id: "fireworks",
        name: "Fireworks",
        url: "https://api.fireworks.ai/inference/v1",
        key_env: Some("FIREWORKS_API_KEY"),
        default_model: None,
    },
    OffloadProviderSpec {
        id: "openrouter",
        name: "OpenRouter",
        url: "https://openrouter.ai/api/v1",
        key_env: Some("OPENROUTER_API_KEY"),
        default_model: None,
    },
    OffloadProviderSpec {
        id: "ollama",
        name: "Ollama (local)",
        url: "http://localhost:11434/v1",
        key_env: None,
        default_model: Some("llama3"),
    },
];

#[derive(Debug, Clone)]
pub struct OffloadInitReport {
    pub catalog_written: bool,
    pub active: Option<OffloadProviderEntry>,
    pub discovered: Vec<String>,
    pub skipped_existing: bool,
}

fn provider_catalog_entries() -> Vec<OffloadProviderEntry> {
    OFFLOAD_PROVIDERS
        .iter()
        .map(|p| OffloadProviderEntry {
            id: p.id.to_string(),
            name: p.name.to_string(),
            url: p.url.to_string(),
            key_env: p.key_env.map(str::to_string),
            model: p.default_model.map(str::to_string),
        })
        .collect()
}

fn env_has_key(key_env: &str) -> bool {
    trimmed(std::env::var(key_env).ok().as_deref()).is_some()
}

fn discover_providers_from_env() -> Vec<&'static str> {
    OFFLOAD_PROVIDERS
        .iter()
        .filter(|p| p.key_env.is_some_and(env_has_key))
        .map(|p| p.id)
        .collect()
}

async fn ollama_reachable() -> bool {
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get("http://localhost:11434/api/tags")
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn detect_provider() -> (Option<String>, Option<String>, Option<String>) {
    for spec in OFFLOAD_PROVIDERS {
        if let Some(key_env) = spec.key_env {
            if let Some(k) = trimmed(std::env::var(key_env).ok().as_deref()) {
                return (
                    Some(spec.url.to_string()),
                    Some(k),
                    Some(key_env.to_string()),
                );
            }
        }
    }
    (None, None, None)
}

fn write_provider_catalog() -> Result<bool, String> {
    let path = config_path();
    let mut root: serde_json::Value = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    if root.get("offload_catalog").is_some() {
        return Ok(false);
    }

    root["offload_catalog"] = serde_json::to_value(provider_catalog_entries()).map_err(|e| e.to_string())?;
    fs::create_dir_all(config_dir()).map_err(|e| e.to_string())?;
    fs::write(
        path,
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(true)
}

/// Seed provider catalog and wire active offload during `ax init`.
pub async fn seed_offload_on_init() -> Result<OffloadInitReport, String> {
    let catalog_written = write_provider_catalog()?;
    let mut discovered: Vec<String> = discover_providers_from_env()
        .into_iter()
        .map(str::to_string)
        .collect();
    if ollama_reachable().await {
        discovered.push("ollama".into());
    }

    let existing = read_offload_config();
    if existing.url.is_some() {
        let active = existing.url.as_ref().and_then(|url| {
            OFFLOAD_PROVIDERS
                .iter()
                .find(|p| p.url == url.as_str())
                .map(|p| OffloadProviderEntry {
                    id: p.id.to_string(),
                    name: p.name.to_string(),
                    url: p.url.to_string(),
                    key_env: existing.key_env.clone().or_else(|| p.key_env.map(str::to_string)),
                    model: existing.model.clone().or_else(|| p.default_model.map(str::to_string)),
                })
        });
        return Ok(OffloadInitReport {
            catalog_written,
            active,
            discovered,
            skipped_existing: true,
        });
    }

    let mut active = None;
    let discovered_set: std::collections::HashSet<&str> = discovered.iter().map(|s| s.as_str()).collect();
    for spec in OFFLOAD_PROVIDERS {
        if !discovered_set.contains(spec.id) {
            continue;
        }
        let cfg = OffloadConfig {
            url: Some(spec.url.to_string()),
            model: spec.default_model.map(str::to_string),
            key_env: spec.key_env.map(str::to_string),
            effort: None,
            style: None,
        };
        write_offload_config(Some(cfg))?;
        active = Some(OffloadProviderEntry {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            url: spec.url.to_string(),
            key_env: spec.key_env.map(str::to_string),
            model: spec.default_model.map(str::to_string),
        });
        break;
    }

    Ok(OffloadInitReport {
        catalog_written,
        active,
        discovered,
        skipped_existing: false,
    })
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

    let mut url = env_url.or_else(|| trimmed(c.url.as_deref()));
    let model = trimmed(std::env::var("AX_OFFLOAD_MODEL").ok().as_deref())
        .or_else(|| trimmed(c.model.as_deref()))
        .unwrap_or_else(|| "gpt-oss-120b".into());

    let mut api_key = None;
    let mut key_source = None;

    if let Some(k) = env_key {
        api_key = Some(k);
        key_source = Some("AX_OFFLOAD_KEY".into());
    } else if let Some(key_env) = trimmed(c.key_env.as_deref()) {
        if let Some(k) = trimmed(std::env::var(&key_env).ok().as_deref()) {
            api_key = Some(k);
            key_source = Some(key_env);
        }
    }

    let mut origin = if had_env_url {
        "env"
    } else if trimmed(c.url.as_deref()).is_some() {
        "config"
    } else {
        "none"
    };

    if url.is_none() {
        let (auto_url, auto_key, auto_src) = detect_provider();
        if let Some(u) = auto_url {
            url = Some(u);
            if api_key.is_none() {
                api_key = auto_key;
                key_source = auto_src;
            }
            origin = "auto";
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_catalog_has_all_supported() {
        assert_eq!(OFFLOAD_PROVIDERS.len(), 7);
        assert!(OFFLOAD_PROVIDERS.iter().any(|p| p.id == "openai"));
        assert!(OFFLOAD_PROVIDERS.iter().any(|p| p.id == "ollama"));
    }

    #[test]
    fn auto_detect_openai_key() {
        let prev = std::env::var("OPENAI_API_KEY").ok();
        std::env::set_var("OPENAI_API_KEY", "sk-test-key");
        let (url, key, src) = detect_provider();
        std::env::remove_var("OPENAI_API_KEY");
        if let Some(v) = prev {
            std::env::set_var("OPENAI_API_KEY", v);
        }
        assert_eq!(url.as_deref(), Some("https://api.openai.com/v1"));
        assert_eq!(key.as_deref(), Some("sk-test-key"));
        assert_eq!(src.as_deref(), Some("OPENAI_API_KEY"));
    }
}
