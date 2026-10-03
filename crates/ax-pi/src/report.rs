//! Text and JSON economics reports. Figures are estimates.

use crate::economics::display_amount;
use crate::store::{ModelRow, OptimizationRow, PersistedSnapshot, TurnRow};
use crate::types::{BudgetSnapshot, ToolEconomicsRecord};
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Serialize)]
pub struct EconomicsReport {
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

pub fn economics_from(
    snapshot: &PersistedSnapshot,
    session_filter: Option<&str>,
    currency: &str,
    budget: Option<BudgetSnapshot>,
) -> EconomicsReport {
    let tools = filter_tools(&snapshot.tools, session_filter);
    let turns = filter_turns(&snapshot.turns, session_filter);
    let models = filter_models(&snapshot.models, session_filter);
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
    let session_id = session_filter
        .map(str::to_string)
        .or_else(|| snapshot.sessions.last().map(|session| session.id.clone()));
    EconomicsReport {
        session_id,
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
        potential_optimization_cost: potential_cost(&snapshot.optimizations, &tools),
        largest_tool,
        currency: currency.to_string(),
        budget,
    }
}

pub fn optimization_from(snapshot: &PersistedSnapshot) -> OptimizationReport {
    let mut groups: BTreeMap<String, OptimizationGroup> = BTreeMap::new();
    let mut total = 0u64;
    for row in &snapshot.optimizations {
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
            report.session_id.as_deref().unwrap_or("(none)")
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
}
