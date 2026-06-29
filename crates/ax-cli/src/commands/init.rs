use ax_extraction::orchestrator::IndexOptions;
use ax_sync::git_hooks::install_git_sync_hooks;

use crate::commands::{check_unsafe_root, resolve_path};

pub async fn run(path: Option<String>) -> Result<(), String> {
    let root = resolve_path(path);
    check_unsafe_root(&root)?;
    let mut ax = ax_core::Ax::init(&root).await.map_err(|e| e.to_string())?;
    ax.index_all(IndexOptions::default(), None).await.map_err(|e| e.to_string())?;
    install_git_sync_hooks(&root).map_err(|e| e.to_string())?;
    crate::installer::run_interactive_installer(&root)?;
    Ok(())
}
