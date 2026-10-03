//! `ax costs` — estimated agent spend from local usage events.

use std::path::Path;

use ax_usage::{collect_report, format_summary, SavingsQuery, UsagePeriod};

pub async fn run(
    action: &str,
    period: Option<String>,
    from: Option<String>,
    to: Option<String>,
    json: bool,
    project: Option<&Path>,
) -> Result<(), String> {
    let period = match action {
        "today" => UsagePeriod::Custom,
        "month" => UsagePeriod::MonthToDate,
        _ => period
            .as_deref()
            .and_then(UsagePeriod::parse)
            .unwrap_or(UsagePeriod::MonthToDate),
    };
    let (from, to) = if action == "today" {
        let today = ax_usage::today_iso();
        (Some(today.clone()), Some(today))
    } else {
        (from, to)
    };
    if period == UsagePeriod::Custom && from.is_none() {
        return Err("custom period requires --from YYYY-MM-DD".into());
    }
    let query = SavingsQuery { period, from, to };
    let (report, snapshot, settings) = collect_report(&query, project).await?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "events": report.events,
                "knownEvents": report.known_events,
                "unknownEvents": report.unknown_events,
                "spendUsd": report.spend_usd,
                "projectedUsd": snapshot.projected_spend_usd,
                "usagePercent": snapshot.usage_percent,
                "decision": format!("{:?}", snapshot.decision),
                "tokensAvoided": report.tokens_avoided,
                "group": action,
            })
        );
        return Ok(());
    }
    let label = match action {
        "today" => "Today".to_string(),
        "session" => "Sessions".to_string(),
        "model" => "Models".to_string(),
        "project" => "Projects".to_string(),
        _ => format!("{} to {}", query_label(&query), ""),
    };
    let mut text = format_summary(&report, &snapshot, &settings, label.trim());
    if action == "session" || action == "project" || action == "model" {
        text.push_str("\nBreakdown\n--------------------------------\n");
        let rows = if action == "project" {
            &report.by_project
        } else {
            &report.by_model
        };
        if action == "session" {
            text.push_str("Session totals are the known event costs in this period.\n");
        }
        for row in rows {
            text.push_str(&format!(
                "{}  {:.2} USD  unknown {}\n",
                row.label, row.usd, row.unknown_events
            ));
        }
    }
    println!("{text}");
    Ok(())
}

fn query_label(query: &SavingsQuery) -> String {
    match query.period {
        UsagePeriod::MonthToDate => "Month to date".into(),
        UsagePeriod::Month => "Month".into(),
        UsagePeriod::Week => "Week".into(),
        UsagePeriod::Year => "Year".into(),
        UsagePeriod::Custom => query.from.clone().unwrap_or_else(|| "Custom".into()),
    }
}
