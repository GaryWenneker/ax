use std::sync::Arc;

use owo_colors::OwoColorize;

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
    run_inner(path, workspace, true).await
}

async fn run_inner(path: Option<String>, workspace: bool, savings: bool) -> Result<(), String> {
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
            Box::pin(run_inner(
                Some(member_root.to_string_lossy().to_string()),
                false,
                false,
            ))
            .await?;
            println!();
        }
        run_savings_setup().await;
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
    ax_policy::ensure_ax_share_gitignore(&root).map_err(|e| format!(".ax/.gitignore: {e}"))?;
    let seed = ax_policy::seed_default_policy(&ax_dir).ok();
    if let Err(e) = choose_agents_dir_for_init(&root) {
        eprintln!("{}", dim(format!("Policy directory prompt skipped: {e}")));
    }
    let cursor = ax_policy::seed_project_cursor_skills(&root).ok();
    let global_policy = ax_policy::seed_global_policy().ok();
    match choose_stacks_for_init(&root) {
        Ok(selected) => match ax_policy::replace_selection(&root, &selected, false) {
            Ok(report) => {
                if selected.is_empty() {
                    println!("{}", ok_line("Core policy only (no stacks)"));
                } else if !report.stacks.is_empty() {
                    println!(
                        "{}",
                        ok_line(format!("Stacks: {}", report.stacks.join(", ")))
                    );
                }
            }
            Err(e) => eprintln!("{}", dim(format!("Stack apply skipped: {e}"))),
        },
        Err(e) => eprintln!("{}", dim(format!("Stack prompt skipped: {e}"))),
    }
    if ax_policy::read_configured_stacks(&root)
        .iter()
        .any(|id| id == "dotnet")
    {
        if let Err(e) = store_dotnet_code_review_in_global_db(&root).await {
            eprintln!("{}", dim(format!("Global skill store skipped: {e}")));
        }
    }
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
    if let Some(ref c) = cursor {
        if !c.created.is_empty() {
            println!(
                "{}",
                ok_line(format!(
                    "Seeded {} baseline Cursor skill(s) in .cursor/skills/",
                    c.created.len()
                ))
            );
            for rel in &c.created {
                println!("  {}", dim(rel));
            }
            println!();
        }
    }
    if let Some(ref g) = global_policy {
        if !g.created.is_empty() {
            println!(
                "{}",
                ok_line(format!(
                    "Seeded {} global policy skill(s) in ~/.ax/global_policy/skills/",
                    g.created.len()
                ))
            );
            for rel in &g.created {
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

    // Database policy mode — always merge from disk so ~/.ax/global_policy/ stays in ax.db.
    let force_policy = true;
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
            targets: Vec::new(),
        },
    )?;
    if savings {
        run_savings_setup().await;
    }
    Ok(())
}

/// Ask for the on-disk rules/skills folder when `policy.agentsDir` is unset.
/// A non-interactive run saves the default `.agents`.
fn choose_agents_dir_for_init(root: &std::path::Path) -> Result<(), String> {
    if ax_policy::configured_agents_dir(root).is_some() {
        return Ok(());
    }
    let name = if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        println!(
            "{}",
            info_line("Where should ax store rule and skill files?")
        );
        println!(
            "  {}",
            dim("Press Enter for .agents, or type another folder name.")
        );
        print!("Directory [.agents]: ");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        ax_policy::validate_agents_dir_name(&line)?
    } else {
        ax_policy::DEFAULT_AGENTS_DIR.to_string()
    };
    let saved = ax_policy::write_project_agents_dir(root, &name)?;
    println!("{}", ok_line(format!("Policy files directory: {saved}")));
    Ok(())
}

const STACK_GROUPS: &[(&str, &[&str])] = &[
    ("Platforms", &["dotnet", "java", "kotlin", "scala"]),
    ("Web", &["javascript", "typescript", "react", "nextjs", "angular", "vue", "svelte", "astro"]),
    ("PHP", &["php", "laravel", "drupal"]),
    ("CMS", &["sitecore", "optimizely"]),
    ("Systems", &["rust", "go", "c", "cpp", "swift", "objc"]),
    ("Languages", &["python", "ruby", "dart", "lua", "luau", "r", "pascal"]),
];

