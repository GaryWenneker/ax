//! API keys are kept in the OS credential store. There is no plaintext fallback.
use std::{collections::HashMap, sync::Mutex};

pub trait SecretStore: Send + Sync {
    fn get(&self, id: &str) -> Result<Option<String>, String>;
    fn set(&self, id: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}

/// macOS Keychain, Windows Credential Manager or the Linux Secret Service.
pub struct KeyringStore;

const SERVICE: &str = "io.wenneker.ax.admin";

fn entry(id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, id).map_err(|_| "OS credential store unavailable".to_string())
}

impl SecretStore for KeyringStore {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        match entry(id)?.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(_) => Err("Cannot read the OS credential store; unlock it and try again".into()),
        }
    }
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        entry(id)?.set_password(secret).map_err(|_| {
            "Cannot save the key in the OS credential store; no plaintext fallback is used".into()
        })
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        match entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err("Cannot remove the key from the OS credential store".into()),
        }
    }
}

/// Keeps secrets in memory for the lifetime of the process and writes nothing.
/// Used by tests, and by `AX_ADMIN_SECRET_STORE=memory` for automated UI tests.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl MemoryStore {
    pub fn len(&self) -> usize {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        Ok(self.0.lock().unwrap_or_else(|e| e.into_inner()).get(id).cloned())
    }
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_owned(), secret.to_owned());
        Ok(())
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        Ok(())
    }
}
