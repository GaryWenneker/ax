//! Local budget math. Catalog prices stay in USD. A euro figure is shown only
//! when the config supplies `usdPerEur`. No exchange rate is assumed.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::Deserialize;
use serde_json::Value;

use crate::cost::CostConfidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Usd,
    Eur,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetMode {
    Cheap,
    Balanced,
    Quality,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetSettings {
    pub enabled: bool,
    pub monthly: f64,
    pub currency: Currency,
    /// USD per 1 EUR. Required before euro amounts are compared with catalog USD.
    pub usd_per_eur: Option<f64>,
    pub warning_percent: f64,
    pub critical_percent: f64,
    pub hard_limit_percent: f64,
    pub working_days: u32,
    pub hours_per_day: f64,
    pub mode: BudgetMode,
    /// Input USD per million tokens at or below this value is the cheap band.
    pub cheap_max_input_per_mtok: f64,
    /// Input USD per million tokens at or below this value, and above cheap, is standard.
    pub standard_max_input_per_mtok: f64,
    pub context_budget_tokens: Option<u32>,
}

impl Default for BudgetSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            monthly: 0.0,
            currency: Currency::Usd,
            usd_per_eur: None,
            warning_percent: 75.0,
            critical_percent: 90.0,
            hard_limit_percent: 100.0,
            working_days: 22,
            hours_per_day: 8.0,
            mode: BudgetMode::Balanced,
            cheap_max_input_per_mtok: 1.0,
            standard_max_input_per_mtok: 5.0,
            context_budget_tokens: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetLevel {
    Ok,
    Warning,
    Critical,
    HardLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetDecision {
    Allow,
    Warn,
    Deny,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DaySpend {
    pub date: NaiveDate,
    pub usd: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BudgetSnapshot {
    pub month: NaiveDate,
    pub today: NaiveDate,
    pub working_days: u32,
    pub elapsed_working_days: u32,
    pub remaining_working_days: u32,
    pub current_spend_usd: f64,
    pub projected_spend_usd: f64,
    pub monthly_budget_usd: Option<f64>,
    pub remaining_budget_usd: Option<f64>,
    pub recommended_daily_usd: Option<f64>,
    pub usage_percent: Option<f64>,
    pub level: BudgetLevel,
    pub decision: BudgetDecision,
    /// False when the budget is in EUR and `usdPerEur` is missing.
    pub fx_known: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DailyPlan {
    pub monthly_display: Option<f64>,
    pub currency: Currency,
    pub fx_known: bool,
    pub working_days: u32,
    pub hours_per_day: f64,
    pub daily: Option<f64>,
    pub hourly: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CycleStats {
    pub samples: usize,
    pub average: f64,
    pub p50: f64,
    pub p90: f64,
    pub p95: f64,
    pub p99: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Simulation {
    pub cycles_per_day: u32,
    pub working_days: u32,
    pub total_cycles: u64,
    pub average_cost_usd: Option<f64>,
    pub estimated_monthly_usd: Option<f64>,
    pub budget_usd: Option<f64>,
    pub difference_usd: Option<f64>,
    pub confidence: CostConfidence,
}

#[derive(Debug, Deserialize, Default)]
struct FileSettings {
    #[serde(default)]
    budget: Option<BudgetSection>,
    #[serde(default)]
    agent: Option<AgentSection>,
    #[serde(default)]
    context: Option<ContextSection>,
}

#[derive(Debug, Deserialize, Default)]
struct BudgetSection {
    enabled: Option<bool>,
    monthly: Option<f64>,
    currency: Option<String>,
    #[serde(rename = "usdPerEur")]
    usd_per_eur: Option<f64>,
    #[serde(rename = "warningPercent")]
    warning_percent: Option<f64>,
    #[serde(rename = "criticalPercent")]
    critical_percent: Option<f64>,
    #[serde(rename = "hardLimitPercent")]
    hard_limit_percent: Option<f64>,
    #[serde(rename = "workingDays")]
    working_days: Option<u32>,
    #[serde(rename = "hoursPerDay")]
    hours_per_day: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
struct AgentSection {
    #[serde(rename = "budgetMode")]
    budget_mode: Option<String>,
    #[serde(rename = "cheapMaxInputPerMtok")]
    cheap_max_input_per_mtok: Option<f64>,
    #[serde(rename = "standardMaxInputPerMtok")]
    standard_max_input_per_mtok: Option<f64>,
}

#[derive(Debug, Deserialize, Default)]
struct ContextSection {
    #[serde(rename = "budgetTokens")]
    budget_tokens: Option<u32>,
}

pub fn parse_settings(global_json: Option<&str>, project_json: Option<&str>) -> BudgetSettings {
    let mut settings = BudgetSettings::default();
    if let Some(text) = global_json {
        apply_json(&mut settings, text);
    }
    if let Some(text) = project_json {
        apply_json(&mut settings, text);
    }
    settings
}

fn apply_json(settings: &mut BudgetSettings, text: &str) {
    let Ok(file) = serde_json::from_str::<FileSettings>(text) else {
        return;
    };
    if let Some(budget) = file.budget {
        if let Some(v) = budget.enabled {
            settings.enabled = v;
        }
        if let Some(v) = budget.monthly {
            settings.monthly = v;
        }
        if let Some(v) = budget.currency {
            if let Some(currency) = parse_currency(&v) {
                settings.currency = currency;
            }
        }
        if let Some(v) = budget.usd_per_eur {
            settings.usd_per_eur = Some(v);
        }
        if let Some(v) = budget.warning_percent {
            settings.warning_percent = v;
        }
        if let Some(v) = budget.critical_percent {
            settings.critical_percent = v;
        }
        if let Some(v) = budget.hard_limit_percent {
            settings.hard_limit_percent = v;
        }
        if let Some(v) = budget.working_days {
            settings.working_days = v;
        }
        if let Some(v) = budget.hours_per_day {
            settings.hours_per_day = v;
        }
    }
    if let Some(agent) = file.agent {
        if let Some(v) = agent.budget_mode.as_deref().and_then(parse_mode) {
            settings.mode = v;
        }
        if let Some(v) = agent.cheap_max_input_per_mtok {
            settings.cheap_max_input_per_mtok = v;
        }
        if let Some(v) = agent.standard_max_input_per_mtok {
            settings.standard_max_input_per_mtok = v;
        }
    }
    if let Some(context) = file.context {
        if let Some(v) = context.budget_tokens {
            settings.context_budget_tokens = Some(v);
        }
    }
}

pub fn parse_currency(raw: &str) -> Option<Currency> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "usd" | "$" => Some(Currency::Usd),
        "eur" | "€" => Some(Currency::Eur),
        _ => None,
    }
}

pub fn parse_mode(raw: &str) -> Option<BudgetMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "cheap" => Some(BudgetMode::Cheap),
        "balanced" => Some(BudgetMode::Balanced),
        "quality" => Some(BudgetMode::Quality),
        _ => None,
    }
}

pub fn load_settings(project_root: Option<&std::path::Path>) -> BudgetSettings {
    let global = global_config_path().and_then(|p| std::fs::read_to_string(p).ok());
    let project = project_root
        .map(|root| root.join("ax.json"))
        .and_then(|p| std::fs::read_to_string(p).ok());
    parse_settings(global.as_deref(), project.as_deref())
}

pub fn global_config_path() -> Option<std::path::PathBuf> {
    ax_utils::paths::home_dir().map(|h| h.join(".ax").join("config.json"))
}

/// Write budget fields into `~/.ax/config.json` without dropping other keys.
pub fn save_global_budget(patch: &BudgetSectionPatch) -> Result<std::path::PathBuf, String> {
    let path = global_config_path().ok_or_else(|| "home directory is not available".to_string())?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut root: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| Value::Object(Default::default()));
    let obj = root
        .as_object_mut()
        .ok_or_else(|| "config root is not an object".to_string())?;
    let budget = obj
        .entry("budget")
        .or_insert_with(|| Value::Object(Default::default()));
    let budget = budget
        .as_object_mut()
        .ok_or_else(|| "budget is not an object".to_string())?;
    if let Some(v) = patch.monthly {
        budget.insert("monthly".into(), serde_json::json!(v));
    }
    if let Some(v) = patch.currency.clone() {
        budget.insert("currency".into(), Value::String(v));
    }
    if let Some(v) = patch.usd_per_eur {
        budget.insert("usdPerEur".into(), serde_json::json!(v));
    }
    if let Some(v) = patch.working_days {
        budget.insert("workingDays".into(), serde_json::json!(v));
    }
    if let Some(v) = patch.hours_per_day {
        budget.insert("hoursPerDay".into(), serde_json::json!(v));
    }
    if let Some(v) = patch.warning_percent {
        budget.insert("warningPercent".into(), serde_json::json!(v));
    }
    if let Some(v) = patch.critical_percent {
        budget.insert("criticalPercent".into(), serde_json::json!(v));
    }
    if let Some(v) = patch.hard_limit_percent {
        budget.insert("hardLimitPercent".into(), serde_json::json!(v));
    }
    budget.insert("enabled".into(), Value::Bool(true));
    std::fs::write(
        &path,
        serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(path)
}

#[derive(Debug, Clone, Default)]
pub struct BudgetSectionPatch {
    pub monthly: Option<f64>,
    pub currency: Option<String>,
    pub usd_per_eur: Option<f64>,
    pub working_days: Option<u32>,
    pub hours_per_day: Option<f64>,
    pub warning_percent: Option<f64>,
    pub critical_percent: Option<f64>,
    pub hard_limit_percent: Option<f64>,
}

pub fn monthly_budget_usd(settings: &BudgetSettings) -> Option<f64> {
    if !settings.enabled || settings.monthly < 0.0 {
        return None;
    }
    match settings.currency {
        Currency::Usd => Some(settings.monthly),
        Currency::Eur => {
            let rate = settings.usd_per_eur.filter(|r| r.is_finite() && *r > 0.0)?;
            Some(settings.monthly * rate)
        }
    }
}

pub fn fx_known(settings: &BudgetSettings) -> bool {
    match settings.currency {
        Currency::Usd => true,
        Currency::Eur => settings
            .usd_per_eur
            .is_some_and(|r| r.is_finite() && r > 0.0),
    }
}

pub fn working_schedule(month_start: NaiveDate, working_days: u32) -> Vec<NaiveDate> {
    let mut cursor = month_start;
    let mut days = Vec::new();
    let limit = if working_days == 0 {
        u32::MAX
    } else {
        working_days
    };
    while cursor.month() == month_start.month() && cursor.year() == month_start.year() {
        if is_weekday(cursor) {
            days.push(cursor);
            if days.len() as u32 >= limit {
                break;
            }
        }
        cursor += Duration::days(1);
    }
    days
}

fn is_weekday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

pub fn project_budget(
    settings: &BudgetSettings,
    month: NaiveDate,
    today: NaiveDate,
    spend: &[DaySpend],
) -> BudgetSnapshot {
    let schedule = working_schedule(month, settings.working_days);
    let elapsed: Vec<NaiveDate> = schedule.iter().copied().filter(|d| *d <= today).collect();
    let elapsed_n = elapsed.len() as u32;
    let remaining_n = (schedule.len() as u32).saturating_sub(elapsed_n);
    let month_end = last_day(month);
    let current = spend
        .iter()
        .filter(|d| d.date >= month && d.date <= today.min(month_end))
        .map(|d| d.usd)
        .sum();
    let trail: Vec<NaiveDate> = elapsed.iter().rev().take(5).copied().collect();
    let trail_avg = if trail.is_empty() {
        0.0
    } else {
        trail.iter().map(|d| spend_on(spend, *d)).sum::<f64>() / trail.len() as f64
    };
    let projected = if elapsed_n == 0 {
        0.0
    } else if remaining_n == 0 {
        current
    } else {
        let run_rate = current / f64::from(elapsed_n) * schedule.len() as f64;
        let trail_month = current + trail_avg * f64::from(remaining_n);
        0.5 * run_rate + 0.5 * trail_month
    };
    let budget = monthly_budget_usd(settings);
    let known = fx_known(settings);
    let (remaining, recommended, percent, level, decision) = match budget.filter(|_| known) {
        Some(budget_usd) => {
            let remaining = budget_usd - current;
            let recommended = if remaining_n == 0 {
                0.0
            } else {
                remaining / f64::from(remaining_n)
            };
            let percent = if budget_usd <= 0.0 {
                None
            } else {
                Some(current / budget_usd * 100.0)
            };
            let (level, decision) = level_for(percent, settings);
            (Some(remaining), Some(recommended), percent, level, decision)
        }
        None => (None, None, None, BudgetLevel::Ok, BudgetDecision::Allow),
    };
    BudgetSnapshot {
        month,
        today,
        working_days: schedule.len() as u32,
        elapsed_working_days: elapsed_n,
        remaining_working_days: remaining_n,
        current_spend_usd: current,
        projected_spend_usd: projected,
        monthly_budget_usd: budget,
        remaining_budget_usd: remaining,
        recommended_daily_usd: recommended,
        usage_percent: percent,
        level,
        decision,
        fx_known: known,
    }
}

fn spend_on(spend: &[DaySpend], date: NaiveDate) -> f64 {
    spend.iter().filter(|d| d.date == date).map(|d| d.usd).sum()
}

fn last_day(month_start: NaiveDate) -> NaiveDate {
    let next = if month_start.month() == 12 {
        NaiveDate::from_ymd_opt(month_start.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(month_start.year(), month_start.month() + 1, 1)
    };
    next.unwrap_or(month_start) - Duration::days(1)
}

pub fn level_for(percent: Option<f64>, settings: &BudgetSettings) -> (BudgetLevel, BudgetDecision) {
    let Some(percent) = percent else {
        return (BudgetLevel::Ok, BudgetDecision::Allow);
    };
    if !settings.enabled {
        return (BudgetLevel::Ok, BudgetDecision::Allow);
    }
    if percent >= settings.hard_limit_percent {
        (BudgetLevel::HardLimit, BudgetDecision::Deny)
    } else if percent >= settings.critical_percent {
        (BudgetLevel::Critical, BudgetDecision::Warn)
    } else if percent >= settings.warning_percent {
        (BudgetLevel::Warning, BudgetDecision::Warn)
    } else {
        (BudgetLevel::Ok, BudgetDecision::Allow)
    }
}

pub fn daily_plan(settings: &BudgetSettings) -> DailyPlan {
    let known = fx_known(settings);
    let monthly = if known { Some(settings.monthly) } else { None };
    let days = settings.working_days.max(1);
    let daily = monthly.map(|m| m / f64::from(days));
    let hourly = match (daily, settings.hours_per_day) {
        (Some(d), hours) if hours > 0.0 => Some(d / hours),
        _ => None,
    };
    DailyPlan {
        monthly_display: monthly,
        currency: settings.currency,
        fx_known: known,
        working_days: settings.working_days,
        hours_per_day: settings.hours_per_day,
        daily,
        hourly,
    }
}

/// Nearest-rank percentile on a sorted copy. Empty input returns `None`.
pub fn cycle_stats(costs: &[f64]) -> Option<CycleStats> {
    let mut values: Vec<f64> = costs
        .iter()
        .copied()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .collect();
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    let average = values.iter().sum::<f64>() / n as f64;
    Some(CycleStats {
        samples: n,
        average,
        p50: rank(&values, 0.50),
        p90: rank(&values, 0.90),
        p95: rank(&values, 0.95),
        p99: rank(&values, 0.99),
    })
}

fn rank(sorted: &[f64], p: f64) -> f64 {
    let idx = ((sorted.len() - 1) as f64 * p).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

pub fn simulate(
    cycles_per_day: u32,
    working_days: u32,
    average_cost_usd: Option<f64>,
    budget_usd: Option<f64>,
) -> Simulation {
    let total = u64::from(cycles_per_day) * u64::from(working_days);
    let estimated = average_cost_usd
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|avg| avg * total as f64);
    let difference = match (estimated, budget_usd) {
        (Some(est), Some(budget)) => Some(est - budget),
        _ => None,
    };
    let confidence = if estimated.is_some() {
        CostConfidence::Estimated
    } else {
        CostConfidence::Unknown
    };
    Simulation {
        cycles_per_day,
        working_days,
        total_cycles: total,
        average_cost_usd,
        estimated_monthly_usd: estimated,
        budget_usd,
        difference_usd: difference,
        confidence,
    }
}

pub fn display_amount(usd: f64, settings: &BudgetSettings) -> Option<f64> {
    match settings.currency {
        Currency::Usd => Some(usd),
        Currency::Eur => {
            let rate = settings.usd_per_eur.filter(|r| r.is_finite() && *r > 0.0)?;
            Some(usd / rate)
        }
    }
}

pub fn currency_label(settings: &BudgetSettings) -> &'static str {
    match settings.currency {
        Currency::Usd => "USD",
        Currency::Eur => "EUR",
    }
}

pub fn format_money(usd: f64, settings: &BudgetSettings) -> String {
    match display_amount(usd, settings) {
        Some(amount) => match settings.currency {
            Currency::Usd => format!("${amount:.2}"),
            Currency::Eur => format!("€{amount:.2}"),
        },
        None => "unknown".to_string(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceBand {
    Cheap,
    Standard,
    Premium,
}

pub fn price_band(input_per_mtok: f64, settings: &BudgetSettings) -> PriceBand {
    if input_per_mtok <= settings.cheap_max_input_per_mtok {
        PriceBand::Cheap
    } else if input_per_mtok <= settings.standard_max_input_per_mtok {
        PriceBand::Standard
    } else {
        PriceBand::Premium
    }
}

pub fn banner(snapshot: &BudgetSnapshot, settings: &BudgetSettings) -> Option<String> {
    if !settings.enabled {
        return None;
    }
    if !snapshot.fx_known {
        return Some("AX BUDGET\n\nBudget currency is EUR and usdPerEur is not set. Spend stays in USD and is not compared with the budget.\n\nThis note does not block a provider call.".into());
    }
    let budget = format_money(snapshot.monthly_budget_usd.unwrap_or(0.0), settings);
    let projected = format_money(snapshot.projected_spend_usd, settings);
    let pct = snapshot.usage_percent.unwrap_or(0.0);
    match snapshot.level {
        BudgetLevel::Warning => Some(format!(
            "AX BUDGET WARNING\n\n{pct:.0}% of monthly budget consumed.\nProjected spend: {projected}\nBudget: {budget}\n\nThis note does not block a provider call."
        )),
        BudgetLevel::Critical => Some(format!(
            "AX BUDGET CRITICAL\n\n{pct:.0}% of monthly budget consumed.\n\nConsider switching to a cheaper model or reducing context.\n\nThis note does not block a provider call."
        )),
        BudgetLevel::HardLimit => Some(format!(
            "AX BUDGET HARD LIMIT\n\n{pct:.0}% of monthly budget consumed.\nBudget: {budget}\n\nAx has not blocked a provider call. An integration that asks ax_budget receives deny."
        )),
        BudgetLevel::Ok => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled(monthly: f64) -> BudgetSettings {
        BudgetSettings {
            enabled: true,
            monthly,
            currency: Currency::Usd,
            ..BudgetSettings::default()
        }
    }

    fn nov() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, 1).unwrap()
    }

    #[test]
    fn zero_spend_at_the_start_of_a_month_that_opens_on_sunday() {
        let snap = project_budget(&enabled(100.0), nov(), nov(), &[]);
        assert_eq!(snap.elapsed_working_days, 0);
        assert_eq!(snap.current_spend_usd, 0.0);
        assert_eq!(snap.projected_spend_usd, 0.0);
        assert_eq!(snap.usage_percent, Some(0.0));
        assert_eq!(snap.decision, BudgetDecision::Allow);
        assert_eq!(snap.level, BudgetLevel::Ok);
    }

    #[test]
    fn thresholds_at_50_75_90_100_and_over() {
        let cases = [
            (50.0, BudgetLevel::Ok, BudgetDecision::Allow),
            (75.0, BudgetLevel::Warning, BudgetDecision::Warn),
            (90.0, BudgetLevel::Critical, BudgetDecision::Warn),
            (100.0, BudgetLevel::HardLimit, BudgetDecision::Deny),
            (130.0, BudgetLevel::HardLimit, BudgetDecision::Deny),
        ];
        for (spend, level, decision) in cases {
            let today = NaiveDate::from_ymd_opt(2026, 11, 16).unwrap();
            let snap = project_budget(
                &enabled(100.0),
                nov(),
                today,
                &[DaySpend {
                    date: today,
                    usd: spend,
                }],
            );
            assert_eq!(snap.level, level, "spend {spend}");
            assert_eq!(snap.decision, decision, "spend {spend}");
        }
    }

    #[test]
    fn middle_of_month_blends_run_rate_and_trailing_average() {
        let settings = BudgetSettings {
            working_days: 2,
            ..enabled(100.0)
        };
        let first = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let snap = project_budget(
            &settings,
            nov(),
            first,
            &[DaySpend {
                date: first,
                usd: 10.0,
            }],
        );
        assert_eq!(snap.elapsed_working_days, 1);
        assert_eq!(snap.remaining_working_days, 1);
        assert!((snap.projected_spend_usd - 20.0).abs() < 1e-9);
    }

    #[test]
    fn end_of_schedule_projects_the_spend_already_recorded() {
        let settings = BudgetSettings {
            working_days: 1,
            ..enabled(100.0)
        };
        let day = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let snap = project_budget(
            &settings,
            nov(),
            day,
            &[DaySpend {
                date: day,
                usd: 12.0,
            }],
        );
        assert_eq!(snap.remaining_working_days, 0);
        assert!((snap.projected_spend_usd - 12.0).abs() < 1e-9);
    }

    #[test]
    fn eur_without_a_rate_does_not_invent_a_comparison() {
        let settings = BudgetSettings {
            enabled: true,
            monthly: 60.0,
            currency: Currency::Eur,
            usd_per_eur: None,
            ..BudgetSettings::default()
        };
        let snap = project_budget(&settings, nov(), nov(), &[]);
        assert!(!snap.fx_known);
        assert_eq!(snap.monthly_budget_usd, None);
        assert_eq!(snap.decision, BudgetDecision::Allow);
        assert_eq!(format_money(10.0, &settings), "unknown");
    }

    #[test]
    fn eur_uses_the_configured_rate_only() {
        let settings = BudgetSettings {
            enabled: true,
            monthly: 60.0,
            currency: Currency::Eur,
            usd_per_eur: Some(1.08),
            ..BudgetSettings::default()
        };
        assert!((monthly_budget_usd(&settings).unwrap() - 64.8).abs() < 1e-9);
        assert!((display_amount(1.08, &settings).unwrap() - 1.0).abs() < 1e-9);
        let plan = daily_plan(&settings);
        assert!((plan.daily.unwrap() - (60.0 / 22.0)).abs() < 1e-9);
        assert!((plan.hourly.unwrap() - (60.0 / 22.0 / 8.0)).abs() < 1e-9);
    }

    #[test]
    fn percentiles_and_simulation_scale_cycles() {
        let stats = cycle_stats(&[1.0, 2.0, 3.0, 4.0, 10.0]).unwrap();
        assert!((stats.p50 - 3.0).abs() < 1e-9);
        assert!(stats.p99 >= stats.p95 && stats.p95 >= stats.p90);
        let sim = simulate(500, 22, Some(stats.average), Some(60.0));
        assert_eq!(sim.total_cycles, 11_000);
        assert_eq!(sim.confidence, CostConfidence::Estimated);
        assert!(sim.estimated_monthly_usd.unwrap() > 60.0);
        let unknown = simulate(50, 22, None, Some(60.0));
        assert_eq!(unknown.confidence, CostConfidence::Unknown);
        assert_eq!(unknown.estimated_monthly_usd, None);
        assert_eq!(simulate(100, 22, Some(1.0), Some(60.0)).total_cycles, 2_200);
        assert_eq!(simulate(200, 22, Some(1.0), Some(60.0)).total_cycles, 4_400);
    }

    #[test]
    fn bands_follow_configured_prices_not_model_names() {
        let settings = BudgetSettings::default();
        assert_eq!(price_band(0.2, &settings), PriceBand::Cheap);
        assert_eq!(price_band(3.0, &settings), PriceBand::Standard);
        assert_eq!(price_band(15.0, &settings), PriceBand::Premium);
    }

    #[test]
    fn project_json_overrides_global_budget() {
        let global = r#"{"budget":{"enabled":true,"monthly":10,"currency":"USD"}}"#;
        let project = r#"{"budget":{"monthly":60,"currency":"EUR","usdPerEur":1.1},"agent":{"budgetMode":"cheap"},"context":{"budgetTokens":12000}}"#;
        let settings = parse_settings(Some(global), Some(project));
        assert!(settings.enabled);
        assert!((settings.monthly - 60.0).abs() < 1e-9);
        assert_eq!(settings.currency, Currency::Eur);
        assert_eq!(settings.mode, BudgetMode::Cheap);
        assert_eq!(settings.context_budget_tokens, Some(12_000));
    }
}
