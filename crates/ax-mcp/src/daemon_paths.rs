//! Daemon socket / named-pipe path helpers - CG: mcp/daemon-paths.ts.

use std::path::{Path, PathBuf};

use ax_context::directory::get_ax_dir;
use sha2::{Digest, Sha256};


pub const DAEMON_PID_FILE: &str = "daemon.pid";

pub fn daemon_pid_path(project_root: &Path) -> PathBuf {
    get_ax_dir(project_root).join(DAEMON_PID_FILE)
}
const POSIX_SOCKET_PATH_LIMIT: usize = 100;

pub fn project_hash(project_root: &Path) -> String {
    let resolved = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf());
    let mut hasher = Sha256::new();
    hasher.update(resolved.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    hex::encode(&digest[..8])
}

pub fn tmpdir_socket_path(project_root: &Path) -> PathBuf {
    let hash = project_hash(project_root);
    std::env::temp_dir().join(format!("ax-{hash}.sock"))
}

pub fn daemon_socket_candidates(project_root: &Path) -> Vec<String> {
    if cfg!(windows) {
        return vec![format!("\\\\.\\pipe\\ax-{}", project_hash(project_root))];
    }
    let in_project = get_ax_dir(project_root).join("daemon.sock");
    let in_project_str = in_project.to_string_lossy().replace('\\', "/");
    let tmp = tmpdir_socket_path(project_root).to_string_lossy().replace('\\', "/");
    if in_project_str.len() > POSIX_SOCKET_PATH_LIMIT {
        vec![tmp]
    } else {
        vec![in_project_str, tmp]
    }
}

pub fn primary_socket_path(project_root: &Path) -> String {
    daemon_socket_candidates(project_root).first().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable() {
        let a = project_hash(Path::new("C:/foo"));
        let b = project_hash(Path::new("C:/foo"));
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
    }
}