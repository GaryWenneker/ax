//! `ax savings` — estimated context-token savings from MCP graph queries.

use ax_usage::{import_agent_logs, query_savings_summary, SavingsQuery, UsagePeriod};

pub async fn run_summary(
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

    let summary = query_savings_summary(&SavingsQuery { period, from, to }).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&summary).unwrap_or_default());
        return Ok(());
    }

    println!("Context savings ({} → {})", summary.from, summary.to);
    println!("Database: {}", summary.db_path);
    println!();
    println!(
        "Tokens saved: {} ({} graph calls of {} MCP calls, {} file reads avoided)",
        format_num(summary.tokens_saved_est),
        format_num(summary.graph_calls),
        format_num(summary.mcp_calls),
        format_num(summary.counterfactual_files),
    );
    println!(
        "Cost saved: ${:.2} (input tokens at {} — ${}/M in)",
        summary.cost_saved_usd_est,
        summary.pricing.reference_model,
        summary.pricing.input_per_mtok,
    );
    println!(
        "Without ax (full files): ~{} tokens",
        format_num(summary.counterfactual_tokens_est),
    );
    println!(
        "With ax (graph response): ~{} tokens",
        format_num(summary.graph_response_tokens_est),
    );
    if summary.counterfactual_tokens_est > 0 {
        let pct = ((summary.counterfactual_tokens_est - summary.graph_response_tokens_est) as f64
            / summary.counterfactual_tokens_est as f64
            * 100.0)
            .round() as i64;
        println!("Net reduction: ~{}% fewer context tokens", pct.max(0));
    }
    println!(
        "All MCP response size: ~{} tokens",
        format_num(summary.response_tokens_est),
    );
    if summary.failed_calls > 0 {
        println!("Failed calls (excluded from savings): {}", format_num(summary.failed_calls));
    }
    if summary.assumptions.exact_tokenizer {
        println!(
            "Measurement: o200k BPE tokenizer ({} of {} counterfactual files measured exactly)",
            format_num(summary.counterfactual_exact_files),
            format_num(summary.counterfactual_files),
        );
    } else {
        println!(
            "Measurement: heuristic fallback ({} chars/token, {} tokens/line, {} tokens/file)",
            summary.assumptions.chars_per_token,
            summary.assumptions.tokens_per_line,
            summary.assumptions.avg_file_tokens,
        );
    }
    println!(
        "Pricing: {} (${}/M in, ${}/M out) — override in {}",
        summary.pricing.reference_model,
        summary.pricing.input_per_mtok,
        summary.pricing.output_per_mtok,
        summary.pricing.config_path,
    );

    if summary.by_tool.is_empty() {
        println!();
        println!("No MCP calls recorded in this period.");
        println!("Use ax with MCP enabled — savings are logged on each ax_explore / graph tool call.");
    } else {
        println!();
        println!("By tool:");
        for row in &summary.by_tool {
            println!(
                "  {:<16} {:>6} calls  {:>12} saved  {:>6} files",
                row.tool,
                format_num(row.calls),
                format_num(row.tokens_saved_est),
                format_num(row.counterfactual_files),
            );
        }
    }

    if !summary.agent_sessions.is_empty() {
        println!();
        println!("Imported agent sessions (sample):");
        for s in summary.agent_sessions.iter().take(10) {
            let tokens = s
                .session_input_tokens
                .map(format_num)
                .unwrap_or_else(|| "n/a".into());
            let cost = s
                .session_cost_usd_est
                .map(|c| format!(" ~${c:.2}"))
                .unwrap_or_default();
            let model = s.model.as_deref().unwrap_or("");
            println!(
                "  {} {} — read:{} grep:{} ax:{} (input tokens: {}{}) {}",
                s.agent,
                &s.session_id[..s.session_id.len().min(8)],
                s.read_calls,
                s.grep_calls,
                s.ax_calls,
                tokens,
                cost,
                model,
            );
        }
        println!("Run `ax savings import --all` to refresh from local agent logs.");
    }

    Ok(())
}

pub async fn run_import(claude: bool, cursor: bool, all: bool) -> Result<(), String> {
    let do_claude = all || claude;
    let do_cursor = all || cursor;
    if !do_claude && !do_cursor {
        return Err("specify --claude, --cursor, or --all".into());
    }
    let result = import_agent_logs(do_claude, do_cursor).await?;
    println!(
        "Imported {} Claude session(s), {} Cursor session(s) ({} skipped)",
        result.claude_sessions, result.cursor_sessions, result.skipped
    );
    Ok(())
}

fn format_num(n: i64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out.chars().rev().collect()
}