fn grouped_stacks(catalog: &[ax_policy::StackInfo]) -> Vec<(String, &ax_policy::StackInfo)> {
    let mut ordered = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (group, ids) in STACK_GROUPS {
        for id in *ids {
            if let Some(stack) = catalog.iter().find(|stack| stack.id == *id) {
                ordered.push(((*group).to_string(), stack));
                seen.insert(stack.id.as_str());
            }
        }
    }
    for stack in catalog {
        if seen.insert(stack.id.as_str()) {
            ordered.push(("Other".to_string(), stack));
        }
    }
    ordered
}

struct StackMenuLine {
    item: Option<usize>,
    text: String,
}

fn stack_menu_lines(
    grouped: &[(String, &ax_policy::StackInfo)],
    checked: &[bool],
    cursor: usize,
) -> Vec<StackMenuLine> {
    let mut lines = Vec::new();
    let mut last_group = "";
    for (index, (group, stack)) in grouped.iter().enumerate() {
        if group != last_group {
            lines.push(StackMenuLine {
                item: None,
                text: format!("  {}", group.yellow().bold()),
            });
            last_group = group;
        }
        let active = index == cursor;
        let pointer = if active {
            "❯".cyan().bold().to_string()
        } else {
            " ".to_string()
        };
        let mark = if checked[index] {
            "[x]".green().bold().to_string()
        } else {
            "[ ]".dimmed().to_string()
        };
        let id = if active {
            stack.id.cyan().bold().to_string()
        } else {
            stack.id.cyan().to_string()
        };
        lines.push(StackMenuLine {
            item: Some(index),
            text: format!(
                "{pointer} {mark} {:<12} {}",
                id,
                stack.description.dimmed()
            ),
        });
    }
    lines
}

/// Arrow keys move the cursor. Space toggles. Enter confirms.
/// Esc keeps the defaults that were already checked.
fn prompt_stack_menu(checked: &mut [bool], grouped: &[(String, &ax_policy::StackInfo)]) -> Result<(), String> {
    let term = console::Term::stderr();
    let original = checked.to_vec();
    let mut cursor = 0;
    let mut top = 0usize;
    let mut drawn = 0usize;
    let _ = term.hide_cursor();
    let result = loop {
        let lines = stack_menu_lines(grouped, checked, cursor);
        let page = (term.size().0 as usize).saturating_sub(8).clamp(8, 18);
        let cursor_line = lines
            .iter()
            .position(|line| line.item == Some(cursor))
            .unwrap_or(0);
        if cursor_line < top {
            top = cursor_line;
        } else if cursor_line >= top + page {
            top = cursor_line + 1 - page;
        }
        if drawn > 0 {
            let _ = term.clear_last_lines(drawn);
        }
        let mut shown = 0usize;
        for line in lines.iter().skip(top).take(page) {
            let _ = term.write_line(&line.text);
            shown += 1;
        }
        let _ = term.write_line(&dim(
            "↑↓ or j/k move    space toggle    enter confirm    esc keep defaults",
        ));
        shown += 1;
        drawn = shown;
        let key = match term.read_key() {
            Ok(key) => key,
            Err(err) => break Err(err.to_string()),
        };
        match key {
            console::Key::ArrowUp | console::Key::Char('k') => {
                if cursor > 0 {
                    cursor -= 1;
                }
            }
            console::Key::ArrowDown | console::Key::Char('j') => {
                if cursor + 1 < grouped.len() {
                    cursor += 1;
                }
            }
            console::Key::PageUp | console::Key::Char('u') => {
                cursor = cursor.saturating_sub(page);
            }
            console::Key::PageDown | console::Key::Char('d') => {
                cursor = (cursor + page).min(grouped.len().saturating_sub(1));
            }
            console::Key::Home => cursor = 0,
            console::Key::End => cursor = grouped.len().saturating_sub(1),
            console::Key::Char(' ') => checked[cursor] = !checked[cursor],
            console::Key::Enter => break Ok(()),
            console::Key::Escape => {
                checked.copy_from_slice(&original);
                break Ok(());
            }
            _ => {}
        }
    };
    let _ = term.show_cursor();
    result
}

