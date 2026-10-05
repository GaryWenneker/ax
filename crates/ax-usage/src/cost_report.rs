//! Roll up recorded usage events with the catalog cost quote.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Local, NaiveDate};

use crate::budget::{
    currency_label, display_amount, format_money, load_settings, project_budget, BudgetSettings,
    BudgetSnapshot, Currency, DaySpend,
};
use crate::cost::{calculate, Cost, CostConfidence, CostUsage};
use crate::period::{resolve_period, UsagePeriod};
use crate::pricing::price_for_cost;
use crate::pricing::ModelPricing;
use crate::savings::{query_savings_summary, SavingsQuery};
use crate::store::open_pool;

#[derive(Debug, Clone, PartialEq)]
pub struct RecordedUsage {
    pub agent: String,
    pub session_id: String,
    pub turn_id: String,
    pub project: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone)]
struct Quoted {
    recorded: RecordedUsage,
    cost: Cost,
}

#[derive(Debug, Clone, Default)]
pub struct TokenTotals {
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub input_known: bool,
    pub output_known: bool,
    pub cache_read_known: bool,
    pub cache_write_known: bool,
}

#[derive(Debug, Clone)]
pub struct GroupSpend {
    pub label: String,
    pub usd: f64,
    pub unknown_events: i64,
}

#[derive(Debug, Clone)]
pub struct CostReport {
    pub events: i64,
    pub known_events: i64,
    pub unknown_events: i64,
    pub spend_usd: f64,
    pub tokens: TokenTotals,
    pub by_model: Vec<GroupSpend>,
    pub by_project: Vec<GroupSpend>,
    pub by_day: Vec<DaySpend>,
    pub cycle_costs_usd: Vec<f64>,
    pub cycle_rows: Vec<(String, f64)>,
    pub tokens_avoided: i64,
    pub savings_usd_est: f64,
    /// Whole-file counterfactual from `ax savings`. Zero when that summary was not attached.
    pub counterfactual_tokens: i64,
    /// Graph response tokens from `ax savings`.
    pub selected_context_tokens: i64,
    pub by_agent: Vec<GroupSpend>,
    /// True when models, agents, and spend were taken from imported sessions
    /// because this period has no quoted usage events.
    pub session_sourced: bool,
    /// Recorded session minutes for this period. Set only when the report is session-sourced.
    pub session_minutes: Option<u64>,
}

pub fn quote_recorded(row: &RecordedUsage, pricing: Option<ModelPricing>) -> Cost {
    let usage = CostUsage {
        provider: row.provider.clone(),
        model: row.model.clone(),
        input_tokens: row.input_tokens,
        output_tokens: row.output_tokens,
        cache_read_tokens: row.cache_read_tokens,
        cache_write_tokens: row.cache_write_tokens,
        timestamp_ms: Some(row.created_at),
        session_id: Some(row.session_id.clone()),
        turn_id: Some(row.turn_id.clone()),
        confidence: CostConfidence::Estimated,
    };
    calculate(&usage, pricing)
}

