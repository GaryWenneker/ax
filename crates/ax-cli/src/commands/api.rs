use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(module: String, limit: usize, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    ax_usage::log_cli(
        Some(&root),
        format!(
            "cmd=api start module_len={} limit={limit}",
            module.chars().count()
        ),
    );
    let _spinner = SpinnerGuard::new(format!("Listing API surface for \"{module}\"..."), false);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let nodes = ax
        .module_api(&module, limit)
        .await
        .map_err(|e| e.to_string())?;
    ax_usage::log_cli(Some(&root), format!("cmd=api ok count={}", nodes.len()));
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&nodes).unwrap_or_default()
        );
        return Ok(());
    }
    if nodes.is_empty() {
        println!("No exported symbols matching module '{module}'.");
        return Ok(());
    }
    println!("## API surface: {module} ({} symbols)\n", nodes.len());
    for n in &nodes {
        println!("- {} ({}) — {}", n.qualified_name, n.kind.as_str(), n.file_path);
    }
    Ok(())
}
