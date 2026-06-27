use ax_types::SearchOptions;

use crate::commands::resolve_path;

pub async fn run(name: Option<String>) -> Result<(), String> {
    let root = resolve_path(None);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let q = name.unwrap_or_default();
    let results = ax
        .search_nodes(&q, &SearchOptions { limit: Some(10), ..Default::default() })
        .await
        .map_err(|e| e.to_string())?;
    println!("{}", serde_json::to_string_pretty(&results).unwrap_or_default());
    Ok(())
}
