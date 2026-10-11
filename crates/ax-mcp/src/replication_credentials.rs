//! Local replication secrets are stored in the OS credential store, never JSON.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub token: String,
    pub control: String,
}

fn entry(root: &Path, node_id: &str) -> Result<keyring::Entry, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "Cannot resolve replication project")?;
    let id = hex::encode(Sha256::digest(
        format!("{}:{node_id}", root.display()).as_bytes(),
    ));
    keyring::Entry::new("io.wenneker.ax.replication", &id)
        .map_err(|_| "OS credential store unavailable".into())
}

pub fn load(root: &Path, node_id: &str) -> Result<Credentials, String> {
    let text = entry(root, node_id)?.get_password().map_err(|_| {
        "Replication credential unavailable; unlock the OS credential store or reconnect"
    })?;
    serde_json::from_str(&text).map_err(|_| "Stored replication credentials are invalid".into())
}

pub fn save(root: &Path, node_id: &str, credentials: &Credentials) -> Result<(), String> {
    write(&entry(root, node_id)?, credentials)
}

/// Return the actual prior keychain value for rollback if settings cannot commit.
pub fn replace(
    root: &Path,
    node_id: &str,
    credentials: &Credentials,
) -> Result<Option<Credentials>, String> {
    let entry = entry(root, node_id)?;
    let previous = match entry.get_password() {
        Ok(text) => Some(
            serde_json::from_str(&text)
                .map_err(|_| "Stored replication credentials are invalid")?,
        ),
        Err(keyring::Error::NoEntry) => None,
        Err(_) => {
            return Err(
                "Cannot read replication credential store; unlock it before reconnecting".into(),
            )
        }
    };
    write(&entry, credentials)?;
    Ok(previous)
}

fn write(entry: &keyring::Entry, credentials: &Credentials) -> Result<(), String> {
    let text =
        serde_json::to_string(credentials).map_err(|_| "Cannot encode replication credentials")?;
    entry.set_password(&text).map_err(|_| "Cannot save replication credentials in the OS credential store; no plaintext fallback is used".into())
}

pub fn remove(root: &Path, node_id: &str) -> Result<(), String> {
    match entry(root, node_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("Cannot remove replication credential from the OS credential store".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn credentials_use_secure_storage_without_plaintext_fallback() {
        let entry =
            keyring::Entry::new_with_credential(Box::new(keyring::mock::MockCredential::default()));
        let mock: &keyring::mock::MockCredential = entry.get_credential().downcast_ref().unwrap();
        let credentials = Credentials {
            token: "replication-secret".into(),
            control: "local-control-secret".into(),
        };
        mock.set_error(keyring::Error::Invalid("store".into(), "locked".into()));
        assert!(write(&entry, &credentials)
            .unwrap_err()
            .contains("no plaintext fallback"));
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
        write(&entry, &credentials).unwrap();
        let saved: Credentials = serde_json::from_str(&entry.get_password().unwrap()).unwrap();
        assert_eq!(saved.token, credentials.token);
        entry.delete_credential().unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
    }
}
