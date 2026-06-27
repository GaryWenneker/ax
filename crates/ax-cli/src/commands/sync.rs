use ax_extraction::orchestrator::IndexOptions;

use crate::commands::resolve_path;
use crate::ui::{info_line, ok_line};

pub async fn run(path: Option<String>, quiet: bool, watch: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let mut ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let opts = IndexOptions {
        force: false,
        quiet,
        custom_extensions: ax.config().extensions.clone(),
    };

    if watch {
        if !quiet {
            println!("{}", info_line("Watching for file changes (Ctrl+C to stop)..."));
        }
        tokio::select! {
            res = ax.watch_and_sync(opts) => res.map_err(|e| e.to_string()),
            _ = tokio::signal::ctrl_c() => {
                ax.unwatch().await;
                if !quiet {
                    println!("{}", ok_line("Stopped watching"));
                }
                Ok(())
            }
        }
    } else {
        let result = ax.sync(opts).await.map_err(|e| e.to_string())?;
        if !quiet {
            println!(
                "{}",
                ok_line(format!(
                    "Synced {} files in {}ms",
                    result.files_indexed, result.duration_ms
                ))
            );
        }
        Ok(())
    }
}