pub fn build_report(
    rows: &[RecordedUsage],
    price_of: impl Fn(Option<&str>) -> Option<ModelPricing>,
    tokens_avoided: i64,
    savings_usd_est: f64,
) -> CostReport {
    let quoted: Vec<Quoted> = rows
        .iter()
        .cloned()
        .map(|recorded| {
            let pricing = price_of(recorded.model.as_deref());
            let cost = quote_recorded(&recorded, pricing);
            Quoted { recorded, cost }
        })
        .collect();
    let mut tokens = TokenTotals::default();
    let mut by_model: BTreeMap<String, (f64, i64)> = BTreeMap::new();
    let mut by_agent: BTreeMap<String, (f64, i64)> = BTreeMap::new();
    let mut by_project: BTreeMap<String, (f64, i64)> = BTreeMap::new();
    let mut by_day: BTreeMap<NaiveDate, f64> = BTreeMap::new();
    let mut cycle_costs = Vec::new();
    let mut cycle_rows = Vec::new();
    let mut spend = 0.0;
    let mut known = 0i64;
    let mut unknown = 0i64;
    for item in &quoted {
        add_tokens(&mut tokens, &item.recorded);
        let label = item
            .recorded
            .model
            .clone()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| "unknown".into());
        let agent = {
            let name = item.recorded.agent.trim();
            if name.is_empty() {
                "unknown".to_string()
            } else {
                name.to_string()
            }
        };
        let project = item
            .recorded
            .project
            .clone()
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "(no project)".into());
        if item.cost.total_confidence == CostConfidence::Estimated {
            let usd = item.cost.total_usd.unwrap_or(0.0);
            spend += usd;
            known += 1;
            cycle_costs.push(usd);
            cycle_rows.push((label.clone(), usd));
            by_model.entry(label).or_default().0 += usd;
            by_agent.entry(agent).or_default().0 += usd;
            by_project.entry(project).or_default().0 += usd;
            if let Some(day) = local_day(item.recorded.created_at) {
                *by_day.entry(day).or_default() += usd;
            }
        } else {
            unknown += 1;
            by_model.entry(label).or_default().1 += 1;
            by_agent.entry(agent).or_default().1 += 1;
            by_project.entry(project).or_default().1 += 1;
        }
    }
    CostReport {
        events: quoted.len() as i64,
        known_events: known,
        unknown_events: unknown,
        spend_usd: spend,
        tokens,
        by_model: groups(by_model),
        by_project: groups(by_project),
        by_agent: groups(by_agent),
        by_day: by_day
            .into_iter()
            .map(|(date, usd)| DaySpend { date, usd })
            .collect(),
        cycle_costs_usd: cycle_costs,
        cycle_rows,
        tokens_avoided,
        savings_usd_est,
        counterfactual_tokens: 0,
        selected_context_tokens: 0,
        session_sourced: false,
        session_minutes: None,
    }
}

/// Fill an empty usage-event report from imported agent sessions.
/// Quoted usage events win: a non-empty event list is left unchanged.
pub fn apply_session_cost(report: &mut CostReport, coverage: &crate::savings::SessionCostCoverage) {
    if report.events != 0 || (coverage.models.is_empty() && coverage.agents.is_empty()) {
        return;
    }
    report.by_model = coverage
        .models
        .iter()
        .map(|row| GroupSpend {
            label: row.label.clone(),
            usd: row.usd,
            unknown_events: 0,
        })
        .collect();
    report.by_agent = coverage
        .agents
        .iter()
        .map(|row| GroupSpend {
            label: row.label.clone(),
            usd: row.usd,
            unknown_events: 0,
        })
        .collect();
    report.spend_usd = coverage.spend_usd;
    if coverage.input_known {
        report.tokens.input = coverage.input_tokens;
        report.tokens.input_known = true;
    }
    if coverage.output_known {
        report.tokens.output = coverage.output_tokens;
        report.tokens.output_known = true;
    }
    report.by_day = coverage
        .by_day
        .iter()
        .filter_map(|row| parse_iso_date(&row.day).map(|date| DaySpend { date, usd: row.usd }))
        .collect();
    report.session_minutes = Some(coverage.minutes);
    report.session_sourced = true;
}

fn parse_iso_date(value: &str) -> Option<NaiveDate> {
    let mut parts = value.split('-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    NaiveDate::from_ymd_opt(year, month, day)
}

fn add_tokens(totals: &mut TokenTotals, row: &RecordedUsage) {
    if let Some(n) = row.input_tokens {
        totals.input += n.max(0);
        totals.input_known = true;
    }
    if let Some(n) = row.output_tokens {
        totals.output += n.max(0);
        totals.output_known = true;
    }
    if let Some(n) = row.cache_read_tokens {
        totals.cache_read += n.max(0);
        totals.cache_read_known = true;
    }
    if let Some(n) = row.cache_write_tokens {
        totals.cache_write += n.max(0);
        totals.cache_write_known = true;
    }
}

fn groups(map: BTreeMap<String, (f64, i64)>) -> Vec<GroupSpend> {
    let mut rows: Vec<GroupSpend> = map
        .into_iter()
        .map(|(label, (usd, unknown_events))| GroupSpend {
            label,
            usd,
            unknown_events,
        })
        .collect();
    rows.sort_by(|a, b| {
        b.usd
            .partial_cmp(&a.usd)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.label.cmp(&b.label))
    });
    rows
}

