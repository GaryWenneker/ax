//! Daemon pid lockfile — CG: mcp/daemon.ts tryAcquireDaemonLock, daemon-paths.ts.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

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
    match OpenOptions::new().write(true).create_new(true).open(&pid_path) {
        Ok(mut file) => {
            file.write_all(body.as_bytes())?;
            Ok(AcquireResult::Acquired {
                pid_path,
                info,
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let existing = read_lock_info(&pid_path);
            Ok(AcquireResult::Taken {
                existing,
                pid_path,
            })
        }
        Err(e) => Err(e),
    }
}

pub fn rewrite_lock_socket_path(pid_path: &Path, socket_path: &str) -> std::io::Result<()> {
    let mut info = read_lock_info(pid_path).unwrap_or_else(|| DaemonLockInfo {
        pid: std::process::id(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        socket_path: socket_path.to_string(),
        started_at: 0,
    });
    info.socket_path = socket_path.to_string();
    let body = serde_json::to_string_pretty(&info)? + "\n";
    fs::write(pid_path, body)
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
    }
    let _ = fs::remove_file(pid_path);
}

pub fn release_daemon_lock(pid_path: &Path) {
    let _ = fs::remove_file(pid_path);
}

pub fn is_pid_alive(pid: u32) -> bool {
  use sysinfo::{Pid, System};
  let mut sys = System::new();
  sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
  sys.process(Pid::from_u32(pid)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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