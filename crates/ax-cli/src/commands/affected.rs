use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(files: Vec<String>) -> Result<(), String> {
    let root = resolve_path(None);
    let _spinner = SpinnerGuard::new("Finding affected tests...", false);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let changed: Vec<String> = if files.is_empty() {
        ax.get_pending_files()
            .await
            .into_iter()
            .map(|p| p.path)
            .collect()
    } else {
        files
    };
    let affected = ax.get_affected_files(&changed).await.map_err(|e| e.to_string())?;
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({ "affected": affected })).unwrap_or_default());
    Ok(())
}