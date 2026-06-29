use ax_types::SearchOptions;

use crate::commands::resolve_path;

pub async fn run(text: String, _kind: Option<String>, limit: Option<u32>, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let results = ax
        .search_nodes(&text, &SearchOptions { limit, ..Default::default() })
        .await
        .map_err(|e| e.to_string())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap_or_default());
    } else {
        for r in results {
            println!("{} {} {}:{}", r.node.kind.as_str(), r.node.name, r.node.file_path, r.node.start_line);
        }
    }
    Ok(())
}
