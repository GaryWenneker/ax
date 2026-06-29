use crate::commands::resolve_path;

pub async fn run(path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let stats = ax.get_stats().await.map_err(|e| e.to_string())?;
    let last = ax.get_last_indexed_at().await.map_err(|e| e.to_string())?;
  if json {
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "stats": stats, "lastIndexedAt": last })).unwrap_or_default());
    } else {
        println!("Nodes: {}  Edges: {}  Files: {}", stats.node_count, stats.edge_count, stats.file_count);
        if let Some(n) = stats.unresolved_ref_count {
            println!("Unresolved refs: {}", n);
        }
        if stats.resolution_resolved.is_some() || stats.resolution_unresolved.is_some() {
            println!(
                "Last resolution: {} resolved, {} unresolved (of {})",
                stats.resolution_resolved.unwrap_or(0),
                stats.resolution_unresolved.unwrap_or(0),
                stats.resolution_total.unwrap_or(0)
            );
        }
        println!("Last indexed: {}", last);
    }
    Ok(())
}
