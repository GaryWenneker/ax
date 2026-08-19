use ax_types::SearchOptions;

use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(from: String, to: String, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    ax_usage::log_cli(
        Some(&root),
        format!(
            "cmd=path start from_len={} to_len={}",
            from.chars().count(),
            to.chars().count()
        ),
    );
    let _spinner = SpinnerGuard::new(format!("Finding path {from} → {to}..."), false);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;

    async fn resolve_sym(ax: &ax_core::Ax, sym: &str) -> Result<ax_types::Node, String> {
        let hits = ax
            .search_nodes(
                sym,
                &SearchOptions {
                    limit: Some(1),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| e.to_string())?;
        hits.into_iter()
            .next()
            .map(|h| h.node)
            .ok_or_else(|| format!("No symbol matching '{sym}'"))
    }

    let from_node = resolve_sym(&ax, &from).await?;
    let to_node = resolve_sym(&ax, &to).await?;
    let path = ax
        .find_path(&from_node.id, &to_node.id)
        .await
        .map_err(|e| e.to_string())?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "from": from_node.qualified_name,
                "to": to_node.qualified_name,
                "path": path,
            }))
            .unwrap_or_default()
        );
        return Ok(());
    }

    match path {
        Some(ids) if !ids.is_empty() => {
            // Resolve names for display.
            let mut names = Vec::new();
            for id in &ids {
                let name = ax
                    .get_node(id)
                    .await
                    .ok()
                    .flatten()
                    .map(|n| n.qualified_name)
                    .unwrap_or_else(|| id.clone());
                names.push(name);
            }
            ax_usage::log_cli(Some(&root), format!("cmd=path ok hops={}", names.len()));
            println!("{}", names.join(" → "));
        }
        _ => {
            ax_usage::log_cli(Some(&root), "cmd=path ok hops=0");
            println!(
                "No Calls/References path from '{}' to '{}'.",
                from_node.qualified_name, to_node.qualified_name
            );
        }
    }
    Ok(())
}
