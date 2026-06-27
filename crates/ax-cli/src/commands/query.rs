use ax_types::SearchOptions;

use crate::commands::resolve_path;
use crate::ui::{accent, dim, SpinnerGuard};

pub async fn run(text: String, _kind: Option<String>, limit: Option<u32>, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    let _spinner = SpinnerGuard::new(format!("Searching for \"{}\"...", text), json);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let results = ax
        .search_nodes(&text, &SearchOptions { limit, ..Default::default() })
        .await
        .map_err(|e| e.to_string())?;
    drop(_spinner);
    if json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap_or_default());
    } else {
        if results.is_empty() {
            println!("{}", dim("No matching symbols."));
            return Ok(());
        }
        for r in results {
            println!(
                "{} {} {}:{}",
                accent(r.node.kind.as_str()),
                r.node.name,
                dim(&r.node.file_path),
                r.node.start_line
            );
        }
    }
    Ok(())
}
