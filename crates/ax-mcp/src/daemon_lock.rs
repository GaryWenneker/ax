//! Daemon pid lockfile — CG: mcp/daemon.ts tryAcquireDaemonLock, daemon-paths.ts.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::daemon_paths::{daemon_pid_path, primary_socket_path};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonLockInfo {
    pub pid: u32,
    pub version: String,
    pub socket_path: String,
    pub started_at: u64,
}

pub enum AcquireResult {
    Acquired {
        pid_path: PathBuf,
        info: DaemonLockInfo,
    },
    Taken {
        existing: Option<DaemonLockInfo>,
        pid_path: PathBuf,
    },
}

pub fn read_lock_info(pid_path: &Path) -> Option<DaemonLockInfo> {
    let raw = fs::read_to_string(pid_path).ok()?;
    serde_json::from_str(raw.trim()).ok()
}

pub fn try_acquire_daemon_lock(project_root: &Path) -> std::io::Result<AcquireResult> {
    let pid_path = daemon_pid_path(project_root);
    clear_stale_daemon_lock(&pid_path, None);
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let info = DaemonLockInfo {
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        socket_path: primary_socket_path(project_root),
        started_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
    };
    let body = serde_json::to_string_pretty(&info)? + "\n";
    if publish_lock(&pid_path, body.as_bytes())? {
        Ok(AcquireResult::Acquired { pid_path, info })
    } else {
        let existing = read_lock_info(&pid_path);
        Ok(AcquireResult::Taken { existing, pid_path })
    }
}

/// A lock file that cannot be parsed is only treated as stale after this long:
/// before that it may belong to a daemon that is still starting.
const UNREADABLE_LOCK_GRACE: Duration = Duration::from_secs(5);

/// A temporary file next to `path`, unique per process and call.
fn temp_sibling(path: &Path) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "daemon.pid".into());
    let n = NEXT.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{}.{n}.tmp", std::process::id()))
}

/// Creates `pid_path` with `body` unless it exists. Returns false when it exists.
///
/// The body is written to a temporary file first and published with a hard
/// link, which fails when the target exists, so no other process ever sees
/// the lock without its contents.
fn publish_lock(pid_path: &Path, body: &[u8]) -> std::io::Result<bool> {
    let tmp = temp_sibling(pid_path);
    let staged = (|| {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(body)?;
        file.sync_all()
    })();
    if let Err(e) = staged {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    let linked = fs::hard_link(&tmp, pid_path);
    let _ = fs::remove_file(&tmp);
    match linked {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        // A file system without hard links: create the file, then fill it.
        // A reader that sees it empty keeps it for UNREADABLE_LOCK_GRACE.
        Err(_) => match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(pid_path)
        {
            Ok(mut file) => {
                file.write_all(body)?;
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(e) => Err(e),
        },
    }
}

/// True when `path` was modified less than [`UNREADABLE_LOCK_GRACE`] ago (or its
/// age cannot be told).
fn is_recent(path: &Path) -> bool {
    let Ok(modified) = fs::metadata(path).and_then(|m| m.modified()) else {
        return false;
    };
    match SystemTime::now().duration_since(modified) {
        Ok(age) => age < UNREADABLE_LOCK_GRACE,
        Err(_) => true,
    }
}

/// Records the socket path in this process's lock. A lock that belongs to
/// another process is left alone.
pub fn rewrite_lock_socket_path(pid_path: &Path, socket_path: &str) -> std::io::Result<()> {
    let Some(mut info) = read_lock_info(pid_path) else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "daemon lock is missing or unreadable",
        ));
    };
    if info.pid != std::process::id() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "daemon lock belongs to another process",
        ));
    }
    info.socket_path = socket_path.to_string();
    let body = serde_json::to_string_pretty(&info)? + "\n";
    ax_utils::atomic_write(pid_path, body.as_bytes())
}

pub fn clear_stale_daemon_lock(pid_path: &Path, expected_pid: Option<u32>) {
    if let Some(existing) = read_lock_info(pid_path) {
        if expected_pid.is_some_and(|p| p != existing.pid) {
            return;
        }
        if is_pid_alive(existing.pid) {
            return;
        }
    } else if let Some(pid) = expected_pid {
        if is_pid_alive(pid) {
            return;
        }
    } else if is_recent(pid_path) {
        return;
    }
    let _ = fs::remove_file(pid_path);
}

/// Removes the lock whatever process it belongs to (`ax daemon stop`).
pub fn release_daemon_lock(pid_path: &Path) {
    let _ = fs::remove_file(pid_path);
}

/// Removes the lock only when it belongs to `pid`.
pub fn release_daemon_lock_if_owned(pid_path: &Path, pid: u32) {
    if read_lock_info(pid_path).is_some_and(|info| info.pid == pid) {
        let _ = fs::remove_file(pid_path);
    }
}

pub fn is_pid_alive(pid: u32) -> bool {
    use sysinfo::{Pid, System};
    let mut sys = System::new();
    sys.refresh_processes(
        sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]),
        true,
    );
    sys.process(Pid::from_u32(pid)).is_some()
}

