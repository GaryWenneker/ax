use ax_context::format_explore_text;
use ax_reasoning::{maybe_synthesize_explore, ExploreOffloadMeta};
use ax_types::ExploreOptions;

use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run(query: Vec<String>, json: bool) -> Result<(), String> {
    let query_text = query.join(" ");
    let root = resolve_path(None);
    ax_usage::log_cli(
        Some(&root),
        format!("cmd=explore start q_len={}", query_text.chars().count()),
    );
    let _spinner = SpinnerGuard::new(format!("Exploring \"{}\"...", query_text), json);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let result = match ax
        .explore(&query_text, ExploreOptions::default())
        .await
    {
        Ok(r) => r,
        Err(e) => {
            ax_usage::log_cli(Some(&root), "cmd=explore fail");
            return Err(e.to_string());
        }
    };
    drop(_spinner);
    ax_usage::log_cli(Some(&root), "cmd=explore ok");
    let raw = format_explore_text(&result);
    let project = root
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string);
    let meta = Some(ExploreOffloadMeta {
        source: "cli_explore",
        project,
    });
    if json {
        let _ = maybe_synthesize_explore(&query_text, &raw, meta).await;
        println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
    } else {
        let out = maybe_synthesize_explore(&query_text, &raw, meta).await;
        println!("{}", out);
    }
    Ok(())
}