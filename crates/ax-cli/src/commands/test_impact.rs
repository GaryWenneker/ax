use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(base: String, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    let _spinner = SpinnerGuard::new("Test impact analysis...", false);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;

    let changed = ax_git::changed_files(&root, &base).unwrap_or_default();
    let pool = ax.db_pool();
    let opts = ax_tia::TiaOptions::default().with_depth(5);
    let result = ax_tia::affected_files_from_changes(pool, &changed, &opts)
        .await
        .map_err(|e| e.to_string())?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
    } else {
        println!("Changed files: {}", changed.len());
        println!("Impacted tests: {}", result.tests.len());
        for t in &result.tests {
            println!("  {} — {}", t.name, t.runner_hint);
        }
        if result.tests.is_empty() && !result.test_files.is_empty() {
            println!("\nTest files:");
            for f in &result.test_files {
                println!("  {f}");
            }
        }
    }
    Ok(())
}
