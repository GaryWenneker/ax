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

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
    /// Why offload is off although an endpoint is set (for status output).
    pub problem: Option<String>,
}

pub fn config_dir() -> PathBuf {
    ax_utils::paths::home_dir()
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
    let value = offload
        .map(|cfg| serde_json::to_value(cfg).map_err(|e| e.to_string()))
        .transpose()?;
    update_config(|root| {
        match value {
            Some(value) => {
                root.insert("offload".into(), value);
            }
            None => {
                root.remove("offload");
            }
        }
        Ok(true)
    })?;
    Ok(())
}

/// Read-modify-write of `~/.ax/config.json` under an advisory lock, replaced
/// atomically. A missing or empty file starts as `{}`. A file that does not
/// parse, or whose root is not a JSON object, is an error naming the file and
/// is left untouched, so the user's other sections are never lost. `edit`
/// returns whether anything changed; nothing is written when it did not.
fn update_config(
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<bool, String>,
) -> Result<bool, String> {
    let path = config_path();
    let shown = path.display().to_string();
    ax_utils::update_locked(&path, |current| {
        let mut root = match current {
            None => serde_json::Map::new(),
            Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => serde_json::Map::new(),
            Some(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(serde_json::Value::Object(map)) => map,
                Ok(_) => {
                    return Err(std::io::Error::other(
                        "the top level is not a JSON object; fix or remove the file",
                    ))
                }
                Err(e) => {
                    return Err(std::io::Error::other(format!(
                        "not valid JSON ({e}); fix or remove the file"
                    )))
                }
            },
        };
        if !edit(&mut root).map_err(std::io::Error::other)? {
            return Ok(None);
        }
        let text = serde_json::to_string_pretty(&serde_json::Value::Object(root))
            .map_err(std::io::Error::other)?;
        Ok(Some((text + "\n").into_bytes()))
    })
    .map_err(|e| format!("{shown}: {e}"))
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
    /// First provider whose API key is in the environment. Offload stays off until
    /// the user saves an endpoint (`ax offload set-endpoint`); this is only a hint.
    pub suggested: Option<OffloadProviderEntry>,
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

/// First provider whose API key is set in the environment: `(url, key, key_env)`.
/// Used only to suggest an endpoint; it never enables offload on its own.
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
    let catalog = serde_json::to_value(provider_catalog_entries()).map_err(|e| e.to_string())?;
    update_config(|root| {
        if root.contains_key("offload_catalog") {
            return Ok(false);
        }
        root.insert("offload_catalog".into(), catalog);
        Ok(true)
    })
}

/// Seed the provider catalog during `ax init` and report which providers have a key.
///
/// This never writes an active `offload` section: offload stays off until the user
/// saves an endpoint with `ax offload set-endpoint`. A discovered provider is
/// returned as `suggested`.
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
                    key_env: existing
                        .key_env
                        .clone()
                        .or_else(|| p.key_env.map(str::to_string)),
                    model: existing
                        .model
                        .clone()
                        .or_else(|| p.default_model.map(str::to_string)),
                })
        });
        return Ok(OffloadInitReport {
            catalog_written,
            active,
            discovered,
            suggested: None,
            skipped_existing: true,
        });
    }

    let suggested = OFFLOAD_PROVIDERS
        .iter()
        .find(|spec| discovered.iter().any(|d| d == spec.id))
        .map(|spec| OffloadProviderEntry {
            id: spec.id.to_string(),
            name: spec.name.to_string(),
            url: spec.url.to_string(),
            key_env: spec.key_env.map(str::to_string),
            model: spec.default_model.map(str::to_string),
        });

    Ok(OffloadInitReport {
        catalog_written,
        active: None,
        discovered,
        suggested,
        skipped_existing: false,
    })
}

/// Model used for an endpoint that is not in [`OFFLOAD_PROVIDERS`] when no
/// model is set.
const FALLBACK_MODEL: &str = "gpt-oss-120b";

/// The known provider whose base URL is `url` (a trailing `/` is ignored).
fn provider_for_url(url: &str) -> Option<&'static OffloadProviderSpec> {
    let url = url.trim().trim_end_matches('/');
    OFFLOAD_PROVIDERS
        .iter()
        .find(|spec| spec.url.eq_ignore_ascii_case(url))
}

