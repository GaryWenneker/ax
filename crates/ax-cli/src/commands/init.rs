use std::sync::Arc;

use ax_extraction::orchestrator::IndexOptions;
use ax_sync::git_hooks::install_git_sync_hooks;

use crate::commands::{check_unsafe_root, resolve_path};
use crate::installer::{run_installer, InstallOptions};
use crate::ui::install_log::tildify;
use crate::ui::{
    dim, finish_progress_bar, format_duration_ms, index_progress_bar, index_progress_callback, info_line,
    ok_line,
};

pub async fn run(path: Option<String>) -> Result<(), String> {
    let root = resolve_path(path);
    check_unsafe_root(&root)?;

    println!();
    println!(
        "{}",
        info_line(format!("Initializing ax in {}", tildify(&root)))
    );
    println!("  {}", dim("Large projects take several minutes — progress updates below."));
    println!();

    let ax_dir = root.join(".ax");
    let seed = ax_policy::seed_default_policy(&ax_dir).ok();
    let sync = ax_policy::sync_instructions(&ax_dir, true).ok();
    if let Some(ref s) = seed {
        if !s.created.is_empty() {
            println!(
                "{}",
                ok_line(format!(
                    "Seeded {} default policy file(s) in .ax/policy/",
                    s.created.len()
                ))
            );
            for rel in &s.created {
                println!("  {}", dim(rel));
            }
            println!();
        }
    }
    if let Some(ref s) = sync {
        if !s.fixed.is_empty() {
            println!(
                "{}",
                ok_line(format!(
                    "Ensured {} startup protocol file(s)",
                    s.fixed.len()
                ))
            );
            for rel in &s.fixed {
                println!("  {}", dim(rel));
            }
            println!();
        }
    }

    let mut ax = ax_core::Ax::init(&root).await.map_err(|e| e.to_string())?;

    let progress = index_progress_bar(false);
    let on_progress = progress
        .as_ref()
        .map(|pb| index_progress_callback(Arc::clone(pb)));
    let result = ax
        .index_all(IndexOptions::default(), on_progress)
        .await
        .map_err(|e| e.to_string())?;
    finish_progress_bar(progress);

    println!(
        "{}",
        ok_line(format!(
            "Indexed {} files in {}",
            result.files_indexed,
            format_duration_ms(result.duration_ms)
        ))
    );

    // Database policy mode keeps rules in SQLite — always import seeded .ax/policy/ files on init.
    match ax.index_policy(true).await {
        Ok(policy) => {
            if policy.rules_indexed > 0 || policy.skills_indexed > 0 {
                println!(
                    "{}",
                    ok_line(format!(
                        "Policy indexed {} rules, {} skills (startup protocol via ax_preflight)",
                        policy.rules_indexed,
                        policy.skills_indexed
                    ))
                );
            }
        }
        Err(e) => {
            eprintln!("{}", dim(format!("Policy index skipped: {e}")));
        }
    }

    install_git_sync_hooks(&root).map_err(|e| e.to_string())?;
    if let Ok(mut t) = ax_telemetry::telemetry().lock() {
        t.record_lifecycle(
            "index",
            serde_json::json!({
                "languages": [],
                "file_count_bucket": ax_telemetry::bucket_file_count(result.files_indexed),
                "duration_bucket": ax_telemetry::bucket_duration(result.duration_ms),
            }),
        );
        t.persist_sync();
        let _ = t.flush_now(ax_telemetry::DEFAULT_FLUSH_TIMEOUT_MS).await;
    }

    run_installer(
        &root,
        InstallOptions {
            yes: true,
            install_all: false,
        },
    )?;
    Ok(())
}
