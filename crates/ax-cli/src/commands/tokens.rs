//! `ax tokens` — per-model LLM token usage from offload calls.

use ax_reasoning::offload_status;
use ax_usage::{query_summary, UsagePeriod, UsageQuery};

pub async fn run(
    period: Option<String>,
    from: Option<String>,
    to: Option<String>,
    json: bool,
) -> Result<(), String> {
    let period = period
        .as_deref()
        .and_then(UsagePeriod::parse)
        .unwrap_or(UsagePeriod::MonthToDate);

    if period == UsagePeriod::Custom && from.is_none() {
        return Err("custom period requires --from YYYY-MM-DD (optional --to)".into());
    }

    let summary = query_summary(&UsageQuery { period, from, to }).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap_or_default());
        return Ok(());
    }

    println!("Token usage ({} → {})", summary.from, summary.to);
    println!("Database: {}", summary.db_path);
    println!();
    println!(
        "Total: {} tokens ({} prompt + {} completion) across {} calls",
        format_num(summary.total_tokens),
        format_num(summary.prompt_tokens),
        format_num(summary.completion_tokens),
        summary.calls,
    );

    if summary.by_model.is_empty() {
        println!();
        println!("No token usage recorded in this period.");
        let status = offload_status();
        let enabled = status
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if enabled {
            let model = status
                .get("model")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let url = status.get("url").and_then(|v| v.as_str()).unwrap_or("?");
            println!("Offload is enabled ({model} @ {url}) but no calls were recorded yet.");
            println!("Run ax explore or use ax_explore MCP with offload active.");
        } else {
            println!("Offload is not configured — ax only records tokens from LLM offload calls.");
            println!("  ax offload set-endpoint https://api.openai.com/v1 --key-env OPENAI_API_KEY");
            println!("  Or set OPENAI_API_KEY, CEREBRAS_API_KEY, or GROQ_API_KEY in your environment.");
        }
        return Ok(());
    }

    println!();
    println!("{:<28} {:>8} {:>12} {:>12} {:>12}", "Model", "Calls", "Prompt", "Completion", "Total");
    println!("{}", "-".repeat(76));
    for row in &summary.by_model {
        println!(
            "{:<28} {:>8} {:>12} {:>12} {:>12}",
            truncate(&row.model, 28),
            row.calls,
            format_num(row.prompt_tokens),
            format_num(row.completion_tokens),
            format_num(row.total_tokens),
        );
    }
    Ok(())
}

fn format_num(n: i64) -> String {
    n.to_string()
        .as_bytes()
        .rchunks(3)
        .rev()
        .map(std::str::from_utf8)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
        .join(",")
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}
