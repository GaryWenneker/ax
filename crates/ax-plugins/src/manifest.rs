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
    /// blake3 over every file in the plugin folder (see
    /// [`plugin_content_hash`]); trust is bound to it.
    pub content_hash: String,
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
        match toml::from_str::<PluginManifest>(&raw) {
            Ok(manifest) => out.push(LoadedPlugin {
                content_hash: plugin_content_hash(&dir),
                dir,
                manifest,
            }),
            Err(e) => {
                tracing::warn!(path = %manifest_path.display(), error = %e, "invalid plugin.toml");
            }
        }
    }
    out.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    out
}

/// blake3 over everything in a plugin folder: `plugin.toml`, the scripts and
/// wasm modules it runs, and every other file below it, in sorted path order.
/// Symlinks are hashed by their target path, not followed. Any change to any
/// file (or a new or removed file) gives a new hash, so it needs a new trust.
pub fn plugin_content_hash(dir: &Path) -> String {
    let mut hasher = blake3::Hasher::new();
    hash_dir(&mut hasher, dir, Path::new(""));
    hasher.finalize().to_hex().to_string()
}

fn hash_dir(hasher: &mut blake3::Hasher, dir: &Path, rel: &Path) {
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.flatten().collect(),
        Err(_) => {
            hash_record(hasher, b"E", rel, b"");
            return;
        }
    };
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let rel = rel.join(entry.file_name());
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            hash_record(hasher, b"E", &rel, b"");
            continue;
        };
        let kind = meta.file_type();
        if kind.is_symlink() {
            let target = std::fs::read_link(&path).unwrap_or_default();
            hash_record(hasher, b"L", &rel, target.to_string_lossy().as_bytes());
        } else if kind.is_dir() {
            hash_record(hasher, b"D", &rel, b"");
            hash_dir(hasher, &path, &rel);
        } else if kind.is_file() {
            match std::fs::read(&path) {
                Ok(bytes) => hash_record(hasher, b"F", &rel, &bytes),
                Err(_) => hash_record(hasher, b"E", &rel, b""),
            }
        } else {
            hash_record(hasher, b"O", &rel, b"");
        }
    }
}

/// One length-prefixed record, so no two different folders hash alike.
fn hash_record(hasher: &mut blake3::Hasher, tag: &[u8], rel: &Path, body: &[u8]) {
    let name = rel.to_string_lossy();
    hasher.update(tag);
    hasher.update(&(name.len() as u64).to_le_bytes());
    hasher.update(name.as_bytes());
    hasher.update(&(body.len() as u64).to_le_bytes());
    hasher.update(body);
}
