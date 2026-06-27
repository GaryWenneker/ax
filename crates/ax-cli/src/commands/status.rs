use crate::commands::resolve_path;
use crate::ui::{bold, kv_line, SpinnerGuard};

pub async fn run(path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let _spinner = SpinnerGuard::new("Loading index stats...", json);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let stats = ax.get_stats().await.map_err(|e| e.to_string())?;
    let last = ax.get_last_indexed_at().await.map_err(|e| e.to_string())?;
    drop(_spinner);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "stats": stats, "lastIndexedAt": last }))
                .unwrap_or_default()
        );
    } else {
        println!("{}", bold("Index status"));
        println!("{}", kv_line("Nodes", stats.node_count.to_string()));
        println!("{}", kv_line("Edges", stats.edge_count.to_string()));
        println!("{}", kv_line("Files", stats.file_count.to_string()));
        if let Some(n) = stats.unresolved_ref_count {
            println!("{}", kv_line("Unresolved refs", n.to_string()));
        }
        if stats.resolution_resolved.is_some() || stats.resolution_unresolved.is_some() {
            println!(
                "{}",
                kv_line(
                    "Last resolution",
                    format!(
                        "{} resolved, {} unresolved (of {})",
                        stats.resolution_resolved.unwrap_or(0),
                        stats.resolution_unresolved.unwrap_or(0),
                        stats.resolution_total.unwrap_or(0)
                    )
                )
            );
        }
        println!("{}", kv_line("Last indexed", last.to_string()));
    }
    Ok(())
}
