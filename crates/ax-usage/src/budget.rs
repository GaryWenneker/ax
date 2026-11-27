//! Local budget math. Catalog prices stay in USD. A euro figure is shown only
//! when the config supplies `usdPerEur`. No exchange rate is assumed.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
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
    /// Preflight tokens kept for the memories block. `None` uses the default reserve.
    pub context_memory_tokens: Option<u32>,
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
            context_memory_tokens: None,
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

/// Strips a leading UTF-8 byte order mark, which `serde_json` rejects.
fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Parses the bytes of a JSON config file. A leading UTF-8 BOM is skipped, and
/// a file that is empty or holds only whitespace is `{}`. Every writer of a
/// shared config file reads it through this, so they agree on what is valid.
pub(crate) fn parse_config_json(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|e| e.to_string())?;
    let text = strip_bom(text);
    if text
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .is_empty()
    {
        // An empty file (made with `touch`) has no settings yet.
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_str::<Value>(text).map_err(|e| e.to_string())
}

/// Reads a number field. Numeric strings such as `"60"` are accepted.
fn number_field(section: &Value, key: &str, path: &str, diags: &mut Vec<String>) -> Option<f64> {
    let raw = section.get(key)?;
    let parsed = match raw {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        Value::Null => return None,
        _ => None,
    };
    match parsed.filter(|v| v.is_finite()) {
        Some(v) => Some(v),
        None => {
            diags.push(format!("{path}.{key} is not a number ({raw}); ignored"));
            None
        }
    }
}

/// Allowed range for a number field: lower bound, whether the lower bound is
/// excluded, upper bound (included).
type Range = (f64, bool, f64);

fn ranged_field(
    section: &Value,
    key: &str,
    path: &str,
    (min, min_open, max): Range,
    diags: &mut Vec<String>,
) -> Option<f64> {
    let v = number_field(section, key, path, diags)?;
    let low_ok = if min_open { v > min } else { v >= min };
    if low_ok && v <= max {
        Some(v)
    } else {
        let open = if min_open { "(" } else { "[" };
        diags.push(format!(
            "{path}.{key} = {v} is outside {open}{min}, {max}]; ignored"
        ));
        None
    }
}

fn whole_field(
    section: &Value,
    key: &str,
    path: &str,
    max: u32,
    diags: &mut Vec<String>,
) -> Option<u32> {
    let v = number_field(section, key, path, diags)?;
    if v.fract() == 0.0 && v >= 0.0 && v <= f64::from(max) {
        Some(v as u32)
    } else {
        diags.push(format!(
            "{path}.{key} = {v} is not a whole number in [0, {max}]; ignored"
        ));
        None
    }
}

fn string_field(section: &Value, key: &str, path: &str, diags: &mut Vec<String>) -> Option<String> {
    match section.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Null => None,
        other => {
            diags.push(format!("{path}.{key} is not a string ({other}); ignored"));
            None
        }
    }
}

/// Which percent thresholds a config file set explicitly.
#[derive(Debug, Default, Clone, Copy)]
struct ExplicitThresholds {
    warning: bool,
    critical: bool,
}

/// Parses budget settings. Each field is read on its own, so one wrong value
/// keeps the rest of the file; every ignored value is named in the returned
/// diagnostics. Thresholds must hold `warning <= critical <= hardLimit`: a
/// value above a higher threshold is lowered to it, so the hard limit is never
/// raised. Lowering a default under a threshold the user set is silent;
/// lowering a value the user set is named in the diagnostics.
pub fn parse_settings_with_diagnostics(
    global_json: Option<&str>,
    project_json: Option<&str>,
) -> (BudgetSettings, Vec<String>) {
    let mut settings = BudgetSettings::default();
    let mut diags = Vec::new();
    let mut explicit = ExplicitThresholds::default();
    for (label, text) in [("global config", global_json), ("ax.json", project_json)] {
        if let Some(text) = text {
            apply_json(&mut settings, &mut explicit, label, text, &mut diags);
        }
    }
    let (warning, critical, hard) = (
        settings.warning_percent,
        settings.critical_percent,
        settings.hard_limit_percent,
    );
    settings.critical_percent = critical.min(hard);
    settings.warning_percent = warning.min(settings.critical_percent);
    let lowered_explicit = (explicit.critical && settings.critical_percent != critical)
        || (explicit.warning && settings.warning_percent != warning);
    if lowered_explicit {
        diags.push(format!(
            "budget thresholds are out of order (warning {warning} <= critical {critical} <= hardLimit {hard} is required); lowered to warning {} critical {} hardLimit {hard}",
            settings.warning_percent, settings.critical_percent
        ));
    }
    (settings, diags)
}

