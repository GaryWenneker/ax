use ax_types::SearchOptions;
use serde_json::json;

use crate::commands::resolve_path;
use crate::ui::{accent, dim, SpinnerGuard};

pub async fn run(
    text: String,
    _kind: Option<String>,
    limit: Option<u32>,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(None);
    let trace = ax_usage::ToolTrace::begin(
        Some(&root),
        "ax_search",
        &json!({ "query": text, "limit": limit }),
    );
    let _spinner = SpinnerGuard::new(format!("Searching for \"{}\"...", text), json);
    let ax = match ax_core::Ax::open(&root).await {
        Ok(ax) => ax,
        Err(e) => {
            trace.finish_err(&e.to_string());
            return Err(e.to_string());
        }
    };
    let results = match ax
        .search_nodes(
            &text,
            &SearchOptions {
                limit,
                ..Default::default()
            },
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            trace.finish_err(&e.to_string());
            return Err(e.to_string());
        }
    };
    drop(_spinner);
    let structured = serde_json::to_value(&results).unwrap_or(json!({}));
    if json {
        let out = serde_json::to_string_pretty(&results).unwrap_or_default();
        println!("{out}");
        trace.finish_ok(&out);
        crate::commands::stash_cli_graph("ax_search", &structured, &out);
    } else if results.is_empty() {
        let msg = "No matching symbols.";
        println!("{}", dim(msg));
        trace.finish_ok(msg);
        crate::commands::stash_cli_graph("ax_search", &structured, msg);
    } else {
        let mut lines = Vec::with_capacity(results.len());
        for r in &results {
            let line = format!(
                "{} {} {}:{}",
                accent(r.node.kind.as_str()),
                r.node.name,
                dim(&r.node.file_path),
                r.node.start_line
            );
            println!("{line}");
            lines.push(format!(
                "{} {} {}:{}",
                r.node.kind.as_str(),
                r.node.name,
                r.node.file_path,
                r.node.start_line
            ));
        }
        let text = lines.join("\n");
        trace.finish_ok(&text);
        crate::commands::stash_cli_graph("ax_search", &structured, &text);
    }
    Ok(())
}
