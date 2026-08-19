use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(limit: usize, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    ax_usage::log_cli(Some(&root), format!("cmd=cycles start limit={limit}"));
    let _spinner = SpinnerGuard::new("Finding call-graph cycles...", false);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let cycles = ax.find_cycles(limit).await.map_err(|e| e.to_string())?;
    ax_usage::log_cli(
        Some(&root),
        format!("cmd=cycles ok count={}", cycles.len()),
    );
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&cycles).unwrap_or_default()
        );
        return Ok(());
    }
    if cycles.is_empty() {
        println!("No call-graph cycles found.");
        return Ok(());
    }
    println!("## Call-graph cycles ({})\n", cycles.len());
    for (i, c) in cycles.iter().enumerate() {
        let mut names = Vec::with_capacity(c.nodes.len());
        for id in &c.nodes {
            let name = ax
                .get_node(id)
                .await
                .ok()
                .flatten()
                .map(|n| n.qualified_name)
                .unwrap_or_else(|| id.clone());
            names.push(name);
        }
        println!("{}. {}", i + 1, names.join(" → "));
    }
    Ok(())
}
