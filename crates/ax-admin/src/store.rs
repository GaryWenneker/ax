//! Saved connections. The file holds names and addresses only; keys and tokens
//! live in the OS credential store (see `secrets`).
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthKind {
    ApiKey,
    Oauth,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnection {
    pub id: String,
    pub name: String,
    pub url: String,
    pub auth: AuthKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default)]
    pub write: bool,
}

#[derive(Clone, Default, Serialize, Deserialize)]
struct Data {
    #[serde(default)]
    active: Option<String>,
    #[serde(default)]
    connections: Vec<SavedConnection>,
}

pub struct Store {
    path: PathBuf,
    data: Data,
}

/// `AX_ADMIN_HOME`, or `~/.ax-admin`.
pub fn home_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("AX_ADMIN_HOME").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(dir));
    }
    ax_utils::paths::home_dir()
        .map(|home| home.join(".ax-admin"))
        .ok_or_else(|| "Home directory unavailable".into())
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// A project MCP address: `https://host/projects/<id>/mcp`. Plain HTTP is
/// accepted for a loopback host only.
pub fn project_url(value: &str) -> Result<Url, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Enter the address of the Ax environment".into());
    }
    if value.len() > 2048 {
        return Err("The address is too long".into());
    }
    let url = Url::parse(value).map_err(|_| "The address is not a valid URL".to_string())?;
    if url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Use an address without a user name, password, query or fragment".into());
    }
    match url.scheme() {
        "https" => {}
        "http" if is_loopback(&url) => {}
        _ => {
            return Err(
                "Use an https:// address (http:// is accepted for this computer only)".into(),
            )
        }
    }
    let mut parts = url.path().split('/');
    let valid = parts.next() == Some("")
        && parts.next() == Some("projects")
        && parts.next().is_some_and(|id| {
            !id.is_empty()
                && id.len() <= 64
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        })
        && parts.next() == Some("mcp")
        && parts.next().is_none();
    if !valid {
        return Err("Use the project MCP address, ending in /projects/<project>/mcp".into());
    }
    Ok(url)
}

/// "host / project", used when the user gives no name.
pub fn default_name(url: &Url) -> String {
    let project = url.path().split('/').nth(2).unwrap_or("");
    format!("{} / {project}", url.host_str().unwrap_or(""))
}

impl Data {
    fn repair_active(&mut self) {
        let known = |id: &String| self.connections.iter().any(|c| &c.id == id);
        if !self.active.as_ref().is_some_and(known) {
            self.active = self.connections.first().map(|c| c.id.clone());
        }
    }
}

/// The file's content: a missing or empty file is an empty store; anything
/// else that does not parse is an error, so it is never overwritten.
fn parse(path: &Path, bytes: Option<&[u8]>) -> Result<Data, String> {
    let mut data: Data = match bytes {
        None => Data::default(),
        Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => Data::default(),
        Some(bytes) => serde_json::from_slice(bytes)
            .map_err(|_| format!("{} is not valid; fix or remove it", path.display()))?,
    };
    data.repair_active();
    Ok(data)
}