/// Resolve the offload settings. The model is `AX_OFFLOAD_MODEL`, else the
/// configured `model`, else the default model of the known provider at the
/// endpoint. A known provider without a default model leaves offload off with
/// a `problem` until a model is set; an unknown endpoint falls back to
/// `gpt-oss-120b`.
pub fn resolve_offload() -> ResolvedOffload {
    if std::env::var("AX_OFFLOAD_DISABLE").ok().as_deref() == Some("1") {
        return ResolvedOffload {
            enabled: false,
            url: None,
            model: FALLBACK_MODEL.into(),
            api_key: None,
            key_source: None,
            effort: "low".into(),
            style: "plain".into(),
            timeout_ms: 20000,
            max_tokens: 12000,
            strip: false,
            debug: std::env::var("AX_OFFLOAD_DEBUG").ok().as_deref() == Some("1"),
            origin: "none".into(),
            problem: None,
        };
    }

    let c = read_offload_config();
    let env_url = trimmed(std::env::var("AX_OFFLOAD_URL").ok().as_deref());
    let env_key = trimmed(std::env::var("AX_OFFLOAD_KEY").ok().as_deref());
    let had_env_url = env_url.is_some();

    let url = env_url.or_else(|| trimmed(c.url.as_deref()));
    let set_model = trimmed(std::env::var("AX_OFFLOAD_MODEL").ok().as_deref())
        .or_else(|| trimmed(c.model.as_deref()));
    let provider = url.as_deref().and_then(provider_for_url);
    let mut problem = None;
    let model = match (set_model, provider) {
        (Some(model), _) => model,
        (None, Some(spec)) => match spec.default_model {
            Some(model) => model.to_string(),
            None => {
                problem = Some(format!(
                    "{} has no default model; set \"model\" in the offload section of ~/.ax/config.json or AX_OFFLOAD_MODEL",
                    spec.name
                ));
                String::new()
            }
        },
        // An endpoint ax does not know: keep the historical default.
        (None, None) => FALLBACK_MODEL.into(),
    };

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

    // Offload is opt-in: only an endpoint saved in config or AX_OFFLOAD_URL turns it on.
    // A provider API key in the environment alone never sends source anywhere.
    let origin = if had_env_url {
        "env"
    } else if url.is_some() {
        "config"
    } else {
        "none"
    };
    if url.is_none() {
        api_key = None;
        key_source = None;
    }

    ResolvedOffload {
        enabled: url.is_some() && problem.is_none(),
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
        problem,
    }
}