/// Ask which stacks to install. Runs on every `ax init`, including a second run.
/// A non-interactive stdin keeps the saved selection so CI does not block.
fn choose_stacks_for_init(root: &std::path::Path) -> Result<Vec<String>, String> {
    let current = ax_policy::read_configured_stacks(root);
    let detected: Vec<String> = ax_policy::detect_stacks(root).into_iter().map(|d| d.id).collect();
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Ok(current);
    }
    let catalog = ax_policy::stack_catalog_list();
    let grouped = grouped_stacks(&catalog);
    println!("{}", info_line("Which stacks should this project use?"));
    if !detected.is_empty() {
        let names = detected
            .iter()
            .map(|id| id.green().bold().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        println!("  {} {names}", "Detected:".dimmed());
    }
    let mut checked: Vec<bool> = grouped
        .iter()
        .map(|(_, stack)| {
            current.iter().any(|id| id == &stack.id) || detected.iter().any(|id| id == &stack.id)
        })
        .collect();
    if let Err(err) = prompt_stack_menu(&mut checked, &grouped) {
        println!(
            "  {}",
            dim(format!(
                "Menu unavailable ({err}). Type ids separated by spaces, or 'none'."
            ))
        );
        print!("Stacks: ");
        let _ = std::io::Write::flush(&mut std::io::stdout());
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        return ax_policy::parse_stack_choice(&line, &current, &detected);
    }
    let ids: Vec<String> = grouped
        .iter()
        .zip(checked)
        .filter(|(_, on)| *on)
        .map(|((_, stack), _)| stack.id.clone())
        .collect();
    ax_policy::resolve_stacks(&ids)
}

async fn store_dotnet_code_review_in_global_db(project_root: &std::path::Path) -> Result<(), String> {
    let path = ax_policy::agents_dir(project_root)
        .join("skills")
        .join("dotnet-code-review")
        .join("SKILL.md");
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let doc = ax_policy::parse_skill_file(&path, &raw).map_err(|e| format!("{e:?}"))?;
    ax_global_db::policy::upsert_machine_skill(&doc.frontmatter.name, &global_skill_payload(&doc))
        .await
        .map_err(|e| e.to_string())?;
    println!("{}", ok_line("Stored dotnet-code-review in ~/.ax/global.db"));
    Ok(())
}

fn global_skill_payload(doc: &ax_policy::PolicySkillDoc) -> serde_json::Value {
    let fm = &doc.frontmatter;
    serde_json::json!({
        "name": fm.name,
        "description": fm.description,
        "alwaysApply": fm.always_apply,
        "triggers": fm.triggers,
        "tags": fm.tags,
        "priority": fm.priority,
        "body": doc.body,
        "sourcePath": "global.db",
        "enabled": fm.enabled,
        "status": fm.status,
        "scope": "company",
    })
}

/// Store the seeded machine skills (`ax_policy::global_db_seed_skills`) in `db_path`
/// under the `machine_root` project. Returns the stored skill names.
pub(crate) async fn store_seeded_skills_in_global_db(
    db_path: &std::path::Path,
    machine_root: &std::path::Path,
) -> Result<Vec<String>, String> {
    let pool = ax_global_db::open_and_init(db_path)
        .await
        .map_err(|e| format!("{e:#}"))?;
    let result = async {
        let project_id = ax_global_db::policy::ensure_project(&pool, machine_root)
            .await
            .map_err(|e| format!("{e:#}"))?;
        let mut stored = Vec::new();
        for (name, raw) in ax_policy::global_db_seed_skills() {
            let path = std::path::Path::new(name).join("SKILL.md");
            let doc = ax_policy::parse_skill_file(&path, raw).map_err(|e| format!("{name}: {e:?}"))?;
            ax_global_db::policy::upsert_policy_item(
                &pool,
                project_id,
                ax_global_db::policy::PolicyKind::Skills,
                name,
                &global_skill_payload(&doc),
            )
            .await
            .map_err(|e| format!("{e:#}"))?;
            stored.push(name.to_string());
        }
        Ok(stored)
    }
    .await;
    pool.close().await;
    result
}

