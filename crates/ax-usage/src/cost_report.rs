//! Roll up recorded usage events with the catalog cost quote.

use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, Local, NaiveDate};

use crate::budget::{
    currency_label, format_money, load_settings, project_budget, BudgetSettings, BudgetSnapshot,
    DaySpend,
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
            by_project.entry(project).or_default().0 += usd;
            if let Some(day) = local_day(item.recorded.created_at) {
                *by_day.entry(day).or_default() += usd;
            }
        } else {
            unknown += 1;
            by_model.entry(label).or_default().1 += 1;
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
        by_day: by_day
            .into_iter()
            .map(|(date, usd)| DaySpend { date, usd })
            .collect(),
        cycle_costs_usd: cycle_costs,
        cycle_rows,
        tokens_avoided,
        savings_usd_est,
    }
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

pub fn format_summary(
    report: &CostReport,
    snapshot: &BudgetSnapshot,
    settings: &BudgetSettings,
    period_label: &str,
) -> String {
    let mut out = String::new();
    out.push_str("AX COST SUMMARY\n\n");
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
    out.push_str("\nTokens\n--------------------------------\n");
    out.push_str(&format!(
        "Input                  {}\n",
        token_cell(report.tokens.input, report.tokens.input_known)
    ));
    out.push_str(&format!(
        "Output                 {}\n",
        token_cell(report.tokens.output, report.tokens.output_known)
    ));
    out.push_str(&format!(
        "Cache read             {}\n",
        token_cell(report.tokens.cache_read, report.tokens.cache_read_known)
    ));
    out.push_str(&format!(
        "Cache write            {}\n",
        token_cell(report.tokens.cache_write, report.tokens.cache_write_known)
    ));
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

fn token_cell(n: i64, known: bool) -> String {
    if known {
        n.to_string()
    } else {
        "unknown".into()
    }
}

pub async fn collect_report(
    query: &SavingsQuery,
    project: Option<&std::path::Path>,
) -> Result<(CostReport, BudgetSnapshot, BudgetSettings), String> {
    let settings = load_settings(project);
    let range = resolve_period(query.period, query.from.as_deref(), query.to.as_deref())?;
    let rows = load_events(range.from_ms, range.to_ms).await?;
    let summary = query_savings_summary(query).await?;
    let report = build_report(
        &rows,
        |model| model.and_then(|name| price_for_cost(name).map(|(pricing, _)| pricing)),
        summary.tokens_saved_est,
        summary.cost_saved_usd_est,
    );
    let today = Local::now().date_naive();
    let month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
    let month_range = resolve_period(UsagePeriod::MonthToDate, None, None)?;
    let month_rows = load_events(month_range.from_ms, month_range.to_ms).await?;
    let month_report = build_report(
        &month_rows,
        |model| model.and_then(|name| price_for_cost(name).map(|(pricing, _)| pricing)),
        0,
        0.0,
    );
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
}
