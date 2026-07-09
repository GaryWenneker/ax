use crate::commands::resolve_path;

pub async fn run(
    path: Option<String>,
    watch: bool,
    evaluate: bool,
    draft: bool,
    title: Option<String>,
    port: u16,
    open: bool,
) -> Result<(), String> {
    let root = resolve_path(path);

    if evaluate {
        let report = ax_ship::evaluate_project(root).await?;
        println!("{}", serde_json::to_string_pretty(&report).unwrap_or_default());
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

    Err("usage: ax ship watch | --evaluate | --draft".into())
}
