//! Which binary file a daemon or proxy runs, so a proxy only attaches to its own build
//! and a daemon notices when its binary was replaced.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const EXE_CHECK_ENV: &str = "AX_DAEMON_EXE_CHECK_MS";
const DEFAULT_EXE_CHECK_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExeIdentity {
    pub path: String,
    pub len: u64,
    #[serde(rename = "mtimeMs")]
    pub mtime_ms: u64,
}

impl ExeIdentity {
    pub fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        let mtime_ms = meta
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_millis() as u64;
        Some(Self {
            path: path.to_string_lossy().into_owned(),
            len: meta.len(),
            mtime_ms,
        })
    }

    pub fn current() -> Option<Self> {
        Self::of(&current_exe_path()?)
    }

    /// The file at `path` is gone, or is no longer the file this identity was taken from.
    pub fn replaced_on_disk(&self) -> bool {
        Self::of(Path::new(&self.path)).as_ref() != Some(self)
    }
}

/// `current_exe`, minus the ` (deleted)` Linux appends once the file was replaced.
pub fn current_exe_path() -> Option<PathBuf> {
    std::env::current_exe().ok().map(|p| strip_deleted_suffix(&p))
}

fn strip_deleted_suffix(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_suffix(" (deleted)") {
        Some(stripped) => PathBuf::from(stripped),
        None => path.to_path_buf(),
    }
}

pub fn exe_check_interval_ms() -> u64 {
    interval_from(std::env::var(EXE_CHECK_ENV).ok().as_deref())
}

fn interval_from(value: Option<&str>) -> u64 {
    value
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(DEFAULT_EXE_CHECK_MS)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attach {
    /// Same binary, or no way to tell: use the running daemon.
    Same,
    /// The daemon runs a newer build than this proxy: use it, never restart it.
    Newer,
    /// This proxy's build is newer: restart the daemon on it first.
    RestartOnMine,
}

pub fn decide(
    mine: Option<&ExeIdentity>,
    my_version: &str,
    daemon: Option<&ExeIdentity>,
    daemon_version: &str,
) -> Attach {
    match (mine, daemon) {
        (Some(m), Some(d)) if m == d => Attach::Same,
        (Some(m), Some(d)) if m.mtime_ms > d.mtime_ms => Attach::RestartOnMine,
        (Some(_), Some(_)) => Attach::Newer,
        // A daemon from before identities existed is older than any proxy that has them.
        (_, None) if my_version != daemon_version => Attach::RestartOnMine,
        _ => Attach::Same,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(path: &str, len: u64, mtime_ms: u64) -> ExeIdentity {
        ExeIdentity { path: path.into(), len, mtime_ms }
    }

    #[test]
    fn e2_the_same_binary_attaches() {
        let a = id("/b/ax", 10, 1000);
        assert_eq!(decide(Some(&a), "5.1.0", Some(&a.clone()), "5.1.0"), Attach::Same);
    }

    #[test]
    fn e3_a_newer_proxy_restarts_an_older_daemon() {
        let old = id("/tmp/g/ax", 10, 1000);
        let new = id("/b/ax", 11, 2000);
        assert_eq!(decide(Some(&new), "5.1.0", Some(&old), "5.1.0"), Attach::RestartOnMine);
        assert_eq!(decide(Some(&new), "5.2.0", Some(&old), "5.1.0"), Attach::RestartOnMine);
    }

    #[test]
    fn e3_an_upgrade_in_place_restarts_the_daemon_still_on_the_old_file() {
        let running = id("/b/ax", 10, 1000);
        let upgraded = id("/b/ax", 12, 2000);
        assert_eq!(decide(Some(&upgraded), "5.2.0", Some(&running), "5.1.0"), Attach::RestartOnMine);
    }

    #[test]
    fn e5_the_check_runs_every_30_seconds_unless_configured() {
        assert_eq!(interval_from(None), 30_000);
        assert_eq!(interval_from(Some("junk")), 30_000);
        assert_eq!(interval_from(Some(" 200 ")), 200);
        assert_eq!(interval_from(Some("0")), 0);
    }

    #[test]
    fn e4_an_older_proxy_never_restarts_a_newer_daemon() {
        let old = id("/b/ax", 10, 1000);
        let new = id("/b2/ax", 11, 2000);
        assert_eq!(decide(Some(&old), "5.1.0", Some(&new), "5.1.0"), Attach::Newer);
        assert_eq!(decide(Some(&old), "5.1.0", Some(&new), "5.2.0"), Attach::Newer);
    }

    #[test]
    fn e4_equal_mtimes_on_different_files_do_not_restart() {
        let a = id("/a/ax", 10, 1000);
        let b = id("/b/ax", 10, 1000);
        assert_eq!(decide(Some(&a), "5.1.0", Some(&b), "5.1.0"), Attach::Newer);
    }

    #[test]
    fn e1_a_daemon_from_before_identities_attaches_on_the_same_version() {
        let mine = id("/b/ax", 10, 2000);
        assert_eq!(decide(Some(&mine), "5.1.0", None, "5.1.0"), Attach::Same);
        assert_eq!(decide(Some(&mine), "5.2.0", None, "5.1.0"), Attach::RestartOnMine);
    }

    #[test]
    fn e5_an_untouched_binary_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("ax");
        std::fs::write(&exe, b"one").unwrap();
        let identity = ExeIdentity::of(&exe).unwrap();
        assert_eq!(identity.len, 3);
        assert!(!identity.replaced_on_disk());
    }

    #[test]
    fn e5_a_rewritten_or_deleted_binary_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("ax");
        std::fs::write(&exe, b"one").unwrap();
        let identity = ExeIdentity::of(&exe).unwrap();
        std::fs::write(&exe, b"one-two").unwrap();
        assert!(identity.replaced_on_disk(), "size changed");

        let same_size = ExeIdentity::of(&exe).unwrap();
        let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options().write(true).open(&exe).unwrap().set_modified(later).unwrap();
        assert!(same_size.replaced_on_disk(), "mtime changed");

        let before_delete = ExeIdentity::of(&exe).unwrap();
        std::fs::remove_file(&exe).unwrap();
        assert!(before_delete.replaced_on_disk(), "deleted");
    }

    #[test]
    fn linux_deleted_suffix_is_stripped() {
        assert_eq!(
            strip_deleted_suffix(Path::new("/usr/bin/ax (deleted)")),
            PathBuf::from("/usr/bin/ax")
        );
        assert_eq!(strip_deleted_suffix(Path::new("/usr/bin/ax")), PathBuf::from("/usr/bin/ax"));
    }

    #[test]
    fn hello_without_identity_still_parses() {
        let hello: crate::daemon::DaemonHello = serde_json::from_str(
            r#"{"type":"hello","pid":1,"ax":"5.1.0","project":"/p","port":0}"#,
        )
        .unwrap();
        assert_eq!(hello.exe, None);
        let info: crate::daemon::DaemonInfo = serde_json::from_str(
            r#"{"pid":1,"port":0,"version":"5.1.0","project_root":"/p"}"#,
        )
        .unwrap();
        assert_eq!(info.exe, None);
    }
}
