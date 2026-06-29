use crate::commands::resolve_path;

pub async fn run(_format: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(None);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let files = ax.queries().get_all_files().await.map_err(|e| e.to_string())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&files).unwrap_or_default());
    } else {
        for f in files {
            println!("{} ({})", f.path, f.language.as_str());
        }
    }
    Ok(())
}
