//! Project configuration from ax.json.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use ax_types::Language;
use serde::Deserialize;

use ax_context::directory::CONFIG_FILENAME;
use ax_extraction::grammars::is_language_supported;

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

#[derive(Debug, Clone, Default)]
pub struct ProjectConfig {
    pub extensions: HashMap<String, Language>,
    pub include_ignored: Vec<String>,
    pub exclude: Vec<String>,
}

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
            let cfg = load_from_disk(project_root);
            guard.insert(project_root.to_path_buf(), (mtime, cfg.clone()));
            return cfg;
        }
        load_from_disk(project_root)
    }
}

fn load_from_disk(project_root: &Path) -> ProjectConfig {
    let config_path = project_root.join(CONFIG_FILENAME);
    if !config_path.exists() {
        return ProjectConfig::default();
    }
    let content = std::fs::read_to_string(&config_path).unwrap_or_default();
    let file: ProjectConfigFile = serde_json::from_str(&content).unwrap_or_default();
    let mut extensions = HashMap::new();
    for (ext, lang_str) in file.extensions {
        let ext = if ext.starts_with('.') { ext } else { format!(".{}", ext) };
        if let Some(lang) = Language::from_str(&lang_str.to_lowercase()) {
            if is_language_supported(lang) {
                extensions.insert(ext, lang);
            }
        }
    }
    ProjectConfig {
        extensions,
        include_ignored: file.include_ignored,
        exclude: file.exclude,
    }
}
