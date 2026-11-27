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
    /// Seconds one file may take before the plugin is stopped
    /// (default [`DEFAULT_TIMEOUT_SECS`]).
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// Per-file time limit for a plugin whose manifest sets no `timeout_secs`.
pub const DEFAULT_TIMEOUT_SECS: u64 = 60;

impl PluginManifest {
    /// The per-file time limit for this plugin.
    pub fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS).max(1))
    }
}

#[derive(Debug, Clone)]
pub struct LoadedPlugin {
    pub dir: PathBuf,
    pub manifest: PluginManifest,
    /// blake3 of the plugin.toml bytes; trust is bound to it.
    pub manifest_hash: String,
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

/// Every parsable `.ax/plugins/*/plugin.toml` under `project_root`, trusted
/// or not. Use [`crate::PluginHost::load`] to get only the plugins that may run.
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
        let manifest_hash = blake3::hash(raw.as_bytes()).to_hex().to_string();
        match toml::from_str::<PluginManifest>(&raw) {
            Ok(manifest) => out.push(LoadedPlugin {
                dir,
                manifest,
                manifest_hash,
            }),
            Err(e) => {
                tracing::warn!(path = %manifest_path.display(), error = %e, "invalid plugin.toml");
            }
        }
    }
    out.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    out
}
