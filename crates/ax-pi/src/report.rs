//! Text and JSON economics reports. Figures are estimates.

use crate::economics::display_amount;
use crate::store::{ModelRow, OptimizationRow, PersistedSnapshot, TurnRow};
use crate::types::{BudgetSnapshot, ToolEconomicsRecord};
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Serialize)]
pub struct EconomicsReport {
    /// The session the report is filtered to; `None` when it covers every session.
    pub session_id: Option<String>,
    pub turns: usize,
    pub models: Vec<String>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub tool_output_tokens: BTreeMap<String, u64>,
    pub repeated_output_tokens: u64,
    pub estimated_model_cost: Option<f64>,
    pub potential_optimization_cost: Option<f64>,
    pub largest_tool: Option<String>,
    pub currency: String,
    pub budget: Option<BudgetSnapshot>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OptimizationReport {
    pub items: Vec<OptimizationGroup>,
    pub total_estimated_avoidable_tokens: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OptimizationGroup {
    pub label: String,
    pub calls: usize,
    pub tokens: u64,
    pub alternative: String,
    pub estimated_reduction_percent: f64,
}

/// Borrowed rows a report is built from, so a live integration can report
/// without copying its state.
pub(crate) struct ReportRows<'a> {
    pub turns: &'a [TurnRow],
    pub tools: &'a [ToolEconomicsRecord],
    pub models: &'a [ModelRow],
    pub optimizations: &'a [OptimizationRow],
}

impl<'a> ReportRows<'a> {
    fn of(snapshot: &'a PersistedSnapshot) -> Self {
        Self {
            turns: &snapshot.turns,
            tools: &snapshot.tools,
            models: &snapshot.models,
            optimizations: &snapshot.optimizations,
        }
    }
}

pub fn economics_from(
    snapshot: &PersistedSnapshot,
    session_filter: Option<&str>,
    currency: &str,
    budget: Option<BudgetSnapshot>,
) -> EconomicsReport {
    economics_of(&ReportRows::of(snapshot), session_filter, currency, budget)
}

pub(crate) fn economics_of(
    rows: &ReportRows<'_>,
    session_filter: Option<&str>,
    currency: &str,
    budget: Option<BudgetSnapshot>,
) -> EconomicsReport {
    let tools = filter_tools(rows.tools, session_filter);
    let turns = filter_turns(rows.turns, session_filter);
    let models = filter_models(rows.models, session_filter);
    let mut tool_output_tokens: BTreeMap<String, u64> = BTreeMap::new();
    let mut repeated_output_tokens = 0u64;
    for tool in &tools {
        *tool_output_tokens
            .entry(tool.tool_name.clone())
            .or_insert(0) += u64::from(tool.output_tokens);
        if tool.repeated {
            repeated_output_tokens += u64::from(tool.output_tokens);
        }
    }
    let largest_tool = tool_output_tokens
        .iter()
        .max_by_key(|(_, tokens)| *tokens)
        .map(|(name, _)| name.clone());
    let input_tokens = models.iter().filter_map(|model| model.input_tokens).sum();
    let output_tokens = models.iter().filter_map(|model| model.output_tokens).sum();
    let cached_input_tokens = models
        .iter()
        .filter_map(|model| model.cached_input_tokens)
        .sum();
    let estimated_model_cost = sum_optional(models.iter().map(|model| model.estimated_cost));
    EconomicsReport {
        session_id: session_filter.map(str::to_string),
        turns: turns.len(),
        models: models
            .iter()
            .filter_map(|model| model.model.clone())
            .collect(),
        input_tokens,
        output_tokens,
        cached_input_tokens,
        tool_output_tokens,
        repeated_output_tokens,
        estimated_model_cost,
        potential_optimization_cost: potential_cost(rows.optimizations, &tools),
        largest_tool,
        currency: currency.to_string(),
        budget,
    }
}

pub fn optimization_from(snapshot: &PersistedSnapshot) -> OptimizationReport {
    optimization_of(&snapshot.optimizations)
}

pub(crate) fn optimization_of(optimizations: &[OptimizationRow]) -> OptimizationReport {
    let mut groups: BTreeMap<String, OptimizationGroup> = BTreeMap::new();
    let mut total = 0u64;
    for row in optimizations {
        let saved = row
            .alternative
            .estimated_current_tokens
            .saturating_sub(row.alternative.estimated_alternative_tokens);
        total += u64::from(saved);
        let entry = groups
            .entry(row.alternative.tool_name.clone())
            .or_insert(OptimizationGroup {
                label: row.alternative.tool_name.clone(),
                calls: 0,
                tokens: 0,
                alternative: row.alternative.tool_name.clone(),
                estimated_reduction_percent: 0.0,
            });
        entry.calls += 1;
        entry.tokens += u64::from(row.alternative.estimated_current_tokens);
        entry.estimated_reduction_percent = row.alternative.estimated_reduction_percent;
    }
    OptimizationReport {
        items: groups.into_values().collect(),
        total_estimated_avoidable_tokens: total,
    }
}

/// Convert stored USD figures into the configured display currency.
/// Euro display needs `usd_per_eur`. Without that rate the label stays USD.
pub fn apply_display_currency(
    report: &mut EconomicsReport,
    currency: &str,
    usd_per_eur: Option<f64>,
) {
    let label = display_amount(
        report.estimated_model_cost.unwrap_or(0.0),
        currency,
        usd_per_eur,
    )
    .1;
    if let Some(cost) = report.estimated_model_cost {
        report.estimated_model_cost = Some(display_amount(cost, currency, usd_per_eur).0);
    }
    if let Some(cost) = report.potential_optimization_cost {
        report.potential_optimization_cost = Some(display_amount(cost, currency, usd_per_eur).0);
    }
    if let Some(budget) = report.budget.as_mut() {
        budget.spent = display_amount(budget.spent, currency, usd_per_eur).0;
        budget.remaining = display_amount(budget.remaining, currency, usd_per_eur).0;
        budget.projected = display_amount(budget.projected, currency, usd_per_eur).0;
        budget.currency = label.clone();
    }
    report.currency = label;
}

pub fn format_economics(report: &EconomicsReport) -> String {
    let mut lines = vec![
        "AX AGENT ECONOMICS".into(),
        String::new(),
        format!(
            "Session:\n{}",
            report.session_id.as_deref().unwrap_or("(all sessions)")
        ),
        String::new(),
        format!("Turns:\n{}", report.turns),
        String::new(),
        format!(
            "Model:\n{}",
            if report.models.is_empty() {
                "(none)".into()
            } else {
                report.models.join(", ")
            }
        ),
        String::new(),
        "Tokens:".into(),
        format!("Input       {}", format_int(report.input_tokens)),
        format!("Output       {}", format_int(report.output_tokens)),
        format!("Cached       {}", format_int(report.cached_input_tokens)),
        String::new(),
        "Tool output:".into(),
    ];
    if report.tool_output_tokens.is_empty() {
        lines.push("(none)".into());
    } else {
        for (name, tokens) in &report.tool_output_tokens {
            lines.push(format!("{name:<12} {}", format_int(*tokens)));
        }
    }
    lines.push(String::new());
    lines.push(format!(
        "Estimated cost:\n{}",
        money(report.estimated_model_cost, &report.currency)
    ));
    lines.push(String::new());
    lines.push(format!(
        "Potential optimization:\n{}",
        money(report.potential_optimization_cost, &report.currency)
    ));
    lines.push(String::new());
    lines.push(format!(
        "Largest source:\n{}",
        report.largest_tool.as_deref().unwrap_or("(none)")
    ));
    lines.push(String::new());
    lines.push(format!(
        "Repeated output:\n{} tokens",
        format_int(report.repeated_output_tokens)
    ));
    if let Some(budget) = &report.budget {
        lines.push(String::new());
        lines.push(format!(
            "Budget ({}):\nspent {}  remaining {}  projected {}",
            budget.currency,
            plain(budget.spent),
            plain(budget.remaining),
            plain(budget.projected)
        ));
    }
    lines.join("\n")
}

pub fn format_optimization(report: &OptimizationReport) -> String {
    let mut lines = vec![
        "AX OPTIMIZATION REPORT".into(),
        String::new(),
        "Potential savings".into(),
        String::new(),
    ];
    if report.items.is_empty() {
        lines.push("(none)".into());
    }
    for (index, item) in report.items.iter().enumerate() {
        lines.push(format!("{}. {}", index + 1, item.label));
        lines.push(format!("   {} calls", item.calls));
        lines.push(format!("   {} tokens", format_int(item.tokens)));
        lines.push(format!("   alternative: {}", item.alternative));
        lines.push(format!(
            "   estimated reduction: {:.0}%",
            item.estimated_reduction_percent
        ));
        lines.push(String::new());
    }
    lines.push(format!(
        "Total estimated avoidable context:\n{} tokens",
        format_int(report.total_estimated_avoidable_tokens)
    ));
    lines.push(String::new());
    lines.push("These figures are estimates.".into());
    lines.join("\n")
}

fn filter_tools<'a>(
    tools: &'a [ToolEconomicsRecord],
    session: Option<&str>,
) -> Vec<&'a ToolEconomicsRecord> {
    tools
        .iter()
        .filter(|tool| session.map(|id| tool.session_id == id).unwrap_or(true))
        .collect()
}