/// Blocking wrapper for sync callers (`ax install`). Runs on its own thread and
/// runtime, so it works inside or outside an async context.
pub(crate) fn store_seeded_skills_in_global_db_blocking() -> Result<Vec<String>, String> {
    let db = ax_global_db::global_db_path().map_err(|e| e.to_string())?;
    let machine = dirs::home_dir().ok_or("no home dir")?.join(".ax");
    store_seeded_skills_blocking_at(db, machine)
}

fn store_seeded_skills_blocking_at(
    db: std::path::PathBuf,
    machine: std::path::PathBuf,
) -> Result<Vec<String>, String> {
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?
            .block_on(store_seeded_skills_in_global_db(&db, &machine))
    })
    .join()
    .map_err(|_| "global.db store thread panicked".to_string())?
}

/// Cursor savings hook plus a full log import. Failures stay visible and do not undo init.
async fn run_savings_setup() {
    if let Err(e) = crate::commands::savings::run_hook_install() {
        eprintln!("{}", dim(format!("Savings hook install skipped: {e}")));
    }
    if let Err(e) = crate::commands::savings::run_import(false, false, true).await {
        eprintln!("{}", dim(format!("Savings import skipped: {e}")));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ax-init-{name}-{}", std::process::id()));
        cleanup(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup(dir: &std::path::Path) {
        // A leftover temp dir only costs disk space; it must not fail the test.
        let _ = std::fs::remove_dir_all(dir);
    }

    async fn skill_rows(db: &std::path::Path, item_id: &str) -> Vec<(String, String)> {
        let pool = ax_global_db::open_and_init(db).await.unwrap();
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT item_id, payload FROM global_policy_skills WHERE item_id = ?",
        )
        .bind(item_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        pool.close().await;
        rows
    }

    #[tokio::test]
    async fn seeded_skills_are_stored_once_as_company_skills() {
        let dir = temp_dir("global-db");
        let db = dir.join("global.db");
        let machine = dir.join(".ax");
        let stored = store_seeded_skills_in_global_db(&db, &machine).await.unwrap();
        assert_eq!(stored, vec!["review-loop".to_string(), "pr-review-comments".to_string()]);
        store_seeded_skills_in_global_db(&db, &machine).await.unwrap();
        let rows = skill_rows(&db, "review-loop").await;
        assert_eq!(rows.len(), 1, "a second store updates the row instead of duplicating it");
        let payload: serde_json::Value = serde_json::from_str(&rows[0].1).unwrap();
        assert_eq!(payload["name"], "review-loop");
        assert_eq!(payload["scope"], "company");
        assert!(payload["body"].as_str().unwrap().contains("## 4. Repeat"));
        assert_eq!(skill_rows(&db, "pr-review-comments").await.len(), 1);
        cleanup(&dir);
    }

    #[tokio::test]
    async fn unusable_global_db_is_an_error_not_a_panic() {
        let dir = temp_dir("global-db-bad");
        let result = store_seeded_skills_in_global_db(&dir, &dir.join(".ax")).await;
        let err = result.expect_err("a directory is not a database");
        assert!(err.contains(&dir.display().to_string()), "error names the path: {err}");
        cleanup(&dir);
    }

    #[test]
    fn blocking_store_works_from_inside_a_runtime() {
        let dir = temp_dir("global-db-blocking");
        let db = dir.join("global.db");
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        let result =
            rt.block_on(async { store_seeded_skills_blocking_at(db.clone(), dir.join(".ax")) });
        assert_eq!(result.unwrap(), vec!["review-loop".to_string(), "pr-review-comments".to_string()]);
        assert!(db.is_file());
        cleanup(&dir);
    }
}
