//! `ax costs` — estimated agent spend from local usage events.

use std::path::Path;

use ax_usage::{
    collect_report, color_cost_banner, format_summary, import_agent_logs, SavingsQuery, UsagePeriod,
};

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
    import_agent_logs(true, true).await?;
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
                "models": report
                    .by_model
                    .iter()
                    .map(|row| serde_json::json!({ "label": row.label, "usd": row.usd }))
                    .collect::<Vec<_>>(),
                "agents": report
                    .by_agent
                    .iter()
                    .map(|row| serde_json::json!({ "label": row.label, "usd": row.usd }))
                    .collect::<Vec<_>>(),
                "sessionSourced": report.session_sourced,
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
    let mut text = color_cost_banner(
        &format_summary(&report, &snapshot, &settings, label.trim()),
        std::io::IsTerminal::is_terminal(&std::io::stdout()),
    );
    if action == "session" || action == "project" || action == "model" {
        text.push_str("\nBreakdown\n--------------------------------\n");
        let rows = if action == "project" {
            &report.by_project
        } else if action == "session" {
            &report.by_agent
        } else {
            &report.by_model
        };
        if action == "session" {
            text.push_str("Agent totals for this period.\n");
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

#[cfg(test)]
mod tests {
    use super::run;
    use ax_usage::{collect_report, SavingsQuery, UsagePeriod};

    #[tokio::test]
    async fn run_imports_a_cursor_transcript_before_the_report() {
        let home = tempfile::tempdir().unwrap();
        let db = tempfile::NamedTempFile::new().unwrap();
        let session = "sess-cost-import";
        let dir = home
            .path()
            .join(".cursor/projects/demo/agent-transcripts")
            .join(session);
        std::fs::create_dir_all(&dir).unwrap();
        // A fixed 08:00Z stamp is still in the future when CI runs earlier that UTC day,
        // and the month window ends at "now", so the imported event is invisible.
        let stamp = rfc3339_seconds_ago(120);
        std::fs::write(
            dir.join(format!("{session}.jsonl")),
            format!(
                r#"{{"role":"assistant","timestamp":"{stamp}","message":{{"model":"grok-4.7","usage":{{"input_tokens":42,"output_tokens":7}}}}}}"#
            ),
        )
        .unwrap();

        let prev_home = std::env::var_os("AX_HOME_DIR");
        let prev_db = std::env::var_os("AX_USAGE_DB");
        std::env::set_var("AX_HOME_DIR", home.path());
        std::env::set_var("AX_USAGE_DB", db.path());

        let ran = run("month", None, None, None, true, None).await;
        let report = collect_report(
            &SavingsQuery {
                period: UsagePeriod::MonthToDate,
                from: None,
                to: None,
            },
            None,
        )
        .await;

        match prev_home {
            Some(value) => std::env::set_var("AX_HOME_DIR", value),
            None => std::env::remove_var("AX_HOME_DIR"),
        }
        match prev_db {
            Some(value) => std::env::set_var("AX_USAGE_DB", value),
            None => std::env::remove_var("AX_USAGE_DB"),
        }

        ran.unwrap();
        let (report, _, _) = report.unwrap();
        assert!(
            report.by_model.iter().any(|row| row.label == "grok-4.7"),
            "models: {:?}",
            report.by_model
        );
        assert_eq!(report.tokens.input, 42);
        assert!(report.tokens.input_known);
    }

    fn rfc3339_seconds_ago(seconds: u64) -> String {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .saturating_sub(seconds);
        let days = (secs / 86_400) as i64;
        let tod = secs % 86_400;
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = (z - era * 146_097) as u64;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            tod / 3_600,
            (tod % 3_600) / 60,
            tod % 60
        )
    }
}
