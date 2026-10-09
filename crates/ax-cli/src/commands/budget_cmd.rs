//! `ax budget` — plan, simulate, and store a local budget.

use std::path::Path;

use ax_usage::{
    collect_report, cycle_stats, daily_plan, format_money, load_settings, monthly_budget_usd,
    parse_mode, price_band, save_global_budget, simulate, BudgetMode, BudgetSectionPatch,
    BudgetSettings, PriceBand, SavingsQuery, UsagePeriod,
};

pub async fn run_plan(json: bool, project: Option<&Path>) -> Result<(), String> {
    let settings = load_settings(project);
    let plan = daily_plan(&settings);
    let query = SavingsQuery {
        period: UsagePeriod::MonthToDate,
        from: None,
        to: None,
    };
    let (report, snapshot, _) = collect_report(&query, project).await?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "monthly": plan.monthly_display,
                "currency": format!("{:?}", plan.currency),
                "fxKnown": plan.fx_known,
                "workingDays": plan.working_days,
                "hoursPerDay": plan.hours_per_day,
                "daily": plan.daily,
                "hourly": plan.hourly,
                "currentSpendUsd": snapshot.current_spend_usd,
                "projectedSpendUsd": snapshot.projected_spend_usd,
                "remainingUsd": snapshot.remaining_budget_usd,
            })
        );
        return Ok(());
    }
    println!("MONTHLY BUDGET PLAN\n");
    println!(
        "Monthly budget       {}",
        money_display(plan.monthly_display, &settings)
    );
    println!("Working days         {}", plan.working_days);
    println!("Hours/day            {}", plan.hours_per_day);
    println!();
    println!(
        "Daily budget         {}",
        money_display(plan.daily, &settings)
    );
    println!(
        "Hourly budget        {}",
        money_display(plan.hourly, &settings)
    );
    println!();
    println!(
        "Current spend        {}",
        format_money(snapshot.current_spend_usd, &settings)
    );
    println!(
        "Projected spend      {}",
        format_money(snapshot.projected_spend_usd, &settings)
    );
    println!(
        "Remaining            {}",
        snapshot
            .remaining_budget_usd
            .map(|v| format_money(v, &settings))
            .unwrap_or_else(|| "unknown".into())
    );
    println!();
    println!("Mode                 {}", mode_label(settings.mode));
    println!("Known events         {}", report.known_events);
    println!("Unknown events       {}", report.unknown_events);
    if !plan.fx_known {
        println!("\nEUR budget needs budget.usdPerEur. No exchange rate was assumed.");
    }
    println!("\nThis is a budget calculator. It does not rank models.");
    Ok(())
}

pub async fn run_simulate(
    cycles: u32,
    days: Option<u32>,
    model: Option<String>,
    strategy: Option<String>,
    cost_per_cycle: Option<f64>,
    json: bool,
    project: Option<&Path>,
) -> Result<(), String> {
    let settings = load_settings(project);
    let days = days.unwrap_or(settings.working_days);
    let query = SavingsQuery {
        period: UsagePeriod::MonthToDate,
        from: None,
        to: None,
    };
    let (report, _, _) = collect_report(&query, project).await?;
    let mode = strategy
        .as_deref()
        .and_then(parse_mode)
        .unwrap_or(settings.mode);
    let stats = if cost_per_cycle.is_some() {
        None
    } else {
        let costs = filter_cycles(&report, model.as_deref(), mode, &settings);
        cycle_stats(&costs)
    };
    let average = cost_per_cycle.or_else(|| stats.as_ref().map(|row| row.average));
    let sim = simulate(cycles, days, average, monthly_budget_usd(&settings));
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "cyclesPerDay": sim.cycles_per_day,
                "workingDays": sim.working_days,
                "totalCycles": sim.total_cycles,
                "averageCostUsd": sim.average_cost_usd,
                "p50Usd": stats.as_ref().map(|row| row.p50),
                "p90Usd": stats.as_ref().map(|row| row.p90),
                "p95Usd": stats.as_ref().map(|row| row.p95),
                "p99Usd": stats.as_ref().map(|row| row.p99),
                "estimatedMonthlyUsd": sim.estimated_monthly_usd,
                "budgetUsd": sim.budget_usd,
                "differenceUsd": sim.difference_usd,
                "confidence": format!("{:?}", sim.confidence),
            }))
            .unwrap_or_default()
        );
        return Ok(());
    }
    println!("AX BUDGET SIMULATION\n");
    println!("Cycles/day           {}", sim.cycles_per_day);
    println!("Working days         {}", sim.working_days);
    println!("Total cycles         {}", sim.total_cycles);
    println!(
        "Average cost/cycle   {}",
        sim.average_cost_usd
            .map(|v| format_money(v, &settings))
            .unwrap_or_else(|| "unknown".into())
    );
    let percentile = |value: Option<f64>| {
        value
            .map(|v| format_money(v, &settings))
            .unwrap_or_else(|| "unknown".into())
    };
    println!(
        "p50                  {}",
        percentile(stats.as_ref().map(|row| row.p50))
    );
    println!(
        "p90                  {}",
        percentile(stats.as_ref().map(|row| row.p90))
    );
    println!(
        "p95                  {}",
        percentile(stats.as_ref().map(|row| row.p95))
    );
    println!(
        "p99                  {}",
        percentile(stats.as_ref().map(|row| row.p99))
    );
    println!(
        "Estimated monthly    {}",
        sim.estimated_monthly_usd
            .map(|v| format_money(v, &settings))
            .unwrap_or_else(|| "unknown".into())
    );
    println!(
        "Budget               {}",
        sim.budget_usd
            .map(|v| format_money(v, &settings))
            .unwrap_or_else(|| "unknown".into())
    );
    println!(
        "Difference           {}",
        sim.difference_usd
            .map(|v| format_money(v, &settings))
            .unwrap_or_else(|| "unknown".into())
    );
    println!(
        "\nContext savings      {} tokens avoided this month",
        report.tokens_avoided
    );
    println!("Confidence           {:?}", sim.confidence);
    println!("\nA missing average stays unknown. Pass --cost-per-cycle to supply a fixture rate.");
    Ok(())
}