pub fn today_iso() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

fn local_day(ms: i64) -> Option<NaiveDate> {
    if ms <= 0 {
        return None;
    }
    DateTime::from_timestamp_millis(ms).map(|utc| utc.with_timezone(&Local).date_naive())
}

const AX_BANNER: &str = "\
 █████╗ ██╗  ██╗\n\
██╔══██╗╚██╗██╔╝\n\
███████║ ╚███╔╝ \n\
██╔══██║ ██╔██╗ \n\
██║  ██║██╔╝ ██╗\n\
╚═╝  ╚═╝╚═╝  ╚═╝\n\
";

pub fn format_summary(
    report: &CostReport,
    snapshot: &BudgetSnapshot,
    settings: &BudgetSettings,
    period_label: &str,
) -> String {
    let mut out = String::new();
    out.push_str(AX_BANNER);
    out.push('\n');
    out.push_str(&banner_info(period_label, report, settings));
    out.push_str("\n\nAX COST SUMMARY\n\n");
    out.push_str(&format!("Period: {period_label}\n\n"));
    out.push_str(&format!(
        "Budget                 {}\n",
        money_opt(snapshot.monthly_budget_usd, settings)
    ));
    out.push_str(&format!(
        "Spent                  {}\n",
        format_money(report.spend_usd, settings)
    ));
    out.push_str(&format!(
        "Remaining              {}\n",
        money_opt(snapshot.remaining_budget_usd, settings)
    ));
    out.push_str(&format!(
        "Projected              {}\n",
        format_money(snapshot.projected_spend_usd, settings)
    ));
    let pct = snapshot
        .usage_percent
        .map(|p| format!("{p:.0}%"))
        .unwrap_or_else(|| "unknown".into());
    out.push_str(&format!("Budget usage           {pct}\n\n"));
    let today_usd = report
        .by_day
        .iter()
        .find(|day| day.date == snapshot.today)
        .map(|day| day.usd)
        .unwrap_or(0.0);
    out.push_str(&format!(
        "Today                  {}\n",
        format_money(today_usd, settings)
    ));
    out.push_str(&format!(
        "Recommended/day        {}\n",
        money_opt(snapshot.recommended_daily_usd, settings)
    ));
    out.push_str(&format!(
        "Currency               {}\n",
        currency_label(settings)
    ));
    if !snapshot.fx_known {
        out.push_str("FX                     unknown (set budget.usdPerEur)\n");
    }
    out.push_str(&format!(
        "\nKnown events           {}\n",
        report.known_events
    ));
    out.push_str(&format!(
        "Unknown events         {}\n",
        report.unknown_events
    ));
    out.push_str("\nProvider / model\n--------------------------------\n");
    if report.by_model.is_empty() {
        out.push_str("(no recorded usage)\n");
    }
    for row in &report.by_model {
        let amount = if row.unknown_events > 0 && row.usd == 0.0 {
            "unknown".to_string()
        } else {
            format_money(row.usd, settings)
        };
        out.push_str(&format!("{:<24} {amount}\n", row.label));
        if row.unknown_events > 0 && row.usd > 0.0 {
            out.push_str(&format!("  unknown events       {}\n", row.unknown_events));
        }
    }
    out.push_str("\nAgents\n--------------------------------\n");
    if report.by_agent.is_empty() {
        out.push_str("(no recorded usage)\n");
    }
    for row in &report.by_agent {
        let amount = if row.unknown_events > 0 && row.usd == 0.0 {
            "unknown".to_string()
        } else {
            format_money(row.usd, settings)
        };
        out.push_str(&format!("{:<24} {amount}\n", row.label));
    }
    if report.session_sourced {
        out.push_str(
            "Models, agents, input tokens, and spend come from imported agent sessions.\n",
        );
    }
    let minutes = report.session_minutes;
    out.push_str("\nTokens\n--------------------------------\n");
    push_optional_metric(
        &mut out,
        "Input",
        known_count(report.tokens.input, report.tokens.input_known),
        minutes,
    );
    push_optional_metric(
        &mut out,
        "Output",
        known_count(report.tokens.output, report.tokens.output_known),
        minutes,
    );
    push_optional_metric(
        &mut out,
        "Cache read",
        known_count(report.tokens.cache_read, report.tokens.cache_read_known),
        minutes,
    );
    push_optional_metric(
        &mut out,
        "Cache write",
        known_count(report.tokens.cache_write, report.tokens.cache_write_known),
        minutes,
    );
    push_session_time(&mut out, minutes);
    let cache_read = if report.tokens.cache_read_known {
        Some(report.tokens.cache_read)
    } else {
        None
    };
    let efficiency = crate::context_plan::efficiency(
        report.counterfactual_tokens,
        report.selected_context_tokens,
        cache_read,
        report.tokens_avoided,
    );
    out.push_str("\nContext efficiency\n--------------------------------\n");
    push_metric(
        &mut out,
        "Raw context",
        &efficiency.raw_context_tokens.to_string(),
    );
    push_metric(
        &mut out,
        "Selected context",
        &efficiency.selected_context_tokens.to_string(),
    );
    push_optional_metric(
        &mut out,
        "Cache read",
        efficiency.cache_read_tokens.map(|n| n.to_string()),
        minutes,
    );
    push_optional_metric(
        &mut out,
        "New context",
        efficiency.new_context_tokens.map(|n| n.to_string()),
        minutes,
    );
    push_session_time(&mut out, minutes);
    let cycles = u64::try_from(report.known_events).unwrap_or(0);
    let cycle = crate::context_plan::cycle_efficiency(
        cycles,
        report.spend_usd,
        report.tokens.input,
        report.tokens.input_known,
        report.tokens.output,
        report.tokens.output_known,
        report.tokens.cache_read,
        report.tokens.cache_read_known,
        report.tokens_avoided,
    );
    out.push_str("\nCycle efficiency\n--------------------------------\n");
    push_metric(&mut out, "Known cycles", &cycles.to_string());
    push_optional_metric(
        &mut out,
        "Cost/cycle",
        cycle.cost_per_cycle_usd.map(|v| format_money(v, settings)),
        minutes,
    );
    push_optional_metric(
        &mut out,
        "Input/cycle",
        cycle.input_per_cycle.map(|n| n.to_string()),
        minutes,
    );
    push_optional_metric(
        &mut out,
        "Output/cycle",
        cycle.output_per_cycle.map(|n| n.to_string()),
        minutes,
    );
    push_optional_metric(
        &mut out,
        "Cache hit ratio",
        cycle.cache_hit_ratio.map(|r| format!("{:.0}%", r * 100.0)),
        minutes,
    );
    if let Some(mins) = minutes {
        push_metric(&mut out, "Time", &format!("{mins} min"));
        if mins > 0 {
            push_metric(
                &mut out,
                "Cost/min",
                &format_rate(report.spend_usd / mins as f64, settings),
            );
            if report.tokens.input_known {
                let per_min = i64::try_from(mins).unwrap_or(i64::MAX);
                push_metric(
                    &mut out,
                    "Input/min",
                    &(report.tokens.input / per_min).to_string(),
                );
            }
        }
    }
    out.push_str("Known cycles are quoted usage events. This is not a completed-task count.\n");
    out.push_str("\nAx savings\n--------------------------------\n");
    out.push_str(&format!(
        "Tokens avoided         {}\n",
        report.tokens_avoided
    ));
    out.push_str(&format!(
        "Estimated savings      ${:.2}\n",
        report.savings_usd_est
    ));
    out.push_str("\nEstimated savings use the reference model input rate from ax savings.\n");
    out.push_str(
        "Event spend is a catalog estimate. Unknown means a token count or a rate was missing.\n",
    );
    out.push_str("Ax does not block provider API calls.\n");
    out
}

