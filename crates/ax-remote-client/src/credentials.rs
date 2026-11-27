//! Credential secrets live only in the OS credential store. Disk holds URL/client metadata.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub url: String,
    pub client_id: String,
    pub issuer: String,
    pub resource: String,
    pub token_endpoint: String,
    pub revocation_endpoint: Option<String>,
    pub expires_at: u64,
    /// Who uses this sign-in ([`USER_AX`], [`USER_ADMIN`]). One entry per
    /// address is shared by all of them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

/// `ax mcp remote login` and the Command Center.
pub const USER_AX: &str = "ax";
/// The ax-admin application.
pub const USER_ADMIN: &str = "ax-admin";

/// What is left after one user lets go of a sign-in.
#[derive(Debug, PartialEq)]
enum Release {
    /// Nobody uses it any more: remove the tokens and revoke them.
    Revoke,
    /// Others still use it (or nothing was stored): keep it, with this list.
    Keep(Vec<Connection>),
}

/// Stands for the users of an entry stored before users were recorded. They
/// are unknown, so such an entry is never revoked on someone else's behalf.
const USER_UNKNOWN: &str = "unknown";

fn users_of(connection: &Connection) -> Vec<String> {
    if connection.users.is_empty() {
        vec![USER_UNKNOWN.to_string()]
    } else {
        connection.users.clone()
    }
}

/// The list after saving `connection`, which replaces the entry of its
/// address. The users of the old entry stay users of the new one.
fn merged(mut list: Vec<Connection>, mut connection: Connection) -> Vec<Connection> {
    if let Some(old) = list.iter().find(|c| c.url == connection.url) {
        for user in users_of(old) {
            if !connection.users.contains(&user) {
                connection.users.push(user);
            }
        }
    }
    list.retain(|c| c.url != connection.url);
    list.push(connection);
    list
}

/// What happens when `user` stops using the sign-in for `url`.
fn released(mut list: Vec<Connection>, url: &str, user: &str) -> Release {
    let Some(entry) = list.iter_mut().find(|c| c.url == url) else {
        return Release::Keep(list);
    };
    let mut users = users_of(entry);
    users.retain(|u| u != user);
    if users.is_empty() {
        return Release::Revoke;
    }
    entry.users = users;
    Release::Keep(list)
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
    read_connections_at(&metadata_path()?)
}

fn read_connections_at(path: &std::path::Path) -> Result<Vec<Connection>, String> {
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
    write_connections_at(&metadata_path()?, connections)
}

fn write_connections_at(path: &std::path::Path, connections: &[Connection]) -> Result<(), String> {
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
        std::fs::remove_file(path).map_err(|_| "Cannot update connection metadata")?;
    }
    std::fs::rename(&temp, path).map_err(|_| "Cannot update connection metadata".into())
}

pub fn save(connection: Connection, tokens: &Tokens) -> Result<(), String> {
    let _lock = lock_file("remote-connections.lock")?;
    write_tokens(&entry(&connection.url)?, tokens)?;
    let list = merged(read_connections()?, connection);
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

/// Lets `user` stop using the sign-in for `url`. Returns true when nobody
/// uses it any more, so the caller removes and revokes the tokens. Otherwise
/// only `user` is taken off the entry.
pub fn release(url: &str, user: &str) -> Result<bool, String> {
    let _lock = lock_file("remote-connections.lock")?;
    match released(read_connections()?, url, user) {
        Release::Revoke => Ok(true),
        Release::Keep(list) => write_connections(&list).map(|()| false),
    }
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
            users: Vec::new(),
        };
        let metadata = serde_json::to_string(&connection).unwrap();
        assert!(!metadata.contains("private-access") && !metadata.contains("private-refresh"));
    }

    fn entry_for(url: &str, users: &[&str]) -> Connection {
        Connection {
            url: url.into(),
            client_id: "native".into(),
            issuer: "https://issuer.example/".into(),
            resource: "https://ax.example".into(),
            token_endpoint: "https://issuer.example/token".into(),
            revocation_endpoint: None,
            expires_at: 1,
            users: users.iter().map(|u| u.to_string()).collect(),
        }
    }

    const URL: &str = "https://ax.example/projects/app/mcp";

    #[test]
    fn connection_metadata_round_trips_privately_and_rejects_a_damaged_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".ax/remote-connections.json");
        assert_eq!(read_connections_at(&path).unwrap(), Vec::new());
        let other = "https://ax.example/projects/other/mcp";
        let list = merged(Vec::new(), entry_for(URL, &[USER_AX]));
        let list = merged(list, entry_for(other, &[USER_ADMIN]));
        write_connections_at(&path, &list).unwrap();
        assert_eq!(read_connections_at(&path).unwrap(), list);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        // A save for an address replaces its entry and keeps its users.
        let mut renewed = entry_for(URL, &[]);
        renewed.expires_at = 99;
        let list = merged(read_connections_at(&path).unwrap(), renewed);
        write_connections_at(&path, &list).unwrap();
        let stored = read_connections_at(&path).unwrap();
        assert_eq!(stored.len(), 2);
        let entry = stored.iter().find(|c| c.url == URL).unwrap();
        assert_eq!(
            (entry.expires_at, entry.users.clone()),
            (99, vec![USER_AX.to_string()])
        );
        // A removal keeps the other entries.
        let mut list = stored;
        list.retain(|c| c.url != URL);
        write_connections_at(&path, &list).unwrap();
        assert_eq!(read_connections_at(&path).unwrap().len(), 1);
        // No temporary files are left behind.
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            names,
            vec![std::ffi::OsString::from("remote-connections.json")]
        );
        std::fs::write(&path, "{ not json").unwrap();
        assert!(read_connections_at(&path).unwrap_err().contains("Invalid"));
    }

    #[test]
    fn a_sign_in_used_by_the_cli_is_not_revoked_when_ax_admin_lets_go() {
        // The CLI signed in, then ax-admin signed in to the same address.
        let list = merged(
            vec![entry_for(URL, &[USER_AX])],
            entry_for(URL, &[USER_ADMIN]),
        );
        assert_eq!(list.len(), 1);
        match released(list, URL, USER_ADMIN) {
            Release::Keep(list) => assert_eq!(list[0].users, vec![USER_AX.to_string()]),
            Release::Revoke => panic!("ax-admin revoked the CLI's sign-in"),
        }
    }

    #[test]
    fn a_sign_in_stored_before_users_were_recorded_is_never_revoked_by_ax_admin() {
        let list = merged(vec![entry_for(URL, &[])], entry_for(URL, &[USER_ADMIN]));
        assert!(matches!(released(list, URL, USER_ADMIN), Release::Keep(_)));
        // A refresh keeps the users it found.
        let list = merged(vec![entry_for(URL, &[USER_ADMIN])], entry_for(URL, &[]));
        assert_eq!(list[0].users, vec![USER_ADMIN.to_string()]);
    }

    #[test]
    fn a_sign_in_only_ax_admin_uses_is_revoked_when_it_lets_go() {
        let other = "https://ax.example/projects/other/mcp";
        let list = vec![entry_for(URL, &[USER_ADMIN]), entry_for(other, &[USER_AX])];
        assert_eq!(released(list.clone(), URL, USER_ADMIN), Release::Revoke);
        // Nothing stored for the address: nothing to revoke.
        assert!(matches!(
            released(list, "https://ax.example/projects/none/mcp", USER_ADMIN),
            Release::Keep(_)
        ));
    }
}
