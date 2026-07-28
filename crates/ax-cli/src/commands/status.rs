use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    ax_usage::log_cli(Some(&root), "cmd=status start");
    let _spinner = SpinnerGuard::new("Loading index stats...", json);
    let ax = match ax_core::Ax::open(&root).await {
        Ok(ax) => ax,
        Err(e) => {
            ax_usage::log_cli(Some(&root), "cmd=status fail stage=open");
            return Err(e.to_string());
        }
    };
    let stats = ax.get_stats().await.map_err(|e| e.to_string())?;
    let last = ax.get_last_indexed_at().await.map_err(|e| e.to_string())?;
    let pending = ax.get_pending_files().await;
    drop(_spinner);
    ax_usage::log_cli(
        Some(&root),
        format!(
            "cmd=status ok nodes={} pending={}",
            stats.node_count,
            pending.len()
        ),
    );
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "stats": stats, "lastIndexedAt": last, "pendingFiles": pending }))
                .unwrap_or_default()
        );
    } else {
        print!("{}", ax_core::stats_format::format_status_text(&stats, last, &pending));
    }
    Ok(())
}