fn filter_turns<'a>(turns: &'a [TurnRow], session: Option<&str>) -> Vec<&'a TurnRow> {
    turns
        .iter()
        .filter(|turn| session.map(|id| turn.session_id == id).unwrap_or(true))
        .collect()
}

fn filter_models<'a>(models: &'a [ModelRow], session: Option<&str>) -> Vec<&'a ModelRow> {
    models
        .iter()
        .filter(|model| session.map(|id| model.session_id == id).unwrap_or(true))
        .collect()
}

fn potential_cost(rows: &[OptimizationRow], tools: &[&ToolEconomicsRecord]) -> Option<f64> {
    let _ = tools;
    let mut total = 0.0;
    let mut any = false;
    for row in rows {
        let saved = row
            .alternative
            .estimated_current_tokens
            .saturating_sub(row.alternative.estimated_alternative_tokens);
        if let Some(tool) = tools.iter().find(|tool| tool.id == row.tool_call_id) {
            if let Some(cost) = tool.context_cost_usd {
                if tool.output_tokens > 0 {
                    total += cost * (f64::from(saved) / f64::from(tool.output_tokens));
                    any = true;
                }
            }
        }
    }
    any.then_some(total)
}

fn sum_optional<I>(values: I) -> Option<f64>
where
    I: Iterator<Item = Option<f64>>,
{
    let mut total = 0.0;
    let mut any = false;
    for value in values.flatten() {
        total += value;
        any = true;
    }
    any.then_some(total)
}