fn filter_cycles(
    report: &ax_usage::CostReport,
    model: Option<&str>,
    mode: BudgetMode,
    settings: &BudgetSettings,
) -> Vec<f64> {
    let band = match mode {
        BudgetMode::Cheap => PriceBand::Cheap,
        BudgetMode::Balanced => PriceBand::Standard,
        BudgetMode::Quality => PriceBand::Premium,
    };
    report
        .cycle_rows
        .iter()
        .filter(|(label, _)| {
            if let Some(model) = model {
                return label.eq_ignore_ascii_case(model);
            }
            ax_usage::price_for_cost(label)
                .map(|(pricing, _)| price_band(pricing.input_per_mtok, settings) == band)
                .unwrap_or(false)
        })
        .map(|(_, usd)| *usd)
        .collect()
}

pub fn run_set(patch: BudgetSectionPatch) -> Result<(), String> {
    if let Some(tokens) = patch.context_budget_tokens {
        ax_usage::preflight_tokens_in_band(tokens)?;
    }
    let path = save_global_budget(&patch)?;
    println!("Saved budget settings in {}", path.display());
    Ok(())
}

/// `ax budget context --level N` or `--tokens N`. Tokens win when both are set.
pub fn run_context(
    level: Option<u8>,
    tokens: Option<u32>,
    project: Option<&std::path::Path>,
) -> Result<(), String> {
    if let Some(level) = level {
        if level > 100 {
            return Err("preflight level must be 0..=100".into());
        }
    }
    let tokens = match (tokens, level) {
        (Some(tokens), _) => ax_usage::preflight_tokens_in_band(tokens)?,
        (None, Some(level)) => ax_usage::level_to_tokens(level),
        (None, None) => {
            return Err("pass --level 0..=100 or --tokens 400..=8000".into());
        }
    };
    let path = if let Some(project) = project {
        ax_usage::save_project_preflight_tokens(project, tokens)?
    } else {
        save_global_budget(&BudgetSectionPatch {
            monthly: None,
            currency: None,
            usd_per_eur: None,
            working_days: None,
            hours_per_day: None,
            warning_percent: None,
            critical_percent: None,
            hard_limit_percent: None,
            mode: None,
            context_budget_tokens: Some(tokens),
        })?
    };
    println!(
        "Preflight size {} tokens (level {}) in {}",
        tokens,
        ax_usage::tokens_to_level(tokens),
        path.display()
    );
    Ok(())
}

fn money_display(amount: Option<f64>, settings: &BudgetSettings) -> String {
    match (amount, settings.currency) {
        (None, _) => "unknown".into(),
        (Some(v), ax_usage::Currency::Usd) => format!("${v:.2}"),
        (Some(v), ax_usage::Currency::Eur) => format!("€{v:.2}"),
    }
}

fn mode_label(mode: BudgetMode) -> &'static str {
    match mode {
        BudgetMode::Cheap => "cheap",
        BudgetMode::Balanced => "balanced",
        BudgetMode::Quality => "quality",
    }
}

#[cfg(test)]
mod tests {
    use super::run_context;

    #[test]
    fn level_zero_writes_400_tokens_win_and_level_101_writes_nothing() {
        let home = std::env::temp_dir().join(format!("ax-budget-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let previous = std::env::var_os("AX_HOME_DIR");
        std::env::set_var("AX_HOME_DIR", &home);
        let err = run_context(Some(101), None, None).unwrap_err();
        assert!(err.contains("0..=100"), "{err}");
        assert!(!home.join(".ax/config.json").exists());
        run_context(Some(0), None, None).unwrap();
        let text = std::fs::read_to_string(home.join(".ax/config.json")).unwrap();
        assert!(text.contains("\"budgetTokens\": 400"), "{text}");
        run_context(Some(10), Some(1500), None).unwrap();
        let text = std::fs::read_to_string(home.join(".ax/config.json")).unwrap();
        assert!(text.contains("\"budgetTokens\": 1500"), "{text}");
        match previous {
            Some(value) => std::env::set_var("AX_HOME_DIR", value),
            None => std::env::remove_var("AX_HOME_DIR"),
        }
        let _ = std::fs::remove_dir_all(&home);
    }
}
