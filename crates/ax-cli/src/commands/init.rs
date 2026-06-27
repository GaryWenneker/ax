use std::sync::Arc;

use ax_extraction::orchestrator::IndexOptions;
use ax_sync::git_hooks::install_git_sync_hooks;

use crate::commands::{check_unsafe_root, resolve_path};
use crate::ui::{finish_progress_bar, index_progress_bar, index_progress_callback, ok_line};

pub async fn run(path: Option<String>) -> Result<(), String> {
    let root = resolve_path(path);
    check_unsafe_root(&root)?;
    let mut ax = ax_core::Ax::init(&root).await.map_err(|e| e.to_string())?;

    let progress = index_progress_bar(false);
    let on_progress = progress
        .as_ref()
        .map(|pb| index_progress_callback(Arc::clone(pb)));
    let result = ax
        .index_all(IndexOptions::default(), on_progress)
        .await
        .map_err(|e| e.to_string())?;
    finish_progress_bar(progress);
    println!(
        "{}",
        ok_line(format!(
            "Initialized and indexed {} files in {}ms",
            result.files_indexed, result.duration_ms
        ))
    );

    install_git_sync_hooks(&root).map_err(|e| e.to_string())?;
    crate::installer::run_interactive_installer(&root)?;
    Ok(())
}
