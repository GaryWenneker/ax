use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    /// External process extractor (always supported).
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Relative path to a wasm module (requires `--features wasm`).
    #[serde(default)]
    pub wasm: Option<String>,
    /// Working directory relative to the plugin folder (default: plugin root).
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub dir: PathBuf,
    pub manifest: PluginManifest,
}

impl LoadedPlugin {
    pub fn matches_ext(&self, ext: &str) -> bool {
        let needle = if ext.starts_with('.') {
            ext.to_ascii_lowercase()
        } else {
            format!(".{}", ext.to_ascii_lowercase())
        };
        self.manifest
            .extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(&needle) || e.eq_ignore_ascii_case(ext))
    }
}

pub fn discover_plugins(project_root: &Path) -> Vec<LoadedPlugin> {
    let root = project_root.join(".ax").join("plugins");
    if !root.is_dir() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return out;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let manifest_path = dir.join("plugin.toml");
        if !manifest_path.is_file() {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        match toml::from_str::<PluginManifest>(&raw) {
            Ok(manifest) => out.push(LoadedPlugin { dir, manifest }),
            Err(e) => {
                tracing::warn!(path = %manifest_path.display(), error = %e, "invalid plugin.toml");
            }
        }
    }
    out.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    out
}
