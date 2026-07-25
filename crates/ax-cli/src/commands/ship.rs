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
        let overrides = ax_ship::AutoCommitOverride {
            enabled: if auto_commit { Some(true) } else { None },
            revert_on_fail: if revert_on_fail { Some(true) } else { None },
        };
        let report = ax_ship::evaluate_project_with_overrides(root, overrides).await?;
        let json = serde_json::to_string_pretty(&report).unwrap_or_default();
        if ci {
            // Machine-readable single line summary on stderr; full report on stdout.
            let status = if report.quality_gate.passed {
                "passed"
            } else {
                "failed"
            };
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
        println!("{json}");
        return Ok(());
    }

    if draft {
        let daemon = ax_ship::ShipDaemon::new(root.clone());
        let cfg = daemon.config().await;
        let pipeline = ax_ship::ShipPipeline::new(root, cfg, daemon.bus);
        let pr = pipeline
            .create_draft_pr(
                title.as_deref().unwrap_or("ax ship draft"),
                "Draft PR created by ax Command Center",
            )
            .await?;
        println!("Draft PR #{} — {}", pr.number, pr.url);
        return Ok(());
    }

    if watch {
        return ax_web::serve(root, port, open).await;
    }

    Err("usage: ax ship --watch | --evaluate | --ci | --draft".into())
}
