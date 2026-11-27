//! Which project plugins the user has trusted to run.
//!
//! A plugin under `<project>/.ax/plugins/<name>/` can run any command, so a
//! cloned repository must not be able to run code just by being indexed.
//! Plugins only run after `ax plugins trust`, which records the plugin
//! folder and a hash of its `plugin.toml` in `~/.ax/plugin-trust.json`.
//! Changing `plugin.toml` (for example its `command`) needs a new trust.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::manifest::LoadedPlugin;

const STORE_FILE: &str = "plugin-trust.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TrustStore {
    #[serde(default)]
    trusted: Vec<TrustEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TrustEntry {
    /// Canonical plugin folder.
    dir: String,
    /// blake3 of the plugin.toml bytes.
    manifest_hash: String,
}

/// `~/.ax/plugin-trust.json` (honours `AX_HOME_DIR`).
pub fn trust_store_path() -> Option<PathBuf> {
    ax_utils::paths::home_dir().map(|h| h.join(".ax").join(STORE_FILE))
}

fn canonical_dir(dir: &Path) -> String {
    dir.canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn entry_for(plugin: &LoadedPlugin) -> TrustEntry {
    TrustEntry {
        dir: canonical_dir(&plugin.dir),
        manifest_hash: plugin.manifest_hash.clone(),
    }
}

fn parse_store(bytes: Option<Vec<u8>>) -> io::Result<TrustStore> {
    match bytes {
        None => Ok(TrustStore::default()),
        Some(b) if b.iter().all(u8::is_ascii_whitespace) => Ok(TrustStore::default()),
        Some(b) => serde_json::from_slice(&b).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("plugin trust store is not valid JSON: {e}"),
            )
        }),
    }
}

fn load_store(store: &Path) -> TrustStore {
    match std::fs::read(store) {
        Ok(b) => parse_store(Some(b)).unwrap_or_else(|e| {
            tracing::warn!(path = %store.display(), error = %e, "ignoring unreadable plugin trust store");
            TrustStore::default()
        }),
        Err(_) => TrustStore::default(),
    }
}

/// Whether `plugin` (same folder, same plugin.toml) is in the trust store.
pub fn is_trusted_in(store: &Path, plugin: &LoadedPlugin) -> bool {
    let want = entry_for(plugin);
    load_store(store).trusted.contains(&want)
}

fn edit_store(store: &Path, edit: impl FnOnce(&mut TrustStore)) -> io::Result<()> {
    ax_utils::update_locked(store, |current| {
        let mut parsed = parse_store(current)?;
        edit(&mut parsed);
        let body = serde_json::to_vec_pretty(&parsed)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(Some(body))
    })
    .map(|_| ())
}

/// Records `plugin` as trusted in `store`, replacing an older entry for the
/// same folder.
pub fn trust_in(store: &Path, plugin: &LoadedPlugin) -> io::Result<()> {
    let entry = entry_for(plugin);
    edit_store(store, |s| {
        s.trusted.retain(|e| e.dir != entry.dir);
        s.trusted.push(entry);
    })
}

/// Removes every trust entry for `plugin`'s folder from `store`.
pub fn untrust_in(store: &Path, plugin: &LoadedPlugin) -> io::Result<()> {
    let dir = canonical_dir(&plugin.dir);
    edit_store(store, |s| s.trusted.retain(|e| e.dir != dir))
}

fn default_store() -> io::Result<PathBuf> {
    trust_store_path().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no home dir"))
}

/// [`trust_in`] on the default store.
pub fn trust_plugin(plugin: &LoadedPlugin) -> io::Result<()> {
    trust_in(&default_store()?, plugin)
}

/// [`untrust_in`] on the default store.
pub fn untrust_plugin(plugin: &LoadedPlugin) -> io::Result<()> {
    untrust_in(&default_store()?, plugin)
}

/// [`is_trusted_in`] on the default store; false without a home folder.
pub fn is_trusted(plugin: &LoadedPlugin) -> bool {
    trust_store_path().is_some_and(|s| is_trusted_in(&s, plugin))
}
