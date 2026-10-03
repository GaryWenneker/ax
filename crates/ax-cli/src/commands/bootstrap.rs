//! `ax bootstrap` applies the versioned architecture seed.

use ax_policy::{apply, verify, BootstrapReport};

use crate::commands::resolve_path;

pub fn run(
    path: Option<String>,
    dry_run: bool,
    verify_only: bool,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    if verify_only {
        verify(&root)?;
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "seedVersion": ax_policy::AX_SEED_VERSION,
                    "status": "verified",
                    "entitiesCreated": 0,
                    "relationshipsCreated": 0,
                    "rulesCreated": 0,
                    "skillsCreated": 0,
                }))
                .map_err(|err| err.to_string())?
            );
        } else {
            println!(
                "seed {version} verified",
                version = ax_policy::AX_SEED_VERSION
            );
        }
        return Ok(());
    }
    let report = apply(&root, dry_run)?;
    if json {
        println!("{}", report_json(&report)?);
    } else {
        println!("{}", report.plan);
    }
    Ok(())
}

fn report_json(report: &BootstrapReport) -> Result<String, String> {
    serde_json::to_string_pretty(&serde_json::json!({
        "seedVersion": report.seed_version,
        "status": report.status,
        "dryRun": report.dry_run,
        "migration": report.migration,
        "entitiesCreated": report.entities_created,
        "relationshipsCreated": report.relationships_created,
        "rulesCreated": report.rules_created,
        "skillsCreated": report.skills_created,
    }))
    .map_err(|err| err.to_string())
}
