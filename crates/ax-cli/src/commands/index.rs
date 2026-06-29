use std::sync::{Arc, Mutex};

use ax_extraction::orchestrator::IndexOptions;
use ax_types::{IndexPhase, IndexProgress};
use indicatif::{ProgressBar, ProgressStyle};

use crate::commands::{check_unsafe_root, resolve_path};

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
        custom_extensions: ax.config().extensions.clone(),
    };

    let progress = if quiet {
        None
    } else {
        let pb = ProgressBar::new(0);
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap()
                .progress_chars("##-"),
        );
        Some(Arc::new(Mutex::new(pb)))
    };

    let on_progress = progress.clone().map(|pb| {
        Box::new(move |p: IndexProgress| {
            let label = match p.phase {
                IndexPhase::Scanning => "scanning",
                IndexPhase::Extracting => "indexing",
                IndexPhase::Resolving => "resolving",
            };
            if let Ok(guard) = pb.lock() {
                if p.total > 0 {
                    guard.set_length(p.total as u64);
                }
                guard.set_position(p.current as u64);
                let msg = p
                    .file_path
                    .as_deref()
                    .map(|f| format!("{} {}", label, f))
                    .unwrap_or_else(|| label.to_string());
                guard.set_message(msg);
            }
        }) as Box<dyn FnMut(IndexProgress) + Send>
    });

    let result = ax.index_all(opts, on_progress).await.map_err(|e| e.to_string())?;

    if let Some(pb) = progress {
        if let Ok(guard) = pb.lock() {
            guard.finish_and_clear();
        }
    }

    if !quiet {
        println!("Indexed {} files in {}ms", result.files_indexed, result.duration_ms);
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
