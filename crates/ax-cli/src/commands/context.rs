use ax_types::{BuildContextOptions, TaskInput};

use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(task: String) -> Result<(), String> {
    let root = resolve_path(None);
    ax_usage::log_cli(
        Some(&root),
        format!("cmd=context start task_len={}", task.chars().count()),
    );
    let _spinner = SpinnerGuard::new("Building task context...", false);
    let mut ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let ctx = match ax
        .build_context(TaskInput::Text(task), BuildContextOptions::default())
        .await
    {
        Ok(c) => c,
        Err(e) => {
            ax_usage::log_cli(Some(&root), "cmd=context fail");
            return Err(e.to_string());
        }
    };
    ax_usage::log_cli(Some(&root), "cmd=context ok");
    let text = ax_context::format_context_as_markdown(&ctx);
    println!("{text}");
    crate::commands::stash_cli_graph("ax_context", &ctx, &text);
    Ok(())
}
