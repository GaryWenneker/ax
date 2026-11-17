//! `ax db check` and `ax db repair`.

use std::path::{Path, PathBuf};

use ax_db::integrity::{
    check_database, record_incident, repair_database, CheckOptions, CheckReport, RepairOptions,
    RepairReport,
};
use serde::Serialize;

use super::resolve_path;

pub async fn run_check(
    path: Option<String>,
    which: String,
    quick: bool,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    let targets = targets(&root, &which)?;
    let model = active_model();
    let mut reports = Vec::new();
    for target in targets {
        if !target.required && !target.path.exists() {
            continue;
        }
        let report = check_database(
            &target.path,
            &CheckOptions {
                quick,
                embedding_model: target.embedding_model.then(|| model.clone()),
            },
        )
        .await;
        if !report.ok {
            let _ = record_incident(&target.path, &report);
        }
        reports.push(report);
    }
    emit_checks(&reports, json);
    if reports.iter().any(|report| !report.ok) {
        return Err("integrity check failed".into());
    }
    Ok(())
}

pub async fn run_repair(
    path: Option<String>,
    which: String,
    dry_run: bool,
    salvage: bool,
    stop_writers: bool,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path.clone());
    if stop_writers {
        super::daemon::run(path, super::daemon::DaemonAction::Stop).await?;
    }
    let targets = targets(&root, &which)?;
    let model = active_model();
    let mut reports = Vec::new();
    for target in targets {
        if !target.required && !target.path.exists() {
            continue;
        }
        let embedding_model = target.embedding_model.then(|| model.clone());
        let mut repair = repair_database(
            &target.path,
            &RepairOptions {
                dry_run,
                salvage,
                backup_dir: None,
                embedding_model: embedding_model.clone(),
                quick: false,
            },
        )
        .await;
        if repair.proceed && !dry_run {
            match follow_memory(&target.path, &repair.follow_up).await {
                Ok(actions) => repair.actions.extend(actions),
                Err(err) => repair.remaining.push(ax_db::integrity::Finding {
                    code: "io".into(),
                    subject: String::new(),
                    detail: err,
                }),
            }
        }
        let after = check_database(
            &target.path,
            &CheckOptions {
                quick: false,
                embedding_model,
            },
        )
        .await;
        repair.remaining = after.findings;
        repair.ok = repair.remaining.is_empty();
        if !repair.ok && !dry_run {
            let _ = record_incident(
                &target.path,
                &CheckReport {
                    path: repair.path.clone(),
                    ok: false,
                    findings: repair.remaining.clone(),
                },
            );
        }
        reports.push(repair);
    }
    emit_repairs(&reports, json);
    if reports.iter().any(|report| !report.ok) {
        return Err("integrity repair left findings".into());
    }
    Ok(())
}

struct Target {
    path: PathBuf,
    required: bool,
    embedding_model: bool,
}

fn targets(root: &Path, which: &str) -> Result<Vec<Target>, String> {
    let project = Target {
        path: root.join(".ax").join("ax.db"),
        required: true,
        embedding_model: true,
    };
    let usage = Target {
        path: ax_usage::usage_db_path(),
        required: which != "all",
        embedding_model: false,
    };
    let global = Target {
        path: ax_utils::paths::resolve_global_db_path(ax_utils::paths::home_dir())
            .unwrap_or_else(|| PathBuf::from(".ax").join("global.db")),
        required: which != "all",
        embedding_model: false,
    };
    match which {
        "project" => Ok(vec![project]),
        "usage" => Ok(vec![usage]),
        "global" => Ok(vec![global]),
        "all" => Ok(vec![project, usage, global]),
        _ => Err("db must be project, usage, global, or all".into()),
    }
}

fn active_model() -> String {
    ax_memory::embed::embed_labeled("ax").1.to_string()
}

async fn follow_memory(path: &Path, follow_up: &[String]) -> Result<Vec<String>, String> {
    if follow_up.is_empty() {
        return Ok(Vec::new());
    }
    let db = ax_db::Database::open(path)
        .await
        .map_err(|err| err.to_string())?;
    let mut done = Vec::new();
    if follow_up.iter().any(|action| action == "reembed") {
        let mut total = 0usize;
        for _ in 0..10_000 {
            let updated = ax_memory::reembed_stale(db.pool(), 200)
                .await
                .map_err(|err| err.to_string())?;
            total += updated;
            if updated == 0 {
                break;
            }
        }
        done.push(format!("reembed:{total}"));
    }
    if follow_up.iter().any(|action| action == "rebuild_vectors") {
        let indexed = ax_memory::rebuild_vector_index(db.pool())
            .await
            .map_err(|err| err.to_string())?;
        done.push(format!("rebuild_vectors:{indexed}"));
    }
    if follow_up
        .iter()
        .any(|action| action.starts_with("reindex:"))
    {
        if let Some(root) = project_root_of(path) {
            let queries = ax_db::queries::QueryBuilder::new(db.pool().clone());
            let actions = ax_extraction::reindex_damaged_files(&root, &queries, follow_up)
                .await
                .map_err(|err| err.to_string())?;
            done.extend(actions);
        }
    }
    db.close().await;
    Ok(done)
}

fn project_root_of(path: &Path) -> Option<std::path::PathBuf> {
    let parent = path.parent()?;
    if parent.file_name().and_then(|name| name.to_str()) == Some(".ax") {
        parent.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::project_root_of;
    use std::path::Path;

    #[test]
    fn a_database_inside_dot_ax_names_the_project_and_any_other_file_does_not() {
        assert_eq!(
            project_root_of(Path::new("/work/app/.ax/ax.db")).as_deref(),
            Some(Path::new("/work/app"))
        );
        assert!(project_root_of(Path::new("/tmp/ax.db")).is_none());
    }
}

fn emit_checks(reports: &[CheckReport], json: bool) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(reports).unwrap_or_else(|_| "[]".into())
        );
        return;
    }
    for report in reports {
        println!("{}", report.path);
        println!("ok: {}", report.ok);
        for finding in &report.findings {
            println!("- {}: {}", finding.code, finding.detail);
        }
    }
}

#[derive(Serialize)]
struct RepairView<'a> {
    path: &'a str,
    ok: bool,
    dry_run: bool,
    backup: &'a Option<String>,
    actions: &'a [String],
    remaining: &'a [ax_db::integrity::Finding],
}

fn emit_repairs(reports: &[RepairReport], json: bool) {
    if json {
        let views: Vec<RepairView> = reports
            .iter()
            .map(|report| RepairView {
                path: &report.path,
                ok: report.ok,
                dry_run: report.dry_run,
                backup: &report.backup,
                actions: &report.actions,
                remaining: &report.remaining,
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&views).unwrap_or_else(|_| "[]".into())
        );
        return;
    }
    for report in reports {
        println!("{}", report.path);
        println!("ok: {}", report.ok);
        if let Some(backup) = &report.backup {
            println!("backup: {backup}");
        }
        for action in &report.actions {
            println!("- {action}");
        }
        for finding in &report.remaining {
            println!("- {}: {}", finding.code, finding.detail);
        }
    }
}
