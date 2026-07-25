use std::sync::Arc;

use ax_context::directory::is_initialized;
use ax_extraction::orchestrator::IndexOptions;
use ax_reasoning::seed_offload_on_init;
use ax_sync::git_hooks::install_git_sync_hooks;

use crate::commands::{check_unsafe_root, resolve_path};
use crate::installer::{run_installer, InstallOptions};
use crate::ui::install_log::tildify;
use crate::ui::{
    dim, finish_progress_bar, format_duration_ms, index_progress_bar, index_progress_callback, info_line,
    ok_line,
};

pub async fn run(path: Option<String>, workspace: bool) -> Result<(), String> {
    let root = resolve_path(path);
    check_unsafe_root(&root)?;

    if workspace {
        let members = ax_core::discover_members(&root);
        if members.is_empty() {
            return Err(
                "no workspace members discovered — add nested .ax/ dirs or a Cargo workspace"
                    .into(),
            );
        }
        let cfg = ax_core::WorkspaceConfig {
            members: members.clone(),
        };
        ax_core::write_workspace_config(&root, &cfg)?;
        println!(
            "{}",
            ok_line(format!(
                "Wrote {} workspace member(s) to ax.json",
                members.len()
            ))
        );
        for m in &members {
            let label = m.name.as_deref().unwrap_or("(unnamed)");
            println!("  {} — {}", dim(&m.path), label);
        }
        println!();
        // Initialize each member project (creates .ax/ + index).
        for m in &members {
            let member_root = root.join(&m.path);
            if !member_root.is_dir() {
                eprintln!("{}", dim(format!("skip missing member: {}", m.path)));
                continue;
            }
            println!(
                "{}",
                info_line(format!("Initializing member {}", tildify(&member_root)))
            );
            Box::pin(run(
                Some(member_root.to_string_lossy().to_string()),
                false,
            ))
            .await?;
            println!();
        }
        return Ok(());
    }

    let already_initialized = is_initialized(&root);

    println!();
    if already_initialized {
        println!(
            "{}",
            info_line(format!("ax already initialized in {}", tildify(&root)))
        );
        println!(
            "  {}",
            dim("Running incremental sync — use `ax index` for a full re-index.")
        );
    } else {
        println!(
            "{}",
            info_line(format!("Initializing ax in {}", tildify(&root)))
        );
        println!("  {}", dim("Large projects take several minutes — progress updates below."));
    }
    println!();

    let ax_dir = root.join(".ax");
    let seed = ax_policy::seed_default_policy(&ax_dir).ok();
    let project_name = root.file_name().and_then(|n| n.to_str());
    let ship = ax_ship::seed_ship_config(&ax_dir, project_name).ok();
    let ide = ax_policy::seed_ide_agent_workflow(&root).ok();
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
    if let Some(ref s) = ship {
        if !s.created.is_empty() {
            println!(
                "{}",
                ok_line(format!(
                    "Seeded Command Center config (.ax/{})",
                    s.created.join(", .ax/")
                ))
            );
            for rel in &s.created {
                println!("  {}", dim(format!(".ax/{rel}")));
            }
            println!();
        }
    }
    if let Some(ref i) = ide {
        if !i.created.is_empty() {
            println!(
                "{}",
                ok_line(format!(
                    "Seeded {} IDE bootstrap file(s)",
                    i.created.len()
                ))
            );
            for rel in &i.created {
                println!("  {}", dim(rel));
            }
            println!();
        } else if !i.updated.is_empty() {
            println!(
                "{}",
                ok_line(format!(
                    "Updated {} IDE bootstrap file(s)",
                    i.updated.len()
                ))
            );
            for rel in &i.updated {
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

    match seed_offload_on_init().await {
        Ok(report) => {
            if report.catalog_written {
                println!(
                    "{}",
                    ok_line(format!(
                        "Wired {} LLM offload providers in ~/.ax/config.json",
                        ax_reasoning::OFFLOAD_PROVIDERS.len()
                    ))
                );
                for p in ax_reasoning::OFFLOAD_PROVIDERS {
                    let marker = if report.discovered.contains(&p.id.to_string()) {
                        "found"
                    } else {
                        "set key"
                    };
                    println!(
                        "  {} {} ({}) — {}",
                        dim("·"),
                        p.name,
                        p.key_env.unwrap_or("(no key)"),
                        marker
                    );
                }
                println!();
            }
            if let Some(active) = &report.active {
                if report.skipped_existing {
                    println!(
                        "{}",
                        ok_line(format!(
                            "Offload already configured: {} ({})",
                            active.name, active.url
                        ))
                    );
                } else {
                    println!(
                        "{}",
                        ok_line(format!(
                            "Offload active: {} ({})",
                            active.name, active.url
                        ))
                    );
                }
                println!();
            } else if report.catalog_written {
                println!(
                    "  {}",
                    dim(
                        "No API keys detected — set OPENAI_API_KEY, CEREBRAS_API_KEY, GROQ_API_KEY, etc. then run ax explore"
                    )
                );
                println!();
            }
        }
        Err(e) => {
            eprintln!("{}", dim(format!("Offload wiring skipped: {e}")));
        }
    }

    let mut ax = if already_initialized {
        ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?
    } else {
        ax_core::Ax::init(&root).await.map_err(|e| e.to_string())?
    };

    let progress = index_progress_bar(false);
    let on_progress = progress
        .as_ref()
        .map(|pb| index_progress_callback(Arc::clone(pb)));
    let result = if already_initialized {
        ax.sync(IndexOptions::default(), on_progress)
            .await
            .map_err(|e| e.to_string())?
    } else {
        ax.index_all(IndexOptions::default(), on_progress)
            .await
            .map_err(|e| e.to_string())?
    };
    finish_progress_bar(progress);

    if already_initialized {
        let summary = if result.files_indexed == 0 {
            ok_line("Already up to date")
        } else {
            ok_line(format!(
                "Synced {} file(s) in {}",
                result.files_indexed,
                format_duration_ms(result.duration_ms)
            ))
        };
        println!("{}", summary);
    } else {
        println!(
            "{}",
            ok_line(format!(
                "Indexed {} files in {}",
                result.files_indexed,
                format_duration_ms(result.duration_ms)
            ))
        );
    }

    // Database policy mode keeps rules in SQLite — import seeded .ax/policy/ files on first init.
    let force_policy = !already_initialized;
    match ax.index_policy(force_policy).await {
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
