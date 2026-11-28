//! Git share API tokens live in the OS credential store (macOS Keychain,
//! Windows Credential Manager or the Linux Secret Service), keyed by project
//! root and share target. Nothing here writes a token to a plain-text file.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};

/// Why a credential-store call failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretError {
    /// This machine has no credential store ax can use at all (no keychain,
    /// no Secret Service, no logon session), so it cannot hold a token
    /// either.
    Absent(String),
    /// The store is there but refused this call (locked, busy, access
    /// denied). A later call may succeed and find the same tokens.
    Refused(String),
}

impl SecretError {
    pub fn is_absent(&self) -> bool {
        matches!(self, SecretError::Absent(_))
    }
}

impl std::fmt::Display for SecretError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SecretError::Absent(msg) | SecretError::Refused(msg) => f.write_str(msg),
        }
    }
}

impl From<SecretError> for String {
    fn from(e: SecretError) -> Self {
        e.to_string()
    }
}

/// Where git share API tokens are kept.
pub trait ShareTokenStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, key: &str, token: &str) -> Result<(), SecretError>;
    fn delete(&self, key: &str) -> Result<(), SecretError>;
}

/// The OS credential store, through the `keyring` crate (as ax-admin does).
pub struct KeyringStore;

const SERVICE: &str = "io.wenneker.ax.share";

const UNAVAILABLE: &str = "OS credential store unavailable";

fn entry(key: &str) -> Result<keyring::Entry, SecretError> {
    // No entry can be made at all: there is no credential store backend.
    keyring::Entry::new(SERVICE, key).map_err(|_| SecretError::Absent(UNAVAILABLE.into()))
}

/// Sorts a keyring error into "there is no store" and "the store refused".
/// Only errors that mean no store exists count as absent; a locked or busy
/// store (macOS `errSecInteractionNotAllowed`, a locked Secret Service
/// collection, which keyring reports as `NoStorageAccess`) is a refusal,
/// because its tokens come back once it is unlocked.
fn classify(err: keyring::Error, refused: &str) -> SecretError {
    match err {
        // macOS: no keychain, keychain not available or invalid. keyring
        // reports a read-only keychain (`errSecReadOnly`) the same way, but
        // that keychain still holds and returns its tokens, so it refuses.
        #[cfg(target_os = "macos")]
        keyring::Error::NoStorageAccess(inner) if !macos_read_only(inner.as_ref()) => {
            SecretError::Absent(UNAVAILABLE.into())
        }
        // Windows: no logon session, so no credential manager for this
        // process.
        #[cfg(target_os = "windows")]
        keyring::Error::NoStorageAccess(_) => SecretError::Absent(UNAVAILABLE.into()),
        // Linux and BSD: the Secret Service is reached over the D-Bus session
        // bus, so without a session bus there is no store.
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        keyring::Error::PlatformFailure(_) if !session_bus_present() => {
            SecretError::Absent(UNAVAILABLE.into())
        }
        _ => SecretError::Refused(refused.into()),
    }
}

/// Whether a keychain error keyring wrapped is `errSecReadOnly` (-25292).
#[cfg(target_os = "macos")]
fn macos_read_only(inner: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    const ERR_SEC_READ_ONLY: i32 = -25292;
    inner
        .downcast_ref::<security_framework::base::Error>()
        .is_some_and(|e| e.code() == ERR_SEC_READ_ONLY)
}

/// Whether this process has a D-Bus session bus to reach a Secret Service
/// through: `DBUS_SESSION_BUS_ADDRESS`, or the socket at
/// `$XDG_RUNTIME_DIR/bus` that D-Bus clients fall back to.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn session_bus_present() -> bool {
    let set = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty());
    set("DBUS_SESSION_BUS_ADDRESS")
        || std::env::var_os("XDG_RUNTIME_DIR")
            .filter(|dir| !dir.is_empty())
            .is_some_and(|dir| Path::new(&dir).join("bus").exists())
}

impl ShareTokenStore for KeyringStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        match entry(key)?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(classify(
                e,
                "Cannot read the OS credential store; unlock it and try again",
            )),
        }
    }
    fn set(&self, key: &str, token: &str) -> Result<(), SecretError> {
        entry(key)?.set_password(token).map_err(|e| {
            classify(
                e,
                "Cannot save the token in the OS credential store; no plain-text fallback is used",
            )
        })
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(classify(
                e,
                "Cannot remove the token from the OS credential store; unlock it and try again",
            )),
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
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self.map().get(key).cloned())
    }
    fn set(&self, key: &str, token: &str) -> Result<(), SecretError> {
        self.map().insert(key.to_owned(), token.to_owned());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
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
pub(crate) fn warn_unavailable_once(error: &SecretError) {
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| {
        tracing::warn!(
            "ax share: OS credential store unavailable ({error}); a git API token is read from \
             its old location, and a new token cannot be saved"
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A read-only macOS keychain (`errSecReadOnly`) still holds and returns
    /// its tokens, so a refused delete there is a refusal, not "no store".
    #[cfg(target_os = "macos")]
    #[test]
    fn a_read_only_macos_keychain_is_a_refusal_not_an_absent_store() {
        let read_only = security_framework::base::Error::from_code(-25292);
        let err = classify(keyring::Error::NoStorageAccess(Box::new(read_only)), "no");
        assert!(!err.is_absent(), "{err:?}");
        for absent in [-25291, -25294, -25295] {
            let e = security_framework::base::Error::from_code(absent);
            let err = classify(keyring::Error::NoStorageAccess(Box::new(e)), "no");
            assert!(err.is_absent(), "{absent}: {err:?}");
        }
    }
}
