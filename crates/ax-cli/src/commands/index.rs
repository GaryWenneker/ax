use std::sync::Arc;

use ax_extraction::orchestrator::IndexOptions;

use crate::commands::{check_unsafe_root, resolve_path};
use crate::ui::{finish_progress_bar, format_duration_ms, index_progress_bar, index_progress_callback, ok_line};

pub async fn run(path: Option<String>, force: bool, quiet: bool, _verbose: bool) -> Result<(), String> {
    let root = resolve_path(path);
    check_unsafe_root(&root)?;
    let mut ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    if force {
        ax.clear().await.map_err(|e| e.to_string())?;
    }
    let opts = IndexOptions {
        force,
        quiet,
        ..IndexOptions::default()
    };

    let progress = index_progress_bar(quiet);
    let on_progress = progress
        .as_ref()
        .map(|pb| index_progress_callback(Arc::clone(pb)));

    let result = ax.index_all(opts, on_progress).await.map_err(|e| e.to_string())?;

    finish_progress_bar(progress);

    if !quiet {
        println!(
            "{}",
            ok_line(format!(
                "Indexed {} files in {}",
                result.files_indexed,
                format_duration_ms(result.duration_ms)
            ))
        );
    }
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
    Ok(())
}