fn money_opt(usd: Option<f64>, settings: &BudgetSettings) -> String {
    usd.map(|v| format_money(v, settings))
        .unwrap_or_else(|| "unknown".into())
}

fn banner_info(period_label: &str, report: &CostReport, settings: &BudgetSettings) -> String {
    let mut parts = vec![
        period_label.to_string(),
        format_money(report.spend_usd, settings),
    ];
    if let Some(model) = report.by_model.first() {
        parts.push(model.label.clone());
    }
    if let Some(mins) = report.session_minutes {
        parts.push(format!("{mins} min"));
    }
    parts.join("  ·  ")
}

fn format_rate(usd: f64, settings: &BudgetSettings) -> String {
    match display_amount(usd, settings) {
        Some(amount) if amount.abs() > 0.0 && amount.abs() < 0.005 => match settings.currency {
            Currency::Usd => format!("${amount:.4}"),
            Currency::Eur => format!("€{amount:.4}"),
        },
        _ => format_money(usd, settings),
    }
}

fn push_metric(out: &mut String, label: &str, value: &str) {
    out.push_str(&format!("{label:<23}{value}\n"));
}

fn known_count(n: i64, known: bool) -> Option<String> {
    known.then(|| n.to_string())
}

/// A missing count stays "unknown" until session minutes exist. Then the row is omitted
/// and the section shows `Time` instead.
fn push_optional_metric(
    out: &mut String,
    label: &str,
    value: Option<String>,
    minutes: Option<u64>,
) {
    match (value, minutes) {
        (Some(text), _) => push_metric(out, label, &text),
        (None, None) => push_metric(out, label, "unknown"),
        (None, Some(_)) => {}
    }
}

