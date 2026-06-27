use ax_types::{BuildContextOptions, TaskInput};

use crate::commands::resolve_path;

pub async fn run(task: String) -> Result<(), String> {
    let root = resolve_path(None);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let ctx = ax
        .build_context(TaskInput::Text(task), BuildContextOptions::default())
        .await
        .map_err(|e| e.to_string())?;
    println!("{}", ax_context::format_context_as_markdown(&ctx));
    Ok(())
}
