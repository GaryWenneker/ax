use ax_context::format_explore_text;
use ax_reasoning::maybe_synthesize_explore;
use ax_types::ExploreOptions;

use crate::commands::resolve_path;

pub async fn run(query: Vec<String>, json: bool) -> Result<(), String> {
    let query_text = query.join(" ");
    let root = resolve_path(None);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let result = ax
        .explore(&query_text, ExploreOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
    } else {
        let raw = format_explore_text(&result);
        let out = maybe_synthesize_explore(&query_text, &raw).await;
        println!("{}", out);
    }
    Ok(())
}