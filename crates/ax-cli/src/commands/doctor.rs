//! `ax doctor` — CLI-only health diagnostics.

use serde_json::json;

use crate::commands::resolve_project_root;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum DoctorMode {
    /// Host and policy checks for a CLI-only setup
    Cli,
    /// Every check, including the MCP wiring
    All,
}

pub async fn run(mode: DoctorMode, path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_project_root(path);
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
            checks.push(check("database", "operational", "ax.db open".into(), None));
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

    let project_db = root.join(".ax").join("ax.db");
    if project_db.is_file() {
        let (_, model) = ax_memory::embed::embed_labeled("ax");
        let integrity = ax_db::integrity::check_database(
            &project_db,
            &ax_db::integrity::CheckOptions {
                quick: true,
                embedding_model: Some(model.to_string()),
            },
        )
        .await;
        if integrity.ok {
            checks.push(check(
                "database_integrity",
                "operational",
                "quick check passed".into(),
                None,
            ));
        } else {
            let detail = integrity
                .findings
                .iter()
                .map(|finding| finding.code.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            checks.push(check(
                "database_integrity",
                "degraded",
                detail,
                Some("ax db repair".into()),
            ));
        }
        if let Ok(db) = ax_db::Database::open(&project_db).await {
            let mine = ax_db::builds::current_build_stamp(env!("CARGO_PKG_VERSION"));
            match ax_db::builds::mcp_stamps(db.pool()).await {
                Ok(rows) => {
                    let lines = ax_db::builds::stale_mcp_lines(&mine, &rows);
                    if lines.is_empty() {
                        checks.push(check(
                            "mcp_builds",
                            "operational",
                            "no older MCP build is registered".into(),
                            None,
                        ));
                    } else {
                        checks.push(check(
                            "mcp_builds",
                            "degraded",
                            lines.join(" "),
                            Some("Restart the MCP server in Cursor".into()),
                        ));
                    }
                }
                Err(error) => checks.push(check(
                    "mcp_builds",
                    "degraded",
                    format!("could not read registered builds: {error}"),
                    None,
                )),
            }
            db.close().await;
        }
    }

    match ax_usage::load_cache_status(None).await.context_error {
        None => checks.push(check(
            "usage_db",
            "operational",
            "usage.db readable".into(),
            None,
        )),
        Some(err) => checks.push(check("usage_db", "degraded", err, None)),
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
        Err(e) => checks.push(check("activity", "degraded", e, None)),
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
        let missing = ax_installer::cli_only::verify_cli_only(&root);
        checks.push(check(
            "cli_memory_procedure",
            if missing.is_empty() {
                "operational"
            } else {
                "degraded"
            },
            if missing.is_empty() {
                "every CLI-only instruction file teaches the memory procedure".into()
            } else {
                missing
                    .iter()
                    .map(|(rel, issue)| format!("{rel}: {issue}"))
                    .collect::<Vec<_>>()
                    .join("; ")
            },
            (!missing.is_empty()).then(|| "ax install --mode cli-only".into()),
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
