use ax_types::SearchOptions;

use crate::commands::resolve_path;

pub async fn run(symbol: String) -> Result<(), String> {
    let root = resolve_path(None);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let nodes = ax
        .search_nodes(&symbol, &SearchOptions { limit: Some(1), ..Default::default() })
        .await
        .map_err(|e| e.to_string())?;
    if let Some(n) = nodes.first() {
        let callers = ax.get_callers(&n.node.id, 3).await.map_err(|e| e.to_string())?;
        println!("{}", serde_json::to_string_pretty(&callers).unwrap_or_default());
    }
    Ok(())
}
