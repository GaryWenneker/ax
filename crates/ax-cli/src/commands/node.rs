use ax_types::SearchOptions;
use serde_json::json;

use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(name: Option<String>) -> Result<(), String> {
    let root = resolve_path(None);
    let label = name.clone().unwrap_or_else(|| "symbol".into());
    let trace = ax_usage::ToolTrace::begin(
        Some(&root),
        "ax_node",
        &json!({ "symbol": label }),
    );
    let _spinner = SpinnerGuard::new(format!("Looking up \"{}\"...", label), false);
    let ax = match ax_core::Ax::open(&root).await {
        Ok(ax) => ax,
        Err(e) => {
            trace.finish_err(&e.to_string());
            return Err(e.to_string());
        }
    };
    let q = name.unwrap_or_default();
    let results = match ax
        .search_nodes(
            &q,
            &SearchOptions {
                limit: Some(10),
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
    let out = serde_json::to_string_pretty(&results).unwrap_or_default();
    println!("{out}");
    trace.finish_ok(&out);
    crate::commands::stash_cli_graph("ax_node", &results, &out);
    Ok(())
}
