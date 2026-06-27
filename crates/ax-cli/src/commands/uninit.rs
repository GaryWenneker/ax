use ax_context::directory::get_ax_dir;
use ax_sync::git_hooks::remove_git_sync_hooks;

use crate::commands::resolve_path;

pub async fn run(path: Option<String>) -> Result<(), String> {
    let root = resolve_path(path);
    let ax_dir = get_ax_dir(&root);
    if ax_dir.exists() {
        std::fs::remove_dir_all(&ax_dir).map_err(|e| e.to_string())?;
    }
    remove_git_sync_hooks(&root).map_err(|e| e.to_string())?;
    Ok(())
}
