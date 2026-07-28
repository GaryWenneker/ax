//! `ax pricing` — daily model price sync and history.

use ax_usage::{
    list_latest_prices, price_history, pricing_status, sync_pricing, usage_db_path,
};

pub async fn run_sync(force: bool) -> Result<(), String> {
    let report = sync_pricing(force).await?;
    if report.skipped {
        println!("Pricing sync skipped — already synced for {}.", report.date);
        println!("Use `ax pricing sync --force` to re-fetch.");
        return Ok(());
    }
    println!("Pricing sync {} for {}", report.status, report.date);
    println!("  OpenRouter: {} models", report.openrouter_count);
    println!("  Database: {}", usage_db_path().display());
    if !report.warnings.is_empty() {
        println!("  Warnings:");
        for w in &report.warnings {
            println!("    - {w}");
        }
    }
    Ok(())
}

pub async fn run_status(json: bool) -> Result<(), String> {
    let status = pricing_status().await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&status).unwrap_or_default());
        return Ok(());
    }
    println!("Model pricing status");
    println!("  Today: {}", status.today);
    println!(
        "  Synced today: {}",
        if status.synced_today { "yes" } else { "no" }
    );
    println!(
        "  Rows: {} OpenRouter prices",
        status.price_rows
    );
    println!("  Database: {}", usage_db_path().display());
    if status.sources.is_empty() {
        println!("  No sync yet — run `ax pricing sync`.");
    } else {
        println!("  Sources:");
        for s in &status.sources {
            if s.source != "openrouter" {
                continue;
            }
            println!(
                "    {:<22} status={} models={} date={}",
                s.source,
                s.status.as_deref().unwrap_or("-"),
                s.models_count.unwrap_or(0),
                s.last_success_date.as_deref().unwrap_or("-"),
            );
            if let Some(err) = &s.error {
                if !err.is_empty() {
                    println!("      error: {err}");
                }
            }
        }
    }
    Ok(())
}

pub async fn run_list(source: Option<String>, json: bool) -> Result<(), String> {
    let rows = list_latest_prices(source.as_deref()).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
        return Ok(());
    }
    if rows.is_empty() {
        println!("No price snapshots yet — run `ax pricing sync`.");
        return Ok(());
    }
    println!(
        "{:<12} {:<22} {:<36} {:>8} {:>8}",
        "DATE", "SOURCE", "MODEL", "IN $/M", "OUT $/M"
    );
    for r in rows.iter().take(80) {
        println!(
            "{:<12} {:<22} {:<36} {:>8.3} {:>8.3}",
            r.date,
            r.source,
            truncate(&r.model_id, 36),
            r.input_per_mtok,
            r.output_per_mtok
        );
    }
    if rows.len() > 80 {
        println!("… {} more (use --json)", rows.len() - 80);
    }
    Ok(())
}

pub async fn run_history(
    model: String,
    source: Option<String>,
    days: i64,
    json: bool,
) -> Result<(), String> {
    let rows = price_history(&model, source.as_deref(), days).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap_or_default());
        return Ok(());
    }
    if rows.is_empty() {
        println!("No history for `{model}` — run `ax pricing sync` first.");
        return Ok(());
    }
    println!(
        "{:<12} {:<22} {:>8} {:>8}",
        "DATE", "SOURCE", "IN $/M", "OUT $/M"
    );
    for r in &rows {
        println!(
            "{:<12} {:<22} {:>8.3} {:>8.3}",
            r.date, r.source, r.input_per_mtok, r.output_per_mtok
        );
    }
    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