impl Store {
    /// Opens `<dir>/connections.json`. A missing file is an empty store; an
    /// unreadable one is an error, so it is never overwritten by accident.
    pub fn open(dir: &Path) -> Result<Self, String> {
        let path = dir.join("connections.json");
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => Some(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("Cannot read {}: {e}", path.display())),
        };
        let data = parse(&path, bytes.as_deref())?;
        Ok(Self { path, data })
    }

    pub fn list(&self) -> &[SavedConnection] {
        &self.data.connections
    }

    pub fn active_id(&self) -> Option<&str> {
        self.data.active.as_deref()
    }

    pub fn active(&self) -> Option<&SavedConnection> {
        self.data.active.as_deref().and_then(|id| self.get(id))
    }

    pub fn get(&self, id: &str) -> Option<&SavedConnection> {
        self.data.connections.iter().find(|c| c.id == id)
    }

    /// Adds or replaces a connection and writes the file.
    pub fn upsert(&mut self, connection: SavedConnection, activate: bool) -> Result<(), String> {
        self.change(|data| {
            let id = connection.id.clone();
            match data.connections.iter_mut().find(|c| c.id == id) {
                Some(existing) => *existing = connection,
                None => data.connections.push(connection),
            }
            if activate || data.active.is_none() {
                data.active = Some(id);
            }
            Ok(())
        })
    }

    /// Changes a saved connection in place and writes the file. Returns the
    /// changed entry, or `None` when no connection has this id (it may have
    /// been removed meanwhile); nothing is added then.
    pub fn update(
        &mut self,
        id: &str,
        edit: impl FnOnce(&mut SavedConnection),
    ) -> Result<Option<SavedConnection>, String> {
        self.change(|data| {
            let Some(existing) = data.connections.iter_mut().find(|c| c.id == id) else {
                return Ok(None);
            };
            edit(existing);
            existing.id = id.to_owned();
            Ok(Some(existing.clone()))
        })
    }

    pub fn activate(&mut self, id: &str) -> Result<(), String> {
        self.change(|data| {
            if !data.connections.iter().any(|c| c.id == id) {
                return Err("Unknown connection".into());
            }
            data.active = Some(id.to_owned());
            Ok(())
        })
    }

    /// Removes a connection. When it was the active one, another becomes active.
    pub fn remove(&mut self, id: &str) -> Result<Option<SavedConnection>, String> {
        self.change(|data| {
            let Some(index) = data.connections.iter().position(|c| c.id == id) else {
                return Ok(None);
            };
            Ok(Some(data.connections.remove(index)))
        })
    }

    /// Applies `op` to the file as it is now, under an exclusive lock, so a
    /// second ax-admin process on the same home directory never has its
    /// changes overwritten. Memory changes only after the file was written,
    /// so what the application shows is always what a restart would load.
    fn change<T>(&mut self, op: impl FnOnce(&mut Data) -> Result<T, String>) -> Result<T, String> {
        let path = self.path.clone();
        let mut outcome: Option<Result<(T, Data), String>> = None;
        let written = ax_utils::update_locked(&path, |current| {
            let step = parse(&path, current.as_deref()).and_then(|mut data| {
                let value = op(&mut data)?;
                data.repair_active();
                let bytes = serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?;
                Ok((value, data, bytes))
            });
            match step {
                Ok((value, data, bytes)) => {
                    #[cfg(unix)]
                    if current.is_none() {
                        // A new file is private from the start: the rename
                        // keeps the permissions of the file it replaces.
                        use std::os::unix::fs::OpenOptionsExt;
                        std::fs::OpenOptions::new()
                            .write(true)
                            .create(true)
                            .truncate(false)
                            .mode(0o600)
                            .open(&path)?;
                    }
                    outcome = Some(Ok((value, data)));
                    Ok(Some(bytes))
                }
                Err(e) => {
                    outcome = Some(Err(e));
                    Ok(None)
                }
            }
        });
        if let Err(e) = written {
            return Err(format!("Cannot update configuration: {e}"));
        }
        let (value, data) = outcome.ok_or("Configuration task failed")??;
        self.data = data;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_url_accepts_https_and_loopback_http_only() {
        assert!(project_url("https://ax.example.com/projects/my-app/mcp").is_ok());
        assert!(project_url("  https://ax.example.com:8443/projects/a_b/mcp ").is_ok());
        assert!(project_url("http://127.0.0.1:9000/projects/alpha/mcp").is_ok());
        assert!(project_url("http://localhost:9000/projects/alpha/mcp").is_ok());
        assert!(project_url("http://[::1]:9000/projects/alpha/mcp").is_ok());
        for bad in [
            "",
            "ax.example.com/projects/a/mcp",
            "http://ax.example.com/projects/a/mcp",
            "http://127.0.0.1.example.com/projects/a/mcp",
            "ftp://ax.example.com/projects/a/mcp",
            "https://user:pw@ax.example.com/projects/a/mcp",
            "https://user@ax.example.com/projects/a/mcp",
            "https://ax.example.com/projects/a/mcp?x=1",
            "https://ax.example.com/projects/a/mcp#x",
            "https://ax.example.com/",
            "https://ax.example.com/projects/a",
            "https://ax.example.com/projects//mcp",
            "https://ax.example.com/projects/a b/mcp",
            "https://ax.example.com/projects/a/mcp/extra",
            "https://ax.example.com/other/a/mcp",
        ] {
            assert!(project_url(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn default_name_is_host_and_project() {
        let url = project_url("https://ax.example.com/projects/my-app/mcp").unwrap();
        assert_eq!(default_name(&url), "ax.example.com / my-app");
    }

    fn connection(id: &str) -> SavedConnection {
        SavedConnection {
            id: id.into(),
            name: id.into(),
            url: format!("https://ax.example.com/projects/{id}/mcp"),
            auth: AuthKind::ApiKey,
            client_id: None,
            write: false,
        }
    }

    #[test]
    fn store_round_trips_and_keeps_an_active_connection() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        assert!(store.active().is_none());
        assert!(!dir.path().join("connections.json").exists());
        store.upsert(connection("one"), false).unwrap();
        store.upsert(connection("two"), false).unwrap();
        assert_eq!(store.active_id(), Some("one"));
        store.activate("two").unwrap();
        assert!(store.activate("missing").is_err());
        let reopened = Store::open(dir.path()).unwrap();
        assert_eq!(reopened.list().len(), 2);
        assert_eq!(reopened.active_id(), Some("two"));
        let mut store = reopened;
        assert_eq!(store.remove("two").unwrap().unwrap().id, "two");
        assert_eq!(store.active_id(), Some("one"));
        assert!(store.remove("two").unwrap().is_none());
        store.remove("one").unwrap();
        assert!(store.active().is_none());
        assert!(Store::open(dir.path()).unwrap().list().is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn configuration_file_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        store.upsert(connection("one"), true).unwrap();
        let mode = std::fs::metadata(dir.path().join("connections.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    /// A directory where the lock file belongs makes every save fail, for
    /// root too (file permissions do not stop root).
    fn break_saves(dir: &Path) -> PathBuf {
        let lock = dir.join("connections.json.lock");
        let _ = std::fs::remove_file(&lock);
        std::fs::create_dir(&lock).unwrap();
        lock
    }

    #[test]
    fn a_failed_save_changes_nothing_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        store.upsert(connection("one"), true).unwrap();
        let lock = break_saves(dir.path());
        assert!(store.upsert(connection("two"), true).is_err());
        assert!(store.remove("one").is_err());
        assert!(store
            .upsert(
                SavedConnection {
                    name: "renamed".into(),
                    ..connection("one")
                },
                false
            )
            .is_err());
        assert!(store.update("one", |c| c.name = "x".into()).is_err());
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.list()[0].name, "one");
        assert_eq!(store.active_id(), Some("one"));
        std::fs::remove_dir(lock).unwrap();
        assert_eq!(Store::open(dir.path()).unwrap().list().len(), 1);
    }

    #[test]
    fn update_changes_an_existing_entry_and_never_adds_one() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        store.upsert(connection("one"), true).unwrap();
        let updated = store
            .update("one", |c| c.name = "renamed".into())
            .unwrap()
            .unwrap();
        assert_eq!(updated.name, "renamed");
        assert_eq!(Store::open(dir.path()).unwrap().list()[0].name, "renamed");
        assert!(store
            .update("gone", |c| c.name = "x".into())
            .unwrap()
            .is_none());
        assert_eq!(store.list().len(), 1);
    }

    /// Two ax-admin processes on one home directory: each change is made to
    /// the file as it is now, so neither drops what the other saved.
    #[test]
    fn two_stores_on_one_file_keep_each_others_changes() {
        let dir = tempfile::tempdir().unwrap();
        let mut first = Store::open(dir.path()).unwrap();
        let mut second = Store::open(dir.path()).unwrap();
        first.upsert(connection("one"), true).unwrap();
        second.upsert(connection("two"), false).unwrap();
        assert_eq!(
            second.list().len(),
            2,
            "the second store sees the first's entry"
        );
        first
            .update("one", |c| c.name = "renamed".into())
            .unwrap()
            .unwrap();
        // An entry the other process removed is not brought back.
        second.remove("two").unwrap();
        assert!(first
            .update("two", |c| c.name = "x".into())
            .unwrap()
            .is_none());
        first.activate("one").unwrap();
        assert!(first.activate("two").is_err());
        let names: Vec<_> = Store::open(dir.path())
            .unwrap()
            .list()
            .iter()
            .map(|c| c.name.clone())
            .collect();
        assert_eq!(names, ["renamed"]);
    }

    #[test]
    fn a_damaged_file_is_reported_and_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("connections.json");
        let mut store = Store::open(dir.path()).unwrap();
        std::fs::write(&path, "{not json").unwrap();
        assert!(Store::open(dir.path()).is_err());
        // Damaged by someone else after it was opened: a change is refused.
        assert!(store.upsert(connection("one"), true).is_err());
        assert!(store.list().is_empty());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{not json");
    }
}
