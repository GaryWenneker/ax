use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(base: String, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    let _spinner = SpinnerGuard::new("Computing git diff...", false);
    let diff = ax_git::diff_vs_base(&root, &base).map_err(|e| e.to_string())?;

    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let pool = ax.db_pool();
    let dirty = if diff.hunks.is_empty() {
        ax_git::map_files_to_nodes(pool, &diff.files.iter().map(|f| f.path.clone()).collect::<Vec<_>>())
            .await
            .map_err(|e| e.to_string())?
    } else {
        ax_git::map_hunks_to_nodes(pool, &diff.hunks)
            .await
            .map_err(|e| e.to_string())?
    };

    let output = serde_json::json!({
        "context": diff.context,
        "files": diff.files,
        "hunks": diff.hunks,
        "dirty_nodes": dirty,
    });

    if json {
        println!("{}", serde_json::to_string_pretty(&output).unwrap_or_default());
    } else {
        println!(
            "Branch {:?} vs {} — {} files, {} symbols",
            diff.context.head_branch,
            diff.context.base_ref,
            diff.files.len(),
            dirty.len()
        );
        for node in &dirty {
            println!("  {} {} ({})", node.kind, node.name, node.file_path);
        }
    }
    Ok(())
}
