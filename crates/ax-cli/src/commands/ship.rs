use crate::commands::resolve_path;

pub async fn run(
    path: Option<String>,
    watch: bool,
    evaluate: bool,
    ci: bool,
    draft: bool,
    title: Option<String>,
    port: u16,
    open: bool,
    auto_commit: bool,
    revert_on_fail: bool,
) -> Result<(), String> {
    let root = resolve_path(path);

    if evaluate || ci {
        let mode = if ci { "ci" } else { "evaluate" };
        if ci {
            ax_usage::log_ship_ci(Some(&root), "start");
        } else {
            ax_usage::log_ship(Some(&root), format!("start mode={mode}"));
        }
        let overrides = ax_ship::AutoCommitOverride {
            enabled: if auto_commit { Some(true) } else { None },
            revert_on_fail: if revert_on_fail { Some(true) } else { None },
        };
        let report = match ax_ship::evaluate_project_with_overrides(root.clone(), overrides).await {
            Ok(r) => r,
            Err(e) => {
                if ci {
                    ax_usage::log_ship_ci(Some(&root), "fail stage=evaluate");
                } else {
                    ax_usage::log_ship(Some(&root), format!("fail mode={mode}"));
                }
                return Err(e);
            }
        };
        let json = serde_json::to_string_pretty(&report).unwrap_or_default();
        if ci {
            let status = if report.quality_gate.passed {
                "passed"
            } else {
                "failed"
            };
            ax_usage::log_ship_ci(
                Some(&root),
                format!(
                    "status={status} steps={}",
                    report.quality_gate.steps.len()
                ),
            );
            eprintln!(
                "ax-ship-ci: status={status} steps={} sonar={}",
                report.quality_gate.steps.len(),
                report
                    .quality_gate
                    .sonar
                    .as_ref()
                    .map(|s| if s.passed { "passed" } else { "failed" })
                    .unwrap_or("n/a")
            );
            println!("{json}");
            if !report.quality_gate.passed {
                std::process::exit(1);
            }
            return Ok(());
        }
        ax_usage::log_ship(
            Some(&root),
            format!(
                "ok mode=evaluate passed={}",
                if report.quality_gate.passed {
                    "1"
                } else {
                    "0"
                }
            ),
        );
        println!("{json}");
        return Ok(());
    }

    if draft {
        ax_usage::log_ship(Some(&root), "start mode=draft");
        let daemon = ax_ship::ShipDaemon::new(root.clone());
        let cfg = daemon.config().await;
        let pipeline = ax_ship::ShipPipeline::new(root.clone(), cfg, daemon.bus);
        let pr = match pipeline
            .create_draft_pr(
                title.as_deref().unwrap_or("ax ship draft"),
                "Draft PR created by ax Command Center",
            )
            .await
        {
            Ok(pr) => pr,
            Err(e) => {
                ax_usage::log_ship(Some(&root), "fail mode=draft");
                return Err(e);
            }
        };
        ax_usage::log_ship(Some(&root), format!("ok mode=draft pr={}", pr.number));
        println!("Draft PR #{} — {}", pr.number, pr.url);
        return Ok(());
    }

    if watch {
        ax_usage::log_ship(Some(&root), format!("start mode=watch port={port}"));
        return ax_web::serve(root, port, open).await;
    }

    Err("usage: ax ship --watch | --evaluate | --ci | --draft".into())
}
