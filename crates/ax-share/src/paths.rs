//! Paths for auth tokens, sync cache, and status.

use std::path::PathBuf;

pub fn ax_home() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(".ax"))
        .unwrap_or_else(|| PathBuf::from(".ax"))
}

pub fn auth_dir() -> PathBuf {
    ax_home().join("auth")
}

pub fn microsoft_auth_path() -> PathBuf {
    auth_dir().join("microsoft.json")
}

pub fn github_auth_path() -> PathBuf {
    auth_dir().join("github.json")
}

pub fn share_cache_dir() -> PathBuf {
    ax_home().join("share").join("cache")
}

pub fn share_status_path(project_root: &std::path::Path) -> PathBuf {
    project_root.join(".ax").join("share").join("status.json")
}

/// Legacy global status path (unused — status is per-project).
pub fn share_status_path_legacy() -> PathBuf {
    ax_home().join("share").join("status.json")
}

pub fn device_flow_path() -> PathBuf {
    auth_dir().join("microsoft_device.json")
}

pub fn ensure_auth_dir() -> Result<(), String> {
    std::fs::create_dir_all(auth_dir()).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(auth_dir()) {
            let mut perms = meta.permissions();
            perms.set_mode(0o700);
            let _ = std::fs::set_permissions(auth_dir(), perms);
        }
    }
    Ok(())
}
