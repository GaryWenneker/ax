//! `ax cache` — context cache stash / expand / status for CLI agents.

use std::io::Read;

use serde_json::json;

use crate::commands::resolve_path;

#[derive(Debug, Clone)]
pub enum CacheAction {
    Stash { label: Option<String> },
    Expand { id: String, offset: usize, limit: Option<usize> },
    Status,
}

pub struct CacheArgs {
    pub action: CacheAction,
    pub session: Option<String>,
    pub path: Option<String>,
    pub text_stdin: bool,
    pub json: bool,
}

pub async fn run(args: CacheArgs) -> Result<(), String> {
    let root = resolve_path(args.path);
    match args.action {
        CacheAction::Stash { label } => {
            let text = if args.text_stdin {
                read_stdin()?
            } else {
                return Err("--text-stdin required for stash".into());
            };
            let receipt = ax_usage::stash_project_text(&root, label.as_deref(), &text).await?;
            let _ = ax_usage::record_activity(
                &root,
                ax_usage::ActivityRecord {
                    idempotency_key: format!("cli.cache.stash:{}", receipt.id),
                    source: Some(ax_usage::ActivitySource::Cli),
                    session_id: args.session.clone(),
                    operation: "cache.stash".into(),
                    status: Some(ax_usage::ActivityStatus::Success),
                    summary: format!("stashed {} tokens", receipt.original_tokens),
                    token_metrics: ax_usage::TokenMetrics {
                        input: None,
                        output: Some(receipt.original_tokens),
                        provenance: "measured".into(),
                    },
                    ..ax_usage::ActivityRecord::default()
                },
            )
            .await;
            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": receipt.id,
                        "originalTokens": receipt.original_tokens,
                    }))
                    .map_err(|e| e.to_string())?
                );
            } else {
                println!("id={} tokens={}", receipt.id, receipt.original_tokens);
            }
        }
        CacheAction::Expand { id, offset, limit } => {
            let page = ax_usage::expand_project_cached(&root, &id, offset, limit).await?;
            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "id": id,
                        "text": page.text,
                        "nextOffset": page.next_offset,
                    }))
                    .map_err(|e| e.to_string())?
                );
            } else {
                println!("{}", page.text);
            }
        }
        CacheAction::Status => {
            let status = ax_usage::load_cache_status(args.session.as_deref()).await;
            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "enabled": status.enabled,
                        "threshold": status.threshold,
                        "liveRows": status.live_rows,
                        "storedTokens": status.stored_tokens,
                        "expiredRows": status.expired_rows,
                        "sessionRows": status.session_rows,
                        "sessionStoredTokens": status.session_stored_tokens,
                        "contextError": status.context_error,
                    }))
                    .map_err(|e| e.to_string())?
                );
            } else {
                for line in ax_usage::format_cache_status_lines(args.session.as_deref(), &status) {
                    println!("{line}");
                }
            }
        }
    }
    Ok(())
}

fn read_stdin() -> Result<String, String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}
