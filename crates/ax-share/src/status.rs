//! Sync status persistence.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths::share_status_path;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareSyncStatus {
    pub last_sync_at: Option<i64>,
    pub last_error: Option<String>,
    pub provider: Option<String>,
    pub rules_added: usize,
    pub skills_added: usize,
    pub rules_pending: usize,
    pub skills_pending: usize,
    pub memory_inserted: usize,
    pub memory_updated: usize,
    pub remote_files: usize,
}

pub fn load_status(project_root: &Path) -> ShareSyncStatus {
    let path = share_status_path(project_root);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_status(project_root: &Path, status: &ShareSyncStatus) -> Result<(), String> {
    let path = share_status_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(status).map_err(|e| e.to_string())? + "\n";
    std::fs::write(path, text.as_bytes()).map_err(|e| e.to_string())
}

pub fn clear_error(status: &mut ShareSyncStatus) {
    status.last_error = None;
}

pub fn now_secs() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
