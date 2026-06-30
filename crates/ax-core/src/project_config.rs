//! Project configuration: per-project `ax.json` merged with global `~/.ax/config.json`.
//!
//! Merge order (last wins):
//!   1. Global defaults from `~/.ax/config.json` → `"index"` section
//!   2. Per-project overrides from `<project-root>/ax.json`
//!
//! `extensions`: global entries are used as a base; per-project entries win on conflict.
//! `exclude` / `includeIgnored`: lists are **unioned** (global + per-project, deduped).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use ax_types::Language;
use serde::Deserialize;

use ax_context::directory::CONFIG_FILENAME;
use ax_extraction::grammars::is_language_supported;

// ---------------------------------------------------------------------------
// Config file shapes (JSON deserialization)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfigFile {
    #[serde(default)]
    pub extensions: HashMap<String, String>,
    #[serde(default)]
    pub include_ignored: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Mirrors the `"index"` section of `~/.ax/config.json`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlobalIndexConfigFile {
    #[serde(default)]
    extensions: HashMap<String, String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    include_ignored: Vec<String>,
}

// ---------------------------------------------------------------------------
// Resolved config (Language-typed, ready to use)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ProjectConfig {
    pub extensions: HashMap<String, Language>,
    pub include_ignored: Vec<String>,
    pub exclude: Vec<String>,
}

// ---------------------------------------------------------------------------
// Cache
// ---------------------------------------------------------------------------

static CACHE: OnceLock<Mutex<HashMap<PathBuf, (u64, ProjectConfig)>>> = OnceLock::new();

impl ProjectConfig {
    pub fn load(project_root: &Path) -> Self {
        let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
        let config_path = project_root.join(CONFIG_FILENAME);
        let mtime = std::fs::metadata(&config_path)
            .and_then(|m| m.modified())
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        if let Ok(mut guard) = cache.lock() {
            if let Some((cached_mtime, cfg)) = guard.get(project_root) {
                if *cached_mtime == mtime {
                    return cfg.clone();
                }
            }
            let cfg = load_merged(project_root);
            guard.insert(project_root.to_path_buf(), (mtime, cfg.clone()));
            return cfg;
        }
        load_merged(project_root)
    }
}

// ---------------------------------------------------------------------------
// Merge: global base + per-project overrides
// ---------------------------------------------------------------------------

fn load_merged(project_root: &Path) -> ProjectConfig {
    let global = read_global_index_config();
    let local = read_project_config_file(project_root);

    // Extensions: global base, per-project wins on conflict.
    let mut raw_extensions = global.extensions;
    for (k, v) in local.extensions {
        raw_extensions.insert(k, v);
    }

    // Lists: union (global + per-project), order-preserving dedup.
    let exclude = dedup_union(global.exclude, local.exclude);
    let include_ignored = dedup_union(global.include_ignored, local.include_ignored);

    let mut extensions = HashMap::new();
    for (ext, lang_str) in raw_extensions {
        let ext = if ext.starts_with('.') { ext } else { format!(".{}", ext) };
        if let Some(lang) = Language::from_str(&lang_str.to_lowercase()) {
            if is_language_supported(lang) {
                extensions.insert(ext, lang);
            }
        }
    }

    ProjectConfig { extensions, include_ignored, exclude }
}

fn dedup_union(base: Vec<String>, overrides: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for item in base.into_iter().chain(overrides) {
        if seen.insert(item.clone()) {
            out.push(item);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Disk readers
// ---------------------------------------------------------------------------

fn read_project_config_file(project_root: &Path) -> ProjectConfigFile {
    let config_path = project_root.join(CONFIG_FILENAME);
    if !config_path.exists() {
        return ProjectConfigFile::default();
    }
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();
    serde_json::from_str(&content).unwrap_or_default()
}

fn global_config_path() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ax").join("config.json"))
        .unwrap_or_else(|| PathBuf::from(".ax/config.json"))
}

fn read_global_index_config() -> GlobalIndexConfigFile {
    let Ok(content) = std::fs::read_to_string(global_config_path()) else {
        return GlobalIndexConfigFile::default();
    };
    let Ok(root) = serde_json::from_str::<serde_json::Value>(&content) else {
        return GlobalIndexConfigFile::default();
    };
    root.get("index")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}