fn push_session_time(out: &mut String, minutes: Option<u64>) {
    if let Some(mins) = minutes {
        push_metric(out, "Time", &format!("{mins} min"));
    }
}

/// Paint the six-line AX banner when `color` is set. The rest of the report stays uncolored.
pub fn color_cost_banner(text: &str, color: bool) -> String {
    if !color {
        return text.to_string();
    }
    const BANNER_LINES: usize = 6;
    let mut out = String::from("\u{1b}[96m");
    let mut rest_started = false;
    for (index, line) in text.split_inclusive('\n').enumerate() {
        if index == BANNER_LINES {
            out.push_str("\u{1b}[0m");
            rest_started = true;
        }
        out.push_str(line);
    }
    if !rest_started {
        out.push_str("\u{1b}[0m");
    }
    out
}

pub async fn collect_report(
    query: &SavingsQuery,
    project: Option<&std::path::Path>,
) -> Result<(CostReport, BudgetSnapshot, BudgetSettings), String> {
    let settings = load_settings(project);
    let range = resolve_period(query.period, query.from.as_deref(), query.to.as_deref())?;
    let rows = load_events(range.from_ms, range.to_ms).await?;
    let summary = query_savings_summary(query).await?;
    let mut report = build_report(
        &rows,
        |model| model.and_then(|name| price_for_cost(name).map(|(pricing, _)| pricing)),
        summary.tokens_saved_est,
        summary.cost_saved_usd_est,
    );
    report.counterfactual_tokens = summary.counterfactual_tokens_est;
    report.selected_context_tokens = summary.graph_response_tokens_est;
    apply_session_cost(&mut report, &summary.session_cost);
    let today = Local::now().date_naive();
    let month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
    let month_range = resolve_period(UsagePeriod::MonthToDate, None, None)?;
    let month_rows = load_events(month_range.from_ms, month_range.to_ms).await?;
    let mut month_report = build_report(
        &month_rows,
        |model| model.and_then(|name| price_for_cost(name).map(|(pricing, _)| pricing)),
        0,
        0.0,
    );
    if range.from_ms == month_range.from_ms && range.to_ms == month_range.to_ms {
        apply_session_cost(&mut month_report, &summary.session_cost);
    } else {
        let month_summary = query_savings_summary(&SavingsQuery {
            period: UsagePeriod::MonthToDate,
            from: None,
            to: None,
        })
        .await?;
        apply_session_cost(&mut month_report, &month_summary.session_cost);
    }
    let snapshot = project_budget(&settings, month, today, &month_report.by_day);
    Ok((report, snapshot, settings))
}

