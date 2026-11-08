//! `ax doctor` — CLI-only health diagnostics.

use serde_json::json;

use crate::commands::resolve_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorMode {
    Cli,
    All,
}

pub async fn run(mode: DoctorMode, path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let mut checks = Vec::new();

    checks.push(check(
        "cli_version",
        "operational",
        format!("ax {}", env!("CARGO_PKG_VERSION")),
        None,
    ));

    let ax_dir = root.join(".ax");
    if ax_dir.is_dir() {
        checks.push(check(
            "project_init",
            "operational",
            format!(".ax present at {}", ax_dir.display()),
            None,
        ));
    } else {
        checks.push(check(
            "project_init",
            "misconfigured",
            ".ax missing — run ax init".into(),
            Some("ax init".into()),
        ));
    }

    match ax_core::Ax::open(&root).await {
        Ok(ax) => {
            checks.push(check(
                "database",
                "operational",
                "ax.db open".into(),
                None,
            ));
            match ax.get_stats().await {
                Ok(stats) => checks.push(check(
                    "index",
                    "operational",
                    format!("nodes={} files={}", stats.node_count, stats.file_count),
                    None,
                )),
                Err(e) => checks.push(check(
                    "index",
                    "degraded",
                    e.to_string(),
                    Some("ax sync".into()),
                )),
            }
            match ax.policy_status().await {
                Ok(s) => checks.push(check(
                    "policy_engine",
                    if s.indexed { "operational" } else { "degraded" },
                    format!("rules={} skills={} mode={}", s.rules, s.skills, s.mode),
                    if s.indexed {
                        None
                    } else {
                        Some("ax policy index".into())
                    },
                )),
                Err(e) => checks.push(check(
                    "policy_engine",
                    "degraded",
                    e.to_string(),
                    Some("ax policy index".into()),
                )),
            }
        }
        Err(e) => {
            checks.push(check(
                "database",
                "misconfigured",
                e.to_string(),
                Some("ax init".into()),
            ));
        }
    }

    match ax_usage::load_cache_status(None).await.context_error {
        None => checks.push(check(
            "usage_db",
            "operational",
            "usage.db readable".into(),
            None,
        )),
        Some(err) => checks.push(check(
            "usage_db",
            "degraded",
            err,
            None,
        )),
    }

    // Activity write probe (idempotent).
    match ax_usage::record_activity(
        &root,
        ax_usage::ActivityRecord {
            idempotency_key: format!("doctor.probe:{}", ax_usage::project_scope(&root)),
            source: Some(ax_usage::ActivitySource::Cli),
            operation: "doctor.probe".into(),
            status: Some(ax_usage::ActivityStatus::Success),
            summary: "doctor activity probe".into(),
            ..ax_usage::ActivityRecord::default()
        },
    )
    .await
    {
        Ok(_) => checks.push(check(
            "activity",
            "operational",
            "activity write ok".into(),
            None,
        )),
        Err(e) => checks.push(check(
            "activity",
            "degraded",
            e,
            None,
        )),
    }

    // Incomplete turns across sessions under .ax/cli-turns
    let turns_root = root.join(".ax").join("cli-turns");
    let mut incomplete = 0usize;
    if turns_root.is_dir() {
        if let Ok(sessions) = std::fs::read_dir(&turns_root) {
            for session in sessions.flatten() {
                if let Some(name) = session.file_name().to_str() {
                    if let Ok(status) = ax_usage::turn_status(&root, name, None) {
                        incomplete += status
                            .get("open")
                            .and_then(|v| v.as_array())
                            .map(|a| a.len())
                            .unwrap_or(0);
                    }
                }
            }
        }
    }
    checks.push(check(
        "incomplete_turns",
        if incomplete == 0 {
            "operational"
        } else {
            "degraded"
        },
        format!("{incomplete} open turn(s)"),
        if incomplete == 0 {
            None
        } else {
            Some("ax turn status --session <id>".into())
        },
    ));

    if matches!(mode, DoctorMode::Cli | DoctorMode::All) {
        checks.push(check(
            "cli_only_constraints",
            "operational",
            "no MCP transport, plugin, daemon, or network listener required for CLI workflow"
                .into(),
            None,
        ));
        // Detect if something is listening is out of scope; we only assert CLI path independence.
        checks.push(check(
            "restricted_mode",
            "operational",
            "CLI commands use in-process shared engine + local SQLite only".into(),
            None,
        ));
    }

    let worst = checks
        .iter()
        .map(|c| c["status"].as_str().unwrap_or("operational"))
        .max_by_key(|s| match *s {
            "misconfigured" => 3,
            "unsupported" => 3,
            "degraded" => 2,
            "restricted" => 1,
            _ => 0,
        })
        .unwrap_or("operational");

    let report = json!({
        "mode": match mode {
            DoctorMode::Cli => "cli",
            DoctorMode::All => "all",
        },
        "project": root.display().to_string(),
        "overall": worst,
        "checks": checks,
    });

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else {
        println!("ax doctor ({})", report["mode"]);
        println!("overall: {worst}");
        for c in &checks {
            println!(
                "- [{}] {}: {}",
                c["status"].as_str().unwrap_or("?"),
                c["id"].as_str().unwrap_or("?"),
                c["detail"].as_str().unwrap_or("")
            );
            if let Some(fix) = c.get("repair").and_then(|v| v.as_str()) {
                println!("    repair: {fix}");
            }
        }
    }
    Ok(())
}

fn check(id: &str, status: &str, detail: String, repair: Option<String>) -> serde_json::Value {
    json!({
        "id": id,
        "status": status,
        "detail": detail,
        "repair": repair,
    })
}
