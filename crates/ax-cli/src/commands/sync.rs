use std::sync::Arc;

use ax_extraction::orchestrator::IndexOptions;

use crate::commands::resolve_path;
use crate::ui::{
    finish_progress_bar, format_duration_ms, index_progress_bar, index_progress_callback, info_line,
    ok_line,
};

pub async fn run(
    path: Option<String>,
    quiet: bool,
    watch: bool,
    all_members: bool,
) -> Result<(), String> {
    let root = resolve_path(path);

    if all_members {
        if watch {
            return Err("ax sync --all cannot be combined with --watch".into());
        }
        let members = ax_core::member_roots(&root);
        if members.len() == 1 && members[0] == root {
            // No workspace config — fall through to single-root sync.
        } else {
            for member in &members {
                if !quiet {
                    println!("{}", info_line(format!("Syncing {}", member.display())));
                }
                sync_one(member, quiet, false).await?;
            }
            if !quiet {
                println!(
                    "{}",
                    ok_line(format!("Synced {} workspace member(s)", members.len()))
                );
            }
            return Ok(());
        }
    }

    sync_one(&root, quiet, watch).await
}

async fn sync_one(root: &std::path::Path, quiet: bool, watch: bool) -> Result<(), String> {
    // Idempotent — adds capture-git to hooks when missing (post-init upgrade path).
    let _ = ax_sync::install_git_sync_hooks(root);
    let mut ax = ax_core::Ax::open(root).await.map_err(|e| e.to_string())?;
    let opts = IndexOptions {
        force: false,
        quiet,
        ..IndexOptions::default()
    };

    let progress = index_progress_bar(quiet);
    let on_progress = progress
        .as_ref()
        .map(|pb| index_progress_callback(Arc::clone(pb)));

    if watch {
        if !quiet {
            println!("{}", info_line("Watching for file changes (Ctrl+C to stop)..."));
        }
        let result = tokio::select! {
            res = ax.watch_and_sync(opts, on_progress) => res.map_err(|e| e.to_string()),
            _ = tokio::signal::ctrl_c() => {
                ax.unwatch().await;
                if !quiet {
                    println!("{}", ok_line("Stopped watching"));
                }
                Ok(())
            }
        };
        finish_progress_bar(progress);
        result
    } else {
        let result = ax.sync(opts, on_progress).await.map_err(|e| e.to_string())?;
        finish_progress_bar(progress);
        if !quiet {
            let summary = if result.files_indexed == 0 {
                ok_line("Already up to date")
            } else {
                ok_line(format!(
                    "Synced {} file(s) in {}",
                    result.files_indexed,
                    format_duration_ms(result.duration_ms)
                ))
            };
            println!("{}", summary);
        }
        Ok(())
    }
}