pub async fn budget_check(project: Option<&std::path::Path>) -> Result<(String, String), String> {
    let query = SavingsQuery {
        period: UsagePeriod::MonthToDate,
        from: None,
        to: None,
    };
    let (_, snapshot, settings) = collect_report(&query, project).await?;
    let decision = match snapshot.decision {
        crate::budget::BudgetDecision::Allow => "allow",
        crate::budget::BudgetDecision::Warn => "warn",
        crate::budget::BudgetDecision::Deny => "deny",
    };
    let text = crate::budget::banner(&snapshot, &settings).unwrap_or_else(|| {
        "AX BUDGET\n\nWithin the configured budget.\n\nAx does not block a provider API call."
            .into()
    });
    Ok((decision.to_string(), text))
}

pub async fn load_events(from_ms: i64, to_ms: i64) -> Result<Vec<RecordedUsage>, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let rows: Vec<(
        String,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        i64,
    )> = sqlx::query_as(
        "SELECT agent, session_id, turn_id, project, provider, model,
                input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, created_at
         FROM agent_usage_event
         WHERE created_at >= ? AND created_at <= ?",
    )
    .bind(from_ms)
    .bind(to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(
                agent,
                session_id,
                turn_id,
                project,
                provider,
                model,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_write_tokens,
                created_at,
            )| {
                RecordedUsage {
                    agent,
                    session_id,
                    turn_id,
                    project,
                    provider,
                    model,
                    input_tokens,
                    output_tokens,
                    cache_read_tokens,
                    cache_write_tokens,
                    created_at,
                }
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::ModelPricing;

    fn row(model: &str, input: Option<i64>, cache_read: Option<i64>) -> RecordedUsage {
        RecordedUsage {
            agent: "claude".into(),
            session_id: "s".into(),
            turn_id: "1".into(),
            project: Some("ax".into()),
            provider: Some("anthropic".into()),
            model: Some(model.into()),
            input_tokens: input,
            output_tokens: Some(0),
            cache_read_tokens: cache_read,
            cache_write_tokens: Some(0),
            created_at: 1_759_000_000_000,
        }
    }

    fn price(model: Option<&str>) -> Option<ModelPricing> {
        match model {
            Some("known") => Some(ModelPricing {
                input_per_mtok: 3.0,
                output_per_mtok: 15.0,
                cache_read_per_mtok: Some(0.3),
                cache_write_per_mtok: Some(3.75),
            }),
            _ => None,
        }
    }

    #[test]
    fn known_and_unknown_events_stay_separate() {
        let report = build_report(
            &[
                row("known", Some(1_000_000), Some(0)),
                row("missing", Some(1_000_000), Some(0)),
            ],
            price,
            10,
            0.03,
        );
        assert_eq!(report.known_events, 1);
        assert_eq!(report.unknown_events, 1);
        assert!((report.spend_usd - 3.0).abs() < 1e-9);
        assert_eq!(report.cycle_costs_usd.len(), 1);
        let text = format_summary(
            &report,
            &project_budget(
                &BudgetSettings::default(),
                chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
                &report.by_day,
            ),
            &BudgetSettings::default(),
            "November 2026",
        );
        assert!(text.contains("AX COST SUMMARY"));
        assert!(text.contains("unknown"));
        assert!(text.contains("Tokens avoided"));
    }

    fn coverage_fixture() -> crate::savings::SessionCostCoverage {
        crate::savings::SessionCostCoverage {
            models: vec![
                crate::savings::SessionGroupSpend {
                    label: "grok-4.7".into(),
                    usd: 1.25,
                },
                crate::savings::SessionGroupSpend {
                    label: "claude-opus-5-5".into(),
                    usd: 0.5,
                },
            ],
            agents: vec![crate::savings::SessionGroupSpend {
                label: "cursor".into(),
                usd: 1.75,
            }],
            input_tokens: 140,
            input_known: true,
            output_tokens: 0,
            output_known: false,
            spend_usd: 1.75,
            by_day: vec![crate::savings::SessionDaySpend {
                day: "2026-10-05".into(),
                usd: 1.75,
            }],
            minutes: 46,
        }
    }

    #[test]
    fn imported_sessions_fill_empty_cost_report() {
        let mut report = build_report(&[], price, 0, 0.0);
        apply_session_cost(&mut report, &coverage_fixture());
        assert_eq!(report.events, 0);
        assert_eq!(report.known_events, 0);
        assert!(report.session_sourced);
        assert!((report.spend_usd - 1.75).abs() < 1e-9);
        assert_eq!(report.tokens.input, 140);
        assert!(report.tokens.input_known);
        assert!(!report.tokens.output_known);
        assert!(!report.tokens.cache_read_known);
        assert!(!report.tokens.cache_write_known);
        let labels: Vec<&str> = report.by_model.iter().map(|m| m.label.as_str()).collect();
        assert!(labels.contains(&"grok-4.7"), "{labels:?}");
        assert_eq!(report.by_agent.len(), 1);
        assert_eq!(report.by_agent[0].label, "cursor");
        assert_eq!(
            report.by_day[0].date,
            chrono::NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()
        );
        let text = format_summary(
            &report,
            &project_budget(
                &BudgetSettings::default(),
                chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
                &report.by_day,
            ),
            &BudgetSettings::default(),
            "October 2026",
        );
        assert!(text.contains("grok-4.7"), "{text}");
        assert!(text.contains("\nAgents\n"), "{text}");
        assert!(text.contains("cursor"), "{text}");
        assert!(text.contains("imported agent sessions"), "{text}");
    }

    #[test]
    fn session_report_leads_with_ax_banner_and_minutes() {
        let mut report = build_report(&[], price, 0, 0.0);
        apply_session_cost(&mut report, &coverage_fixture());
        let text = format_summary(
            &report,
            &project_budget(
                &BudgetSettings::default(),
                chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
                &report.by_day,
            ),
            &BudgetSettings::default(),
            "Today",
        );
        assert!(text.contains("█████"), "{text}");
        assert!(text.starts_with("█████╗"), "{text}");
        assert!(text.contains("46 min"), "{text}");
        assert!(text.contains("Cost/min"), "{text}");
        assert!(text.contains("Input/min"), "{text}");
        let body = text
            .split("\nTokens\n")
            .nth(1)
            .unwrap()
            .split("\nAx savings\n")
            .next()
            .unwrap();
        assert!(!body.to_ascii_lowercase().contains("unknown"), "{body}");
        assert!(!body.contains("Output"), "{body}");
    }

    #[test]
    fn color_cost_banner_paints_only_the_six_art_lines() {
        let plain = "█████╗\n2\n3\n4\n5\n6\n\nToday\n";
        assert_eq!(color_cost_banner(plain, false), plain);
        let colored = color_cost_banner(plain, true);
        assert!(colored.starts_with("\u{1b}[96m█████╗\n"), "{colored}");
        assert!(colored.contains("6\n\u{1b}[0m\nToday\n"), "{colored}");
        assert!(!colored.contains("\u{1b}[96m\nToday"), "{colored}");
    }

    #[test]
    fn tiny_cost_per_minute_keeps_four_decimals() {
        let mut report = build_report(&[], price, 0, 0.0);
        let mut coverage = coverage_fixture();
        coverage.spend_usd = 0.5;
        coverage.minutes = 201;
        apply_session_cost(&mut report, &coverage);
        let text = format_summary(
            &report,
            &project_budget(
                &BudgetSettings::default(),
                chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap(),
                chrono::NaiveDate::from_ymd_opt(2026, 10, 5).unwrap(),
                &report.by_day,
            ),
            &BudgetSettings::default(),
            "Today",
        );
        assert!(
            text.lines()
                .any(|line| line.starts_with("Cost/min") && line.contains("$0.0025")),
            "{text}"
        );
    }

    #[test]
    fn usage_events_are_not_replaced_by_sessions() {
        let mut report = build_report(&[row("known", Some(1_000_000), Some(0))], price, 0, 0.0);
        let spend = report.spend_usd;
        apply_session_cost(&mut report, &coverage_fixture());
        assert!((report.spend_usd - spend).abs() < 1e-9);
        assert!(!report.session_sourced);
        assert!(report.by_agent.iter().any(|row| row.label == "claude"));
        assert!(report.by_model.iter().any(|m| m.label == "known"));
        assert!(!report.by_model.iter().any(|m| m.label == "grok-4.7"));
    }
}
