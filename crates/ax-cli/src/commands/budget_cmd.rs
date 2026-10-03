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
    let path = save_global_budget(&patch)?;
    println!("Saved budget settings in {}", path.display());
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