/// Force-stop a process by PID (Windows `taskkill` / Unix `kill -9`).
pub fn kill_pid(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/F"])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-9", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock_root() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join(".ax")).unwrap();
        (dir, root)
    }

    #[test]
    fn a_lock_still_being_written_is_not_cleared_as_stale() {
        let (_dir, root) = lock_root();
        let pid_path = daemon_pid_path(&root);
        fs::create_dir_all(pid_path.parent().unwrap()).unwrap();
        fs::write(&pid_path, "").unwrap();
        clear_stale_daemon_lock(&pid_path, None);
        assert!(pid_path.exists(), "a fresh empty lock was deleted as stale");
        match try_acquire_daemon_lock(&root).unwrap() {
            AcquireResult::Taken { existing: None, .. } => {}
            _ => panic!("a fresh empty lock must stay taken"),
        }
    }

    #[test]
    fn an_old_unreadable_lock_is_cleared() {
        let (_dir, root) = lock_root();
        let pid_path = daemon_pid_path(&root);
        fs::create_dir_all(pid_path.parent().unwrap()).unwrap();
        fs::write(&pid_path, "").unwrap();
        let old = SystemTime::now() - Duration::from_secs(60);
        fs::File::options()
            .write(true)
            .open(&pid_path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        match try_acquire_daemon_lock(&root).unwrap() {
            AcquireResult::Acquired { info, .. } => assert_eq!(info.pid, std::process::id()),
            AcquireResult::Taken { .. } => panic!("an old unreadable lock kept the daemon out"),
        }
        assert_eq!(read_lock_info(&pid_path).unwrap().pid, std::process::id());
    }

    #[test]
    fn a_lock_of_a_dead_pid_is_cleared_and_a_live_one_is_kept() {
        let (_dir, root) = lock_root();
        let pid_path = daemon_pid_path(&root);
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                vec!["/C", "exit"]
            } else {
                vec![]
            })
            .spawn()
            .unwrap();
        let dead = child.id();
        child.wait().unwrap();
        let write = |pid: u32| {
            let info = DaemonLockInfo {
                pid,
                version: "test".into(),
                socket_path: "sock".into(),
                started_at: 0,
            };
            fs::write(&pid_path, serde_json::to_string(&info).unwrap()).unwrap();
        };
        write(dead);
        clear_stale_daemon_lock(&pid_path, None);
        assert!(!pid_path.exists(), "the lock of a dead daemon was kept");
        write(std::process::id());
        clear_stale_daemon_lock(&pid_path, None);
        assert!(pid_path.exists(), "the lock of a live daemon was cleared");
    }

    #[test]
    fn release_and_rewrite_only_touch_this_process_lock() {
        let (_dir, root) = lock_root();
        let pid_path = daemon_pid_path(&root);
        let other = DaemonLockInfo {
            pid: 1,
            version: "test".into(),
            socket_path: "theirs".into(),
            started_at: 0,
        };
        fs::write(&pid_path, serde_json::to_string(&other).unwrap()).unwrap();
        assert!(rewrite_lock_socket_path(&pid_path, "mine").is_err());
        release_daemon_lock_if_owned(&pid_path, std::process::id());
        let kept = read_lock_info(&pid_path).expect("lock kept");
        assert_eq!((kept.pid, kept.socket_path.as_str()), (1, "theirs"));

        fs::remove_file(&pid_path).unwrap();
        let AcquireResult::Acquired { pid_path, .. } = try_acquire_daemon_lock(&root).unwrap()
        else {
            panic!("expected acquired");
        };
        rewrite_lock_socket_path(&pid_path, "mine").unwrap();
        assert_eq!(read_lock_info(&pid_path).unwrap().socket_path, "mine");
        release_daemon_lock_if_owned(&pid_path, std::process::id());
        assert!(!pid_path.exists());
        let leftovers: Vec<_> = fs::read_dir(pid_path.parent().unwrap())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temporary files left: {leftovers:?}");
    }

    #[test]
    fn concurrent_acquirers_get_exactly_one_lock() {
        for _ in 0..40 {
            let (_dir, root) = lock_root();
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(8));
            let threads: Vec<_> = (0..8)
                .map(|_| {
                    let root = root.clone();
                    let barrier = std::sync::Arc::clone(&barrier);
                    std::thread::spawn(move || {
                        barrier.wait();
                        match try_acquire_daemon_lock(&root).unwrap() {
                            AcquireResult::Acquired { .. } => (1, 0),
                            AcquireResult::Taken { existing, .. } => {
                                (0, usize::from(existing.is_none()))
                            }
                        }
                    })
                })
                .collect();
            let (acquired, unreadable) = threads
                .into_iter()
                .map(|t| t.join().unwrap())
                .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1));
            assert_eq!(acquired, 1, "daemons that acquired the same lock");
            assert_eq!(unreadable, 0, "a taken lock was seen without its contents");
        }
    }

    #[test]
    fn acquire_and_release_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("ax-lock-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join(".ax")).unwrap();
        let root = tmp.clone();
        let r = try_acquire_daemon_lock(&root).unwrap();
        match r {
            AcquireResult::Acquired { pid_path, .. } => {
                release_daemon_lock(&pid_path);
            }
            AcquireResult::Taken { .. } => panic!("expected acquired"),
        }
        let _ = fs::remove_dir_all(&tmp);
    }
}