fn format_int(value: u64) -> String {
    let raw = value.to_string();
    let mut out = String::new();
    for (index, ch) in raw.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn money(amount: Option<f64>, currency: &str) -> String {
    match amount {
        Some(value) if currency.eq_ignore_ascii_case("eur") => format!("€{value:.2}"),
        Some(value) => format!("{value:.2} {currency}"),
        None => "unknown".into(),
    }
}

fn plain(value: f64) -> String {
    format!("{value:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eur_display_divides_by_usd_per_eur() {
        let mut report = EconomicsReport {
            session_id: None,
            turns: 0,
            models: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            tool_output_tokens: BTreeMap::new(),
            repeated_output_tokens: 0,
            estimated_model_cost: Some(1.08),
            potential_optimization_cost: Some(0.54),
            largest_tool: None,
            currency: "USD".into(),
            budget: None,
        };
        apply_display_currency(&mut report, "EUR", Some(1.08));
        assert_eq!(report.currency, "EUR");
        assert!((report.estimated_model_cost.unwrap() - 1.0).abs() < 0.001);
        assert!((report.potential_optimization_cost.unwrap() - 0.5).abs() < 0.001);
        let text = format_economics(&report);
        assert!(text.contains("€1.00"));
    }

    #[test]
    fn eur_without_a_rate_stays_usd() {
        let mut report = EconomicsReport {
            session_id: None,
            turns: 0,
            models: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            tool_output_tokens: BTreeMap::new(),
            repeated_output_tokens: 0,
            estimated_model_cost: Some(2.0),
            potential_optimization_cost: None,
            largest_tool: None,
            currency: "EUR".into(),
            budget: None,
        };
        apply_display_currency(&mut report, "EUR", None);
        assert_eq!(report.currency, "USD");
        assert!((report.estimated_model_cost.unwrap() - 2.0).abs() < 0.001);
    }

    fn report_with_budget(currency: &str) -> EconomicsReport {
        EconomicsReport {
            session_id: None,
            turns: 0,
            models: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            tool_output_tokens: BTreeMap::new(),
            repeated_output_tokens: 0,
            estimated_model_cost: None,
            potential_optimization_cost: None,
            largest_tool: None,
            currency: currency.into(),
            budget: Some(BudgetSnapshot {
                spent: 10.8,
                remaining: 54.0,
                projected: 21.6,
                currency: "USD".into(),
                days_elapsed: 10,
                days_remaining: 20,
            }),
        }
    }

    #[test]
    fn eur_display_converts_every_budget_figure_and_labels_it() {
        let mut report = report_with_budget("USD");
        apply_display_currency(&mut report, "EUR", Some(1.08));
        assert_eq!(report.currency, "EUR");
        let budget = report.budget.expect("budget");
        assert_eq!(budget.currency, "EUR");
        assert!((budget.spent - 10.0).abs() < 1e-9, "{budget:?}");
        assert!((budget.remaining - 50.0).abs() < 1e-9, "{budget:?}");
        assert!((budget.projected - 20.0).abs() < 1e-9, "{budget:?}");
        assert_eq!((budget.days_elapsed, budget.days_remaining), (10, 20));
    }

    #[test]
    fn eur_display_without_a_rate_leaves_the_budget_in_usd() {
        let mut report = report_with_budget("EUR");
        apply_display_currency(&mut report, "EUR", None);
        assert_eq!(report.currency, "USD");
        let budget = report.budget.expect("budget");
        assert_eq!(budget.currency, "USD");
        assert!((budget.spent - 10.8).abs() < 1e-9, "{budget:?}");
        assert!((budget.remaining - 54.0).abs() < 1e-9, "{budget:?}");
    }
}

#[cfg(test)]
mod aggregate_tests {
    use super::*;
    use crate::store::SessionRow;
    use crate::types::{Confidence, ToolAlternative};

    fn session(id: &str) -> SessionRow {
        SessionRow {
            id: id.into(),
            project_id: "p".into(),
            created_at: 1,
            updated_at: 2,
            repository_root: None,
            active_branch: None,
            cache_namespace: String::new(),
        }
    }

    fn model(id: &str, session_id: &str, name: &str) -> ModelRow {
        ModelRow {
            id: id.into(),
            session_id: session_id.into(),
            turn_id: None,
            provider: Some("anthropic".into()),
            model: Some(name.into()),
            input_tokens: Some(10),
            output_tokens: Some(1),
            cached_input_tokens: None,
            estimated_cost: Some(0.01),
            created_at: 1,
        }
    }

    fn tool(id: &str, output_tokens: u32, cost: f64) -> ToolEconomicsRecord {
        ToolEconomicsRecord {
            id: id.into(),
            session_id: "s1".into(),
            turn_id: "t1".into(),
            tool_name: "rg".into(),
            input_hash: "h".into(),
            input_bytes: None,
            output_bytes: None,
            output_tokens,
            token_confidence: "estimated".into(),
            duration_ms: 0,
            execution_cost_usd: 0.0,
            context_cost_usd: Some(cost),
            cache_hit: false,
            repeated: false,
            alternative: None,
        }
    }

    fn optimization(id: &str, call: &str, current: u32, alternative: u32) -> OptimizationRow {
        let percent = if current == 0 {
            0.0
        } else {
            f64::from(current - alternative) * 100.0 / f64::from(current)
        };
        OptimizationRow {
            id: id.into(),
            tool_call_id: call.into(),
            accepted: false,
            alternative: ToolAlternative {
                tool_name: "ax_explore".into(),
                estimated_current_tokens: current,
                estimated_alternative_tokens: alternative,
                estimated_reduction_percent: percent,
                confidence: Confidence::High,
                reason: "graph".into(),
                rule: "rg-symbol".into(),
            },
        }
    }

    fn snapshot() -> PersistedSnapshot {
        PersistedSnapshot {
            sessions: vec![session("s1"), session("s2")],
            turns: Vec::new(),
            tools: Vec::new(),
            models: vec![
                model("m1", "s1", "claude"),
                model("m2", "s2", "claude"),
                model("m3", "s1", "gpt"),
                model("m4", "s1", "claude"),
            ],
            optimizations: Vec::new(),
            cache_entries: Vec::new(),
        }
    }

    #[test]
    fn unfiltered_report_is_labelled_as_all_sessions() {
        let report = economics_from(&snapshot(), None, "USD", None);
        assert_eq!(report.session_id, None, "the totals cover every session");
        assert_eq!(report.input_tokens, 40);
        assert!(format_economics(&report).contains("Session:\n(all sessions)"));
        let filtered = economics_from(&snapshot(), Some("s2"), "USD", None);
        assert_eq!(filtered.session_id.as_deref(), Some("s2"));
        assert_eq!(filtered.input_tokens, 10);
    }

    #[test]
    fn report_lists_each_model_once_in_first_seen_order() {
        let report = economics_from(&snapshot(), None, "USD", None);
        assert_eq!(report.models, vec!["claude".to_string(), "gpt".to_string()]);
        assert!(format_economics(&report).contains("Model:\nclaude, gpt\n"));
    }

    #[test]
    fn group_reduction_is_saved_over_current_for_the_whole_group() {
        let report = optimization_of(&[
            optimization("o1", "c1", 7423, 668),
            optimization("o2", "c2", 0, 0),
        ]);
        let group = &report.items[0];
        assert_eq!((group.calls, group.tokens), (2, 7423));
        let expected = f64::from(7423 - 668) * 100.0 / 7423.0;
        assert!(
            (group.estimated_reduction_percent - expected).abs() < 1e-9,
            "{group:?}"
        );
        assert_eq!(report.total_estimated_avoidable_tokens, 7423 - 668);
    }

    #[test]
    fn potential_cost_matches_each_optimization_to_its_tool_call() {
        let tools = [tool("c1", 1000, 0.10), tool("c2", 500, 0.20)];
        let refs: Vec<&ToolEconomicsRecord> = tools.iter().collect();
        let rows = [
            optimization("o1", "c1", 1000, 500),
            optimization("o2", "c2", 500, 0),
            optimization("o3", "missing", 900, 0),
        ];
        let cost = potential_cost(&rows, &refs).expect("a cost");
        assert!((cost - (0.05 + 0.20)).abs() < 1e-9, "{cost}");
        assert_eq!(potential_cost(&rows[2..], &refs), None);
    }
}
