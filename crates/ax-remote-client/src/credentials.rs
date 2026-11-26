//! Credential secrets live only in the OS credential store. Disk holds URL/client metadata.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Connection {
    pub url: String,
    pub client_id: String,
    pub issuer: String,
    pub resource: String,
    pub token_endpoint: String,
    pub revocation_endpoint: Option<String>,
    pub expires_at: u64,
}

#[derive(Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn metadata_path() -> Result<PathBuf, String> {
    let home = ax_utils::paths::home_dir().ok_or("Home directory unavailable")?;
    Ok(home.join(".ax/remote-connections.json"))
}

fn entry(url: &str) -> Result<keyring::Entry, String> {
    let id = hex::encode(Sha256::digest(url.as_bytes()));
    keyring::Entry::new("io.wenneker.ax.remote-mcp", &id)
        .map_err(|_| "OS credential store unavailable".into())
}

pub fn load_tokens(url: &str) -> Result<Tokens, String> {
    let text = entry(url)?
        .get_password()
        .map_err(|_| "No credentials available; connect again or unlock the OS credential store")?;
    serde_json::from_str(&text).map_err(|_| "Stored credentials are invalid; reconnect".into())
}

pub fn connections() -> Result<Vec<Connection>, String> {
    let _lock = lock_file("remote-connections.lock")?;
    read_connections()
}

fn read_connections() -> Result<Vec<Connection>, String> {
    let path = metadata_path()?;
    match std::fs::read_to_string(path) {
        Ok(text) => {
            serde_json::from_str(&text).map_err(|_| "Invalid remote connection metadata".into())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(_) => Err("Cannot read remote connection metadata".into()),
    }
}

pub fn connection(url: &str) -> Result<Connection, String> {
    connections()?
        .into_iter()
        .find(|c| c.url == url)
        .ok_or_else(|| "Remote server is not connected".into())
}

fn write_connections(connections: &[Connection]) -> Result<(), String> {
    let path = metadata_path()?;
    let parent = path.parent().ok_or("Invalid metadata path")?;
    std::fs::create_dir_all(parent).map_err(|_| "Cannot create connection directory")?;
    let temp = parent.join(format!("remote-connections-{}.tmp", uuid::Uuid::new_v4()));
    let bytes =
        serde_json::to_vec_pretty(connections).map_err(|_| "Cannot encode connection metadata")?;
    std::fs::write(&temp, bytes).map_err(|_| "Cannot write connection metadata")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))
            .map_err(|_| "Cannot protect connection metadata")?;
    }
    // Windows rename cannot replace a file; use a serialized metadata update there.
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(&path).map_err(|_| "Cannot update connection metadata")?;
    }
    std::fs::rename(&temp, &path).map_err(|_| "Cannot update connection metadata".into())
}

pub fn save(connection: Connection, tokens: &Tokens) -> Result<(), String> {
    let _lock = lock_file("remote-connections.lock")?;
    write_tokens(&entry(&connection.url)?, tokens)?;
    let mut list = read_connections()?;
    list.retain(|c| c.url != connection.url);
    list.push(connection);
    write_connections(&list)
}

fn write_tokens(entry: &keyring::Entry, tokens: &Tokens) -> Result<(), String> {
    let text = serde_json::to_string(tokens).map_err(|_| "Cannot encode credentials")?;
    entry.set_password(&text).map_err(|_| {
        "Cannot save credentials in OS credential store; no plaintext fallback is used".into()
    })
}

pub fn remove(url: &str) -> Result<(), String> {
    let _lock = lock_file("remote-connections.lock")?;
    match entry(url)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(_) => return Err("Cannot remove credentials from OS credential store".into()),
    }
    let mut list = read_connections()?;
    list.retain(|c| c.url != url);
    write_connections(&list)
}

fn lock_file(name: &str) -> Result<std::fs::File, String> {
    use fs2::FileExt;
    let parent = metadata_path()?
        .parent()
        .ok_or("Invalid metadata path")?
        .to_path_buf();
    std::fs::create_dir_all(&parent).map_err(|_| "Cannot create connection directory")?;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(parent.join(name))
        .map_err(|_| "Cannot open connection lock")?;
    file.lock_exclusive()
        .map_err(|_| "Cannot lock connection metadata")?;
    Ok(file)
}

pub fn lock_refresh(url: &str) -> Result<std::fs::File, String> {
    lock_file(&format!(
        "remote-refresh-{}.lock",
        hex::encode(Sha256::digest(url.as_bytes()))
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn secure_store_failure_has_no_plaintext_fallback_and_logout_removes_secret() {
        let entry =
            keyring::Entry::new_with_credential(Box::new(keyring::mock::MockCredential::default()));
        let mock: &keyring::mock::MockCredential = entry.get_credential().downcast_ref().unwrap();
        let tokens = Tokens {
            access_token: "private-access".into(),
            refresh_token: Some("private-refresh".into()),
        };
        mock.set_error(keyring::Error::Invalid("store".into(), "locked".into()));
        assert!(write_tokens(&entry, &tokens)
            .unwrap_err()
            .contains("no plaintext fallback"));
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
        write_tokens(&entry, &tokens).unwrap();
        assert_eq!(
            serde_json::from_str::<Tokens>(&entry.get_password().unwrap())
                .unwrap()
                .access_token,
            "private-access"
        );
        entry.delete_credential().unwrap();
        assert!(matches!(entry.get_password(), Err(keyring::Error::NoEntry)));
        let connection = Connection {
            url: "https://ax.example/projects/demo/mcp".into(),
            client_id: "native".into(),
            issuer: "https://issuer.example/".into(),
            resource: "https://ax.example".into(),
            token_endpoint: "https://issuer.example/token".into(),
            revocation_endpoint: None,
            expires_at: 1,
        };
        let metadata = serde_json::to_string(&connection).unwrap();
        assert!(!metadata.contains("private-access") && !metadata.contains("private-refresh"));
    }
}
