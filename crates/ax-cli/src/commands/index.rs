use std::sync::Arc;

use ax_extraction::orchestrator::IndexOptions;

use crate::commands::{check_unsafe_root, resolve_path};
use crate::ui::{
    finish_progress_bar, format_duration_ms, index_progress_bar, index_progress_callback, info_line,
    ok_line,
};

pub async fn run(
    path: Option<String>,
    force: bool,
    quiet: bool,
    _verbose: bool,
    all_members: bool,
) -> Result<(), String> {
    let root = resolve_path(path);

    if all_members {
        let members = ax_core::member_roots(&root);
        if members.len() > 1 || (members.len() == 1 && members[0] != root) {
            ax_usage::log_workspace(
                Some(&root),
                format!("index-all start members={}", members.len()),
            );
            for member in &members {
                if !quiet {
                    println!("{}", info_line(format!("Indexing {}", member.display())));
                }
                ax_usage::log_workspace(
                    Some(&root),
                    format!("index-all member={}", member.display()),
                );
                if let Err(e) = index_one(member, force, quiet).await {
                    ax_usage::log_workspace(
                        Some(&root),
                        format!("index-all fail member={}", member.display()),
                    );
                    return Err(e);
                }
            }
            ax_usage::log_workspace(
                Some(&root),
                format!("index-all ok members={}", members.len()),
            );
            if !quiet {
                println!(
                    "{}",
                    ok_line(format!("Indexed {} workspace member(s)", members.len()))
                );
            }
            return Ok(());
        }
    }

    index_one(&root, force, quiet).await
}

async fn index_one(root: &std::path::Path, force: bool, quiet: bool) -> Result<(), String> {
    check_unsafe_root(root)?;
    ax_usage::log_workspace(Some(root), "index start");
    let mut ax = match ax_core::Ax::open(root).await {
        Ok(ax) => ax,
        Err(e) => {
            ax_usage::log_workspace(Some(root), "index fail stage=open");
            return Err(e.to_string());
        }
    };
    if force {
        if let Err(e) = ax.clear().await {
            ax_usage::log_workspace(Some(root), "index fail stage=clear");
            return Err(e.to_string());
        }
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

    let result = match ax.index_all(opts, on_progress).await {
        Ok(r) => r,
        Err(e) => {
            finish_progress_bar(progress);
            ax_usage::log_workspace(Some(root), "index fail stage=index");
            return Err(e.to_string());
        }
    };

    finish_progress_bar(progress);

    ax_usage::log_workspace(
        Some(root),
        format!(
            "index ok files={} duration_ms={}",
            result.files_indexed, result.duration_ms
        ),
    );

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
