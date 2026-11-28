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
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        err @ keyring::Error::PlatformFailure(_) => {
            classify_secret_service(err, refused, session_bus_present())
        }
        _ => SecretError::Refused(refused.into()),
    }
}

/// Linux and BSD: sorts a Secret Service error. The Secret Service is reached
/// over the D-Bus session bus, so without a session bus there is no store.
/// Neither is there one on a bus where no program provides
/// `org.freedesktop.secrets` (containers, WSL, minimal desktops).
#[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
fn classify_secret_service(err: keyring::Error, refused: &str, bus_present: bool) -> SecretError {
    match err {
        keyring::Error::PlatformFailure(inner)
            if !bus_present || no_secret_service(inner.as_ref()) =>
        {
            SecretError::Absent(UNAVAILABLE.into())
        }
        _ => SecretError::Refused(refused.into()),
    }
}

/// Whether a Secret Service error (or one it wraps) says nothing on the bus
/// provides the service. keyring passes the `secret-service` crate's error on
/// as a boxed error, so this reads the D-Bus error names in its text: zbus
/// prints them as `<name>: <detail>`, and `secret-service` reports a missing
/// provider as "no secret service provider or dbus session found".
#[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
fn no_secret_service(err: &(dyn std::error::Error + 'static)) -> bool {
    const MARKERS: [&str; 3] = [
        "org.freedesktop.DBus.Error.ServiceUnknown",
        "org.freedesktop.DBus.Error.NameHasNoOwner",
        "no secret service provider or dbus session found",
    ];
    let mut next = Some(err);
    while let Some(e) = next {
        let text = e.to_string();
        if MARKERS.iter().any(|m| text.contains(m)) {
            return true;
        }
        next = e.source();
    }
    false
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

    /// An error shaped like the one keyring wraps from the `secret-service`
    /// crate (its `Display` is what this test controls).
    #[derive(Debug)]
    struct Wrapped(&'static str);
    impl std::fmt::Display for Wrapped {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str(self.0)
        }
    }
    impl std::error::Error for Wrapped {}

    fn platform_failure(text: &'static str) -> keyring::Error {
        keyring::Error::PlatformFailure(Box::new(Wrapped(text)))
    }

    /// A session bus with no Secret Service on it (containers, WSL, minimal
    /// desktops) has no store that could hold a token, so it is absent, not
    /// refusing: otherwise clearing a token there fails forever.
    #[test]
    fn a_session_bus_without_a_secret_service_is_an_absent_store() {
        for text in [
            "zbus error: org.freedesktop.DBus.Error.ServiceUnknown: The name \
             org.freedesktop.secrets was not provided by any .service files",
            "zbus error: org.freedesktop.DBus.Error.NameHasNoOwner: Could not get owner of \
             name 'org.freedesktop.secrets': no such name",
            "no secret service provider or dbus session found",
        ] {
            let err = classify_secret_service(platform_failure(text), "no", true);
            assert!(err.is_absent(), "{text}: {err:?}");
        }
        // No session bus at all: absent too.
        let err = classify_secret_service(platform_failure("zbus error: I/O"), "no", false);
        assert!(err.is_absent(), "{err:?}");
        // A Secret Service that is there but fails a call refuses.
        for err in [
            platform_failure("zbus error: org.freedesktop.DBus.Error.NoReply: timed out"),
            keyring::Error::NoStorageAccess(Box::new(Wrapped("SS Error: object locked"))),
        ] {
            let err = classify_secret_service(err, "no", true);
            assert!(!err.is_absent(), "{err:?}");
        }
    }

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
