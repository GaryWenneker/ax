use std::sync::Arc;

use ax_extraction::orchestrator::IndexOptions;
use ax_sync::git_hooks::install_git_sync_hooks;

use crate::commands::{check_unsafe_root, resolve_path};
use crate::installer::{run_installer, InstallOptions};
use crate::ui::install_log::tildify;
use crate::ui::{
    dim, finish_progress_bar, format_duration_ms, index_progress_bar, index_progress_callback, info_line,
    ok_line,
};

pub async fn run(path: Option<String>) -> Result<(), String> {
    let root = resolve_path(path);
    check_unsafe_root(&root)?;

    println!();
    println!(
        "{}",
        info_line(format!("Initializing ax in {}", tildify(&root)))
    );
    println!("  {}", dim("Large projects take several minutes — progress updates below."));
    println!();

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
            "Indexed {} files in {}",
            result.files_indexed,
            format_duration_ms(result.duration_ms)
        ))
    );

    install_git_sync_hooks(&root).map_err(|e| e.to_string())?;
    if let Ok(mut t) = ax_telemetry::telemetry().lock() {
        t.record_lifecycle(
            "index",
            serde_json::json!({
                "languages": [],
                "file_count_bucket": ax_telemetry::bucket_file_count(result.files_indexed),
                "duration_bucket": ax_telemetry::bucket_duration(result.duration_ms),
            }),
        );
        t.persist_sync();
        let _ = t.flush_now(ax_telemetry::DEFAULT_FLUSH_TIMEOUT_MS).await;
    }

    run_installer(
        &root,
        InstallOptions {
            yes: true,
            install_all: false,
        },
    )?;
    Ok(())
}