/// Env var of the first provider whose API key is set, when offload is not configured.
/// For status output only: offload stays off until an endpoint is saved.
pub fn suggested_key_env() -> Option<String> {
    detect_provider().2
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

    /// Tests in this module change process-wide env vars; run them one at a time.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    const OFFLOAD_ENV: &[&str] = &[
        "AX_HOME_DIR",
        "AX_OFFLOAD_DISABLE",
        "AX_OFFLOAD_URL",
        "AX_OFFLOAD_KEY",
        "AX_OFFLOAD_MODEL",
        "OPENAI_API_KEY",
        "CEREBRAS_API_KEY",
        "GROQ_API_KEY",
        "TOGETHER_API_KEY",
        "FIREWORKS_API_KEY",
        "OPENROUTER_API_KEY",
    ];

    /// Runs `f` with a fresh home dir, no offload env, then `vars` set; restores the env after.
    fn with_env<T>(vars: &[(&str, &str)], f: impl FnOnce(&std::path::Path) -> T) -> T {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let saved: Vec<(&str, Option<String>)> = OFFLOAD_ENV
            .iter()
            .map(|k| (*k, std::env::var(k).ok()))
            .collect();
        for k in OFFLOAD_ENV {
            std::env::remove_var(k);
        }
        let home = tempfile::tempdir().expect("tempdir");
        std::env::set_var("AX_HOME_DIR", home.path());
        for (k, v) in vars {
            std::env::set_var(k, v);
        }
        let out = f(home.path());
        for (k, v) in saved {
            match v {
                Some(v) => std::env::set_var(k, v),
                None => std::env::remove_var(k),
            }
        }
        out
    }

    #[test]
    fn auto_detect_openai_key() {
        let (url, key, src) = with_env(&[("OPENAI_API_KEY", "sk-test-key")], |_| detect_provider());
        assert_eq!(url.as_deref(), Some("https://api.openai.com/v1"));
        assert_eq!(key.as_deref(), Some("sk-test-key"));
        assert_eq!(src.as_deref(), Some("OPENAI_API_KEY"));
    }

    #[test]
    fn provider_key_alone_does_not_enable_offload() {
        let r = with_env(&[("CEREBRAS_API_KEY", "csk-test")], |_| resolve_offload());
        assert!(!r.enabled, "an API key in the env must not turn offload on");
        assert_eq!(r.url, None);
        assert_eq!(r.api_key, None);
        assert_eq!(r.origin, "none");
    }

    #[test]
    fn configured_url_enables_offload() {
        let r = with_env(&[("OPENAI_API_KEY", "sk-test")], |_| {
            write_offload_config(Some(OffloadConfig {
                url: Some("https://api.openai.com/v1".into()),
                key_env: Some("OPENAI_API_KEY".into()),
                ..OffloadConfig::default()
            }))
            .unwrap();
            resolve_offload()
        });
        assert!(r.enabled);
        assert_eq!(r.origin, "config");
        assert_eq!(r.api_key.as_deref(), Some("sk-test"));
    }

    #[test]
    fn env_url_enables_offload() {
        let r = with_env(&[("AX_OFFLOAD_URL", "http://localhost:11434/v1")], |_| {
            resolve_offload()
        });
        assert!(r.enabled);
        assert_eq!(r.origin, "env");
    }

    #[test]
    fn init_does_not_activate_offload_from_env_key() {
        let (report, cfg, text) = with_env(&[("OPENAI_API_KEY", "sk-test")], |home| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let report = rt.block_on(seed_offload_on_init()).unwrap();
            let text = std::fs::read_to_string(home.join(".ax/config.json")).unwrap();
            (report, read_offload_config(), text)
        });
        assert!(report.active.is_none(), "init must not activate offload");
        assert_eq!(
            report.suggested.as_ref().map(|p| p.id.as_str()),
            Some("openai")
        );
        assert!(cfg.url.is_none());
        assert!(text.contains("offload_catalog"));
        assert!(
            !text.contains("\"offload\""),
            "no active offload section: {text}"
        );
    }

    #[test]
    fn a_config_that_does_not_parse_is_never_overwritten() {
        let bad = "{\n  \"index\": { \"exclude\": [\"**/coverage/**\"] },\n}\n";
        let (offload, catalog, after) = with_env(&[], |home| {
            let path = home.join(".ax/config.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bad).unwrap();
            let offload = write_offload_config(Some(OffloadConfig {
                url: Some("https://api.openai.com/v1".into()),
                ..OffloadConfig::default()
            }));
            let catalog = write_provider_catalog();
            (offload, catalog, std::fs::read_to_string(&path).unwrap())
        });
        let err = offload.expect_err("a broken config must not be replaced");
        assert!(err.contains("config.json"), "{err}");
        assert!(catalog.is_err());
        assert_eq!(after, bad, "the file is left as the user wrote it");
    }

    #[test]
    fn a_config_whose_root_is_not_an_object_is_an_error_not_a_panic() {
        for root in ["[]", "\"text\"", "42"] {
            let (offload, catalog, after) = with_env(&[], |home| {
                let path = home.join(".ax/config.json");
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, root).unwrap();
                let offload = std::panic::catch_unwind(|| {
                    write_offload_config(Some(OffloadConfig::default()))
                });
                let catalog = std::panic::catch_unwind(write_provider_catalog);
                (offload, catalog, std::fs::read_to_string(&path).unwrap())
            });
            assert!(matches!(offload, Ok(Err(_))), "root {root}: offload write");
            assert!(matches!(catalog, Ok(Err(_))), "root {root}: catalog write");
            assert_eq!(after, root);
        }
    }

    #[test]
    fn writing_offload_keeps_the_index_section() {
        let after = with_env(&[], |home| {
            let path = home.join(".ax/config.json");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, r#"{"index":{"exclude":["**/coverage/**"]}}"#).unwrap();
            write_offload_config(Some(OffloadConfig {
                url: Some("http://localhost:11434/v1".into()),
                ..OffloadConfig::default()
            }))
            .unwrap();
            assert!(write_provider_catalog().unwrap());
            std::fs::read_to_string(&path).unwrap()
        });
        let value: serde_json::Value = serde_json::from_str(&after).unwrap();
        assert_eq!(value["index"]["exclude"][0], "**/coverage/**");
        assert_eq!(value["offload"]["url"], "http://localhost:11434/v1");
        assert!(value["offload_catalog"].is_array());
    }

    fn configured(url: &str, model: Option<&str>) -> ResolvedOffload {
        with_env(&[], |_| {
            write_offload_config(Some(OffloadConfig {
                url: Some(url.into()),
                model: model.map(str::to_string),
                ..OffloadConfig::default()
            }))
            .unwrap();
            resolve_offload()
        })
    }

    #[test]
    fn a_known_provider_without_a_model_uses_its_default_model() {
        let groq = configured("https://api.groq.com/openai/v1/", None);
        assert!(groq.enabled);
        assert_eq!(groq.model, "llama-3.3-70b-versatile");
        assert_eq!(groq.problem, None);
        let openai = with_env(&[("AX_OFFLOAD_URL", "https://api.openai.com/v1")], |_| {
            resolve_offload()
        });
        assert_eq!(openai.model, "gpt-4o");
        let cerebras = configured("https://api.cerebras.ai/v1", None);
        assert_eq!(cerebras.model, "gpt-oss-120b");
    }

    #[test]
    fn a_set_model_wins_over_the_provider_default() {
        assert_eq!(
            configured("https://api.groq.com/openai/v1", Some("mixtral")).model,
            "mixtral"
        );
        let r = with_env(
            &[
                ("AX_OFFLOAD_URL", "https://openrouter.ai/api/v1"),
                ("AX_OFFLOAD_MODEL", "openai/gpt-4o"),
            ],
            |_| resolve_offload(),
        );
        assert!(r.enabled);
        assert_eq!(r.model, "openai/gpt-4o");
    }

    #[test]
    fn a_provider_without_a_default_model_needs_one_before_offload_runs() {
        let r = configured("https://api.together.xyz/v1", None);
        assert!(!r.enabled, "no request that must fail");
        assert_eq!(r.origin, "config");
        let problem = r.problem.expect("status says why");
        assert!(problem.contains("Together AI"), "{problem}");
        assert!(problem.contains("AX_OFFLOAD_MODEL"), "{problem}");
    }
}