/// Same as [`parse_settings_with_diagnostics`]; each diagnostic is logged as a warning.
pub fn parse_settings(global_json: Option<&str>, project_json: Option<&str>) -> BudgetSettings {
    let (settings, diags) = parse_settings_with_diagnostics(global_json, project_json);
    for diag in &diags {
        tracing::warn!(target: "ax_usage::budget", "{diag}");
    }
    settings
}

fn apply_json(
    settings: &mut BudgetSettings,
    explicit: &mut ExplicitThresholds,
    label: &str,
    text: &str,
    diags: &mut Vec<String>,
) {
    let text = strip_bom(text);
    if text
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .is_empty()
    {
        // An empty file has no settings, as when saving into it.
        return;
    }
    let file: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            diags.push(format!(
                "{label} is not valid JSON ({e}); its settings are ignored"
            ));
            return;
        }
    };
    const PERCENT: Range = (0.0, true, 1_000.0);
    const NON_NEGATIVE: Range = (0.0, false, f64::MAX);
    const POSITIVE: Range = (0.0, true, f64::MAX);
    if let Some(budget) = file.get("budget").filter(|v| v.is_object()) {
        let p = "budget";
        match budget.get("enabled") {
            Some(Value::Bool(v)) => settings.enabled = *v,
            Some(Value::Null) | None => {}
            Some(other) => diags.push(format!(
                "budget.enabled is not true/false ({other}); ignored"
            )),
        }
        if let Some(v) = ranged_field(budget, "monthly", p, NON_NEGATIVE, diags) {
            settings.monthly = v;
        }
        if let Some(v) = string_field(budget, "currency", p, diags) {
            match parse_currency(&v) {
                Some(currency) => settings.currency = currency,
                None => diags.push(format!(
                    "budget.currency = {v:?} is not USD or EUR; ignored"
                )),
            }
        }
        if let Some(v) = ranged_field(budget, "usdPerEur", p, POSITIVE, diags) {
            settings.usd_per_eur = Some(v);
        }
        if let Some(v) = ranged_field(budget, "warningPercent", p, PERCENT, diags) {
            settings.warning_percent = v;
            explicit.warning = true;
        }
        if let Some(v) = ranged_field(budget, "criticalPercent", p, PERCENT, diags) {
            settings.critical_percent = v;
            explicit.critical = true;
        }
        if let Some(v) = ranged_field(budget, "hardLimitPercent", p, PERCENT, diags) {
            settings.hard_limit_percent = v;
        }
        if let Some(v) = whole_field(budget, "workingDays", p, 31, diags) {
            settings.working_days = v;
        }
        if let Some(v) = ranged_field(budget, "hoursPerDay", p, (0.0, true, 24.0), diags) {
            settings.hours_per_day = v;
        }
    }
    if let Some(agent) = file.get("agent").filter(|v| v.is_object()) {
        let p = "agent";
        if let Some(v) = string_field(agent, "budgetMode", p, diags) {
            match parse_mode(&v) {
                Some(mode) => settings.mode = mode,
                None => diags.push(format!(
                    "agent.budgetMode = {v:?} is not cheap, balanced or quality; ignored"
                )),
            }
        }
        if let Some(v) = ranged_field(agent, "cheapMaxInputPerMtok", p, NON_NEGATIVE, diags) {
            settings.cheap_max_input_per_mtok = v;
        }
        if let Some(v) = ranged_field(agent, "standardMaxInputPerMtok", p, NON_NEGATIVE, diags) {
            settings.standard_max_input_per_mtok = v;
        }
    }
    if let Some(context) = file.get("context").filter(|v| v.is_object()) {
        let p = "context";
        if let Some(v) = whole_field(context, "budgetTokens", p, u32::MAX, diags) {
            settings.context_budget_tokens = Some(v);
        }
        if let Some(v) = whole_field(context, "memoryTokens", p, u32::MAX, diags) {
            settings.context_memory_tokens = Some(v);
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
    let (settings, diags) = load_settings_with_diagnostics(project_root);
    for diag in &diags {
        tracing::warn!(target: "ax_usage::budget", "{diag}");
    }
    settings
}

/// Loads `~/.ax/config.json` and the project's `ax.json`, and names every
/// value that was ignored.
pub fn load_settings_with_diagnostics(
    project_root: Option<&std::path::Path>,
) -> (BudgetSettings, Vec<String>) {
    let global = global_config_path().and_then(|p| std::fs::read_to_string(p).ok());
    let project = project_root
        .map(|root| root.join("ax.json"))
        .and_then(|p| std::fs::read_to_string(p).ok());
    parse_settings_with_diagnostics(global.as_deref(), project.as_deref())
}

pub fn global_config_path() -> Option<std::path::PathBuf> {
    ax_utils::paths::home_dir().map(|h| h.join(".ax").join("config.json"))
}

/// Write budget fields into `~/.ax/config.json` without dropping other keys.
pub fn save_global_budget(patch: &BudgetSectionPatch) -> Result<std::path::PathBuf, String> {
    let path = global_config_path().ok_or_else(|| "home directory is not available".to_string())?;
    save_budget_to(&path, patch)
}

fn save_budget_to(
    path: &std::path::Path,
    patch: &BudgetSectionPatch,
) -> Result<std::path::PathBuf, String> {
    edit_json_file(path, |obj| apply_budget_patch(obj, patch))?;
    Ok(path.to_path_buf())
}

/// Read-modify-write of a JSON config file under a lock, written atomically.
/// A missing or empty file starts as `{}`. A file that cannot be read or parsed is an
/// error and stays untouched, so other settings in it are never lost.
fn edit_json_file<F>(path: &std::path::Path, edit: F) -> Result<(), String>
where
    F: FnOnce(&mut serde_json::Map<String, Value>) -> Result<(), String>,
{
    let shown = path.display().to_string();
    let mut failure: Option<String> = None;
    let result = ax_utils::update_locked(path, |current| {
        let mut root = match current {
            None => Value::Object(Default::default()),
            Some(bytes) => match parse_config_json(&bytes) {
                Ok(v) => v,
                Err(e) => {
                    failure = Some(format!(
                            "{shown} is not valid JSON ({e}); fix or remove it first, it was left unchanged"
                        ));
                    return Ok(None);
                }
            },
        };
        let Some(obj) = root.as_object_mut() else {
            failure = Some(format!(
                "{shown}: root is not an object; it was left unchanged"
            ));
            return Ok(None);
        };
        if let Err(e) = edit(obj) {
            failure = Some(format!("{shown}: {e}; it was left unchanged"));
            return Ok(None);
        }
        let text = serde_json::to_string_pretty(&root)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        Ok(Some((text + "\n").into_bytes()))
    });
    if let Some(msg) = failure {
        return Err(msg);
    }
    result.map(|_| ()).map_err(|e| format!("{shown}: {e}"))
}

fn apply_budget_patch(
    obj: &mut serde_json::Map<String, Value>,
    patch: &BudgetSectionPatch,
) -> Result<(), String> {
    {
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
        check_threshold_order(budget)?;
    }
    if let Some(v) = patch.mode.clone() {
        let agent = obj
            .entry("agent")
            .or_insert_with(|| Value::Object(Default::default()));
        let agent = agent
            .as_object_mut()
            .ok_or_else(|| "agent is not an object".to_string())?;
        agent.insert("budgetMode".into(), Value::String(v));
    }
    if let Some(v) = patch.context_budget_tokens {
        let context = obj
            .entry("context")
            .or_insert_with(|| Value::Object(Default::default()));
        let context = context
            .as_object_mut()
            .ok_or_else(|| "context is not an object".to_string())?;
        context.insert("budgetTokens".into(), serde_json::json!(v));
    }
    Ok(())
}

/// Refuses a budget section whose explicitly set thresholds are not
/// `warning <= critical <= hardLimit`. Unset thresholds are not checked: the
/// reader lowers a default to fit under a set one.
fn check_threshold_order(budget: &serde_json::Map<String, Value>) -> Result<(), String> {
    let keys = ["warningPercent", "criticalPercent", "hardLimitPercent"];
    let set: Vec<(&str, f64)> = keys
        .iter()
        .filter_map(|k| budget.get(*k).and_then(Value::as_f64).map(|v| (*k, v)))
        .collect();
    for pair in set.windows(2) {
        let ((low_key, low), (high_key, high)) = (pair[0], pair[1]);
        if low > high {
            return Err(format!(
                "budget thresholds would be out of order ({low_key} {low} > {high_key} {high}); warning <= critical <= hardLimit is required"
            ));
        }
    }
    Ok(())
}

/// Write `context.budgetTokens` into the project's `ax.json` and leave every other key alone.
pub fn save_project_context_budget(
    project_root: &std::path::Path,
    tokens: u32,
) -> Result<std::path::PathBuf, String> {
    let path = project_root.join("ax.json");
    edit_json_file(&path, |obj| {
        let context = obj
            .entry("context")
            .or_insert_with(|| Value::Object(Default::default()));
        let context = context
            .as_object_mut()
            .ok_or_else(|| "context is not an object".to_string())?;
        context.insert("budgetTokens".into(), serde_json::json!(tokens));
        Ok(())
    })?;
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
    pub mode: Option<String>,
    pub context_budget_tokens: Option<u32>,
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
        assert_eq!(settings.context_memory_tokens, None);
    }

    #[test]
    fn project_context_memory_tokens_override_the_default() {
        let project = r#"{"context":{"memoryTokens":80}}"#;
        let settings = parse_settings(None, Some(project));
        assert_eq!(settings.context_memory_tokens, Some(80));
    }

    #[test]
    fn one_wrongly_typed_field_keeps_the_rest_of_the_file() {
        let text = r#"{"budget":{"enabled":true,"monthly":"60","workingDays":"twenty"},"agent":{"budgetMode":"cheap"},"context":{"budgetTokens":9000}}"#;
        let (settings, diagnostics) = parse_settings_with_diagnostics(None, Some(text));
        assert!(settings.enabled);
        assert!(
            (settings.monthly - 60.0).abs() < 1e-9,
            "numeric string accepted"
        );
        assert_eq!(settings.working_days, 22, "bad value keeps the default");
        assert_eq!(settings.mode, BudgetMode::Cheap);
        assert_eq!(settings.context_budget_tokens, Some(9_000));
        assert!(
            diagnostics.iter().any(|d| d.contains("budget.workingDays")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn thresholds_out_of_range_are_ignored_and_out_of_order_ones_are_lowered() {
        let zero = r#"{"budget":{"enabled":true,"monthly":100,"hardLimitPercent":0}}"#;
        let (settings, diagnostics) = parse_settings_with_diagnostics(None, Some(zero));
        assert_eq!(settings.hard_limit_percent, 100.0);
        assert!(diagnostics
            .iter()
            .any(|d| d.contains("budget.hardLimitPercent")));
        assert_eq!(
            level_for(Some(0.0), &settings),
            (BudgetLevel::Ok, BudgetDecision::Allow)
        );

        let unordered =
            r#"{"budget":{"warningPercent":95,"criticalPercent":80,"hardLimitPercent":90}}"#;
        let (settings, diagnostics) = parse_settings_with_diagnostics(None, Some(unordered));
        assert_eq!(
            (
                settings.warning_percent,
                settings.critical_percent,
                settings.hard_limit_percent
            ),
            (80.0, 80.0, 90.0)
        );
        assert!(
            diagnostics.iter().any(|d| d.contains("order")),
            "{diagnostics:?}"
        );

        let hours = r#"{"budget":{"hoursPerDay":30,"workingDays":40}}"#;
        let (settings, diagnostics) = parse_settings_with_diagnostics(None, Some(hours));
        assert_eq!(settings.hours_per_day, 8.0);
        assert_eq!(settings.working_days, 22);
        assert_eq!(diagnostics.len(), 2, "{diagnostics:?}");
    }

    #[test]
    fn a_lower_hard_limit_alone_is_kept_and_lowers_the_defaults_under_it() {
        let text = r#"{"budget":{"enabled":true,"monthly":100,"hardLimitPercent":80}}"#;
        let (settings, diagnostics) = parse_settings_with_diagnostics(None, Some(text));
        assert_eq!(
            (
                settings.warning_percent,
                settings.critical_percent,
                settings.hard_limit_percent
            ),
            (75.0, 80.0, 80.0)
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            level_for(Some(85.0), &settings),
            (BudgetLevel::HardLimit, BudgetDecision::Deny)
        );

        let low = r#"{"budget":{"hardLimitPercent":50}}"#;
        let (settings, diagnostics) = parse_settings_with_diagnostics(None, Some(low));
        assert_eq!(
            (
                settings.warning_percent,
                settings.critical_percent,
                settings.hard_limit_percent
            ),
            (50.0, 50.0, 50.0)
        );
        assert!(diagnostics.is_empty(), "{diagnostics:?}");

        // A global hard limit stays when the project only sets a warning.
        let global = r#"{"budget":{"hardLimitPercent":80}}"#;
        let project = r#"{"budget":{"warningPercent":60}}"#;
        let (settings, _) = parse_settings_with_diagnostics(Some(global), Some(project));
        assert_eq!(
            (
                settings.warning_percent,
                settings.critical_percent,
                settings.hard_limit_percent
            ),
            (60.0, 80.0, 80.0)
        );
    }

    #[test]
    fn saving_out_of_order_thresholds_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"budget":{"criticalPercent":70}}"#).unwrap();
        let patch = BudgetSectionPatch {
            hard_limit_percent: Some(60.0),
            ..BudgetSectionPatch::default()
        };
        let err = save_budget_to(&path, &patch).unwrap_err();
        assert!(err.contains("order"), "{err}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"budget":{"criticalPercent":70}}"#
        );
        let ok = BudgetSectionPatch {
            hard_limit_percent: Some(80.0),
            ..BudgetSectionPatch::default()
        };
        save_budget_to(&path, &ok).unwrap();
        // Setting only a hard limit under the default critical is allowed.
        let fresh = dir.path().join("fresh.json");
        save_budget_to(&fresh, &patch).unwrap();
    }

    #[test]
    fn a_file_with_a_bom_is_read() {
        let text = "\u{feff}{\"budget\":{\"enabled\":true,\"monthly\":5}}";
        let settings = parse_settings(None, Some(text));
        assert!(settings.enabled);
        assert!((settings.monthly - 5.0).abs() < 1e-9);
    }

    #[test]
    fn saving_refuses_to_replace_an_unparseable_ax_json() {
        let dir = tempfile::tempdir().unwrap();
        let broken = "{\"name\":\"keep\",\"other\":1,}";
        std::fs::write(dir.path().join("ax.json"), broken).unwrap();
        let err = save_project_context_budget(dir.path(), 8_000).unwrap_err();
        assert!(err.contains("ax.json"), "{err}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("ax.json")).unwrap(),
            broken
        );
    }

    #[test]
    fn saving_keeps_keys_of_a_file_with_a_bom_and_writes_without_one() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("ax.json"), "\u{feff}{\"name\":\"keep\"}").unwrap();
        save_project_context_budget(dir.path(), 8_000).unwrap();
        let text = std::fs::read_to_string(dir.path().join("ax.json")).unwrap();
        assert!(!text.starts_with('\u{feff}'));
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["name"], "keep");
        assert_eq!(v["context"]["budgetTokens"], 8_000);
    }

    #[test]
    fn saving_global_budget_refuses_an_unparseable_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, "{\"web\":{\"port\":1},").unwrap();
        let patch = BudgetSectionPatch {
            monthly: Some(60.0),
            ..BudgetSectionPatch::default()
        };
        assert!(save_budget_to(&path, &patch).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"web\":{\"port\":1},"
        );
        let missing = dir.path().join("new.json");
        save_budget_to(&missing, &patch).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&missing).unwrap()).unwrap();
        assert_eq!(v["budget"]["monthly"], 60.0);
    }

    #[test]
    fn saving_budget_into_an_empty_config_file_writes_it() {
        let dir = tempfile::tempdir().unwrap();
        let patch = BudgetSectionPatch {
            monthly: Some(60.0),
            ..BudgetSectionPatch::default()
        };
        for (name, body) in [
            ("empty.json", "".as_bytes()),
            ("blank.json", " \n\t".as_bytes()),
            ("bom.json", "\u{feff}\n".as_bytes()),
        ] {
            let path = dir.path().join(name);
            std::fs::write(&path, body).unwrap();
            save_budget_to(&path, &patch).unwrap_or_else(|e| panic!("{name}: {e}"));
            let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            assert_eq!(v["budget"]["monthly"], 60.0, "{name}");
        }
        let (_, diags) = parse_settings_with_diagnostics(Some(" \n"), Some(""));
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn project_context_budget_keeps_other_keys() {
        let dir = std::env::temp_dir().join(format!("ax-budget-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("ax.json"),
            r#"{"name":"keep","budget":{"enabled":false}}"#,
        )
        .unwrap();
        save_project_context_budget(&dir, 8_000).unwrap();
        let text = std::fs::read_to_string(dir.join("ax.json")).unwrap();
        let settings = parse_settings(None, Some(&text));
        assert_eq!(settings.context_budget_tokens, Some(8_000));
        assert!(!settings.enabled);
        assert!(text.contains("\"name\": \"keep\"") || text.contains("\"name\":\"keep\""));
        std::fs::remove_dir_all(&dir).ok();
    }
}
