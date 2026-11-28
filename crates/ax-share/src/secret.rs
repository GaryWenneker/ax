//! Git share API tokens live in the OS credential store (macOS Keychain,
//! Windows Credential Manager or the Linux Secret Service), keyed by project
//! root and share target. Nothing here writes a token to a plain-text file.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Where git share API tokens are kept.
pub trait ShareTokenStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, String>;
    fn set(&self, key: &str, token: &str) -> Result<(), String>;
    fn delete(&self, key: &str) -> Result<(), String>;
}

/// The OS credential store, through the `keyring` crate (as ax-admin does).
pub struct KeyringStore;

const SERVICE: &str = "io.wenneker.ax.share";

fn entry(key: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, key).map_err(|_| "OS credential store unavailable".to_string())
}

impl ShareTokenStore for KeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        match entry(key)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("Cannot read the OS credential store; unlock it and try again".into()),
        }
    }
    fn set(&self, key: &str, token: &str) -> Result<(), String> {
        entry(key)?.set_password(token).map_err(|_| {
            "Cannot save the token in the OS credential store; no plain-text fallback is used"
                .into()
        })
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("Cannot remove the token from the OS credential store".into()),
        }
    }
}

/// Keeps tokens in memory for the lifetime of the process and writes nothing.
/// Used by tests, and by `AX_SHARE_SECRET_STORE=memory` for automated runs.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl MemoryStore {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl ShareTokenStore for MemoryStore {
    fn get(&self, key: &str) -> Result<Option<String>, String> {
        Ok(self.map().get(key).cloned())
    }
    fn set(&self, key: &str, token: &str) -> Result<(), String> {
        self.map().insert(key.to_owned(), token.to_owned());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), String> {
        self.map().remove(key);
        Ok(())
    }
}

/// The store ax uses: the OS credential store, or a process-local memory
/// store when `AX_SHARE_SECRET_STORE=memory`.
pub fn default_store() -> &'static dyn ShareTokenStore {
    static MEMORY: OnceLock<MemoryStore> = OnceLock::new();
    if std::env::var("AX_SHARE_SECRET_STORE").as_deref() == Ok("memory") {
        return MEMORY.get_or_init(MemoryStore::default);
    }
    &KeyringStore
}

/// The credential-store key for one project's share target.
pub fn token_key(project_root: &Path, repo_url: &str) -> String {
    let root = std::fs::canonicalize(project_root).unwrap_or_else(|_| project_root.to_path_buf());
    format!("{}|{}", root.display(), repo_url.trim())
}

/// Logs the "credential store unavailable" warning once per process.
pub(crate) fn warn_unavailable_once(error: &str) {
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| {
        tracing::warn!(
            "ax share: OS credential store unavailable ({error}); a git API token is read from \
             its old location, and a new token cannot be saved"
        );
    });
}
