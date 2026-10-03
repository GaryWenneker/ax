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
    dim, finish_progress_bar, format_duration_ms, index_progress_bar, index_progress_callback,
    info_line, ok_line,
};

pub async fn run(path: Option<String>, workspace: bool, all: bool) -> Result<(), String> {
    if all {
        if path.is_some() {
            return Err("`ax init --all` does not take a path".into());
        }
        if workspace {
            return Err("`ax init --all` does not combine with --workspace".into());
        }
        return refresh_discovered_projects().await;
    }
    run_inner(path, workspace, true, true).await
}

/// Scan the home directory and initialize every discovered project.
/// `AX_SKIP_PROJECT_INIT=1` leaves projects untouched.
pub async fn refresh_discovered_projects() -> Result<(), String> {
    if std::env::var("AX_SKIP_PROJECT_INIT").ok().as_deref() == Some("1") {
        println!(
            "{}",
            info_line("Skipping project discovery (AX_SKIP_PROJECT_INIT=1).")
        );
        return Ok(());
    }
    let Some(home) = ax_utils::paths::home_dir() else {
        return Err("no home directory".into());
    };
    let projects =
        ax_context::discover_projects_for_init(&home, ax_context::INSTALL_DISCOVERY_DEPTH);
    if projects.is_empty() {
        println!(
            "{}",
            info_line("No projects found under the home directory.")
        );
        return Ok(());
    }
    println!(
        "{}",
        info_line(format!(
            "Initializing {} discovered project(s)…",
            projects.len()
        ))
    );
    let mut failed = 0usize;
    for project in &projects {
        let label = project.to_string_lossy().to_string();
        if let Err(err) = run_inner(Some(label.clone()), false, false, false).await {
            failed += 1;
            eprintln!("{}", dim(format!("init failed for {label}: {err}")));
        }
    }
    run_savings_setup().await;
    if failed > 0 {
        eprintln!(
            "{}",
            dim(format!(
                "{failed} project(s) failed. Re-run `ax init --all`."
            ))
        );
    }
    Ok(())
}

async fn run_inner(
    path: Option<String>,
    workspace: bool,
    savings: bool,
    interactive: bool,
) -> Result<(), String> {
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
                interactive,
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
        println!(
            "  {}",
            dim("Large projects take several minutes — progress updates below.")
        );
    }
    println!();

    let ax_dir = root.join(".ax");
    ax_policy::ensure_ax_share_gitignore(&root).map_err(|e| format!(".ax/.gitignore: {e}"))?;
    let seed = ax_policy::seed_default_policy(&ax_dir).ok();
    if let Err(e) = choose_agents_dir_for_init(&root, interactive) {
        eprintln!("{}", dim(format!("Policy directory prompt skipped: {e}")));
    }
    let cursor = ax_policy::seed_project_cursor_skills(&root).ok();
    let global_policy = ax_policy::seed_global_policy().ok();
    match choose_stacks_for_init(&root, interactive) {
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
    let ides = choose_ides_for_init(&root, interactive)?;
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
                ok_line(format!("Seeded {} IDE bootstrap file(s)", i.created.len()))
            );
            for rel in &i.created {
                println!("  {}", dim(rel));
            }
            println!();
        } else if !i.updated.is_empty() {
            println!(
                "{}",
                ok_line(format!("Updated {} IDE bootstrap file(s)", i.updated.len()))
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
                        ok_line(format!("Offload active: {} ({})", active.name, active.url))
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

    match ax_policy::apply(&root, false) {
        Ok(report) => {
            println!(
                "{}",
                ok_line(format!(
                    "Architecture seed {} (+{} entities, +{} rules)",
                    report.seed_version, report.entities_created, report.rules_created
                ))
            );
        }
        Err(err) => return Err(format!("architecture seed failed: {err}")),
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
                        policy.rules_indexed, policy.skills_indexed
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
    }
    ax_telemetry::flush_global(ax_telemetry::DEFAULT_FLUSH_TIMEOUT_MS).await;

    apply_ide_choice(&root, ides)?;
    if savings {
        run_savings_setup().await;
    }
    Ok(())
}

/// Ask for the on-disk rules/skills folder when `policy.agentsDir` is unset.
/// A non-interactive run saves the default `.agents`.
fn choose_agents_dir_for_init(root: &std::path::Path, interactive: bool) -> Result<(), String> {
    if ax_policy::configured_agents_dir(root).is_some() {
        return Ok(());
    }
    let name = if interactive && std::io::IsTerminal::is_terminal(&std::io::stdin()) {
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
    (
        "Web",
        &[
            "javascript",
            "typescript",
            "react",
            "nextjs",
            "angular",
            "vue",
            "svelte",
            "astro",
        ],
    ),
    ("PHP", &["php", "laravel", "drupal"]),
    ("CMS", &["sitecore", "optimizely"]),
    ("Systems", &["rust", "go", "c", "cpp", "swift", "objc"]),
    (
        "Languages",
        &["python", "ruby", "dart", "lua", "luau", "r", "pascal"],
    ),
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

/// One selectable row in an init checklist menu.
struct MenuItem {
    group: String,
    id: String,
    note: String,
}

fn stack_items(grouped: &[(String, &ax_policy::StackInfo)]) -> Vec<MenuItem> {
    grouped
        .iter()
        .map(|(group, stack)| MenuItem {
            group: group.clone(),
            id: stack.id.clone(),
            note: stack.description.clone(),
        })
        .collect()
}

#[cfg(test)]
fn stack_menu_lines(
    grouped: &[(String, &ax_policy::StackInfo)],
    checked: &[bool],
    cursor: usize,
) -> Vec<StackMenuLine> {
    menu_lines(&stack_items(grouped), checked, cursor)
}

fn menu_lines(items: &[MenuItem], checked: &[bool], cursor: usize) -> Vec<StackMenuLine> {
    let mut lines = Vec::new();
    let mut last_group = "";
    for (index, item) in items.iter().enumerate() {
        if item.group != last_group {
            lines.push(StackMenuLine {
                item: None,
                text: format!("  {}", item.group.yellow().bold()),
            });
            last_group = &item.group;
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
            item.id.cyan().bold().to_string()
        } else {
            item.id.cyan().to_string()
        };
        lines.push(StackMenuLine {
            item: Some(index),
            text: format!("{pointer} {mark} {:<12} {}", id, item.note.dimmed()),
        });
    }
    lines
}

/// Arrow keys move the cursor. Space toggles. Enter confirms.
/// Esc keeps the defaults that were already checked.
fn prompt_menu(checked: &mut [bool], items: &[MenuItem]) -> Result<(), String> {
    let term = console::Term::stderr();
    let original = checked.to_vec();
    let mut cursor = 0;
    let mut top = 0usize;
    let mut drawn = 0usize;
    let _ = term.hide_cursor();
    let result = loop {
        let lines = menu_lines(items, checked, cursor);
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
                cursor = cursor.saturating_sub(1);
            }
            console::Key::ArrowDown | console::Key::Char('j') => {
                if cursor + 1 < items.len() {
                    cursor += 1;
                }
            }
            console::Key::PageUp | console::Key::Char('u') => {
                cursor = cursor.saturating_sub(page);
            }
            console::Key::PageDown | console::Key::Char('d') => {
                cursor = (cursor + page).min(items.len().saturating_sub(1));
            }
            console::Key::Home => cursor = 0,
            console::Key::End => cursor = items.len().saturating_sub(1),
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
fn choose_stacks_for_init(
    root: &std::path::Path,
    interactive: bool,
) -> Result<Vec<String>, String> {
    let current = ax_policy::read_configured_stacks(root);
    let detected: Vec<String> = ax_policy::detect_stacks(root)
        .into_iter()
        .map(|d| d.id)
        .collect();
    if !interactive || !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
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
    if let Err(err) = prompt_menu(&mut checked, &stack_items(&grouped)) {
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

/// The IDE answer, asked right after stacks and applied at the end of init.
struct IdeChoice {
    chosen: Vec<String>,
    /// False without a terminal: nothing is saved or removed then.
    asked: bool,
}

fn ide_items(detected: &[String]) -> Vec<MenuItem> {
    ax_installer::ide_groups()
        .into_iter()
        .map(|(group, id)| {
            let name = ax_installer::display_name(id);
            let found = detected.iter().any(|d| d == id);
            MenuItem {
                group: group.to_string(),
                id: id.to_string(),
                note: if found {
                    format!("{name} (found)")
                } else {
                    name.to_string()
                },
            }
        })
        .collect()
}

/// Answers the IDE question without a terminal (scripts, tests); treated like a typed answer.
const ANSWER_ENV: &str = "AX_INIT_IDES";

/// Ask which IDEs to connect. Without a terminal, keep the saved list (or the found IDEs).
fn choose_ides_for_init(root: &std::path::Path, interactive: bool) -> Result<IdeChoice, String> {
    let raw = ax_policy::read_project_ides(root);
    let saved = raw.as_deref().map(|list| {
        let (known, unknown) = ax_installer::known_ides(list);
        if !unknown.is_empty() {
            eprintln!(
                "{}",
                dim(format!(
                    "Ignoring unknown saved IDE(s): {}",
                    unknown.join(", ")
                ))
            );
        }
        known
    });
    let detected: Vec<String> = ax_installer::TARGETS
        .iter()
        .filter(|id| ax_installer::is_detected(id))
        .map(|id| id.to_string())
        .collect();
    let defaults = ax_installer::ide_defaults(saved.as_deref(), &detected);
    if let Ok(answer) = std::env::var(ANSWER_ENV) {
        let chosen = ax_installer::parse_ide_choice(&answer, &defaults)?;
        return Ok(IdeChoice {
            chosen,
            asked: true,
        });
    }
    if !interactive || !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Ok(IdeChoice {
            chosen: defaults,
            asked: false,
        });
    }
    let items = ide_items(&detected);
    println!("{}", info_line("Which IDEs and agents should ax connect?"));
    let mut checked: Vec<bool> = items.iter().map(|i| defaults.contains(&i.id)).collect();
    let chosen = match prompt_menu(&mut checked, &items) {
        Ok(()) => items
            .iter()
            .zip(checked)
            .filter(|(_, on)| *on)
            .map(|(i, _)| i.id.clone())
            .collect(),
        Err(err) => {
            println!(
                "  {}",
                dim(format!(
                    "Menu unavailable ({err}). Type ids separated by spaces, or 'none'."
                ))
            );
            print!("IDEs: ");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            let mut line = String::new();
            std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            ax_installer::parse_ide_choice(&line, &defaults)?
        }
    };
    Ok(IdeChoice {
        chosen,
        asked: true,
    })
}

/// Save the answer, remove ax from IDEs that were dropped, and connect the chosen ones.
fn apply_ide_choice(root: &std::path::Path, choice: IdeChoice) -> Result<(), String> {
    if choice.asked {
        ax_policy::write_project_ides(root, &choice.chosen)?;
        let configured: Vec<String> = ax_installer::agent_status(root)?
            .into_iter()
            .filter(|s| s.configured)
            .map(|s| s.id)
            .collect();
        let dropped = ax_installer::ides_to_remove(&configured, &choice.chosen);
        if !dropped.is_empty() {
            for report in ax_installer::uninstall_targets(root, &dropped)? {
                let mut seen = std::collections::HashSet::new();
                let files: Vec<String> = report
                    .files
                    .iter()
                    .map(|f| tildify(&f.path))
                    .filter(|p| seen.insert(p.clone()))
                    .collect();
                let what = if files.is_empty() {
                    "nothing to remove".to_string()
                } else {
                    files.join(", ")
                };
                println!(
                    "{}",
                    ok_line(format!("Removed ax from {}: {what}", report.display_name))
                );
            }
        }
    }
    if choice.chosen.is_empty() {
        ax_installer::ensure_global_config();
        println!(
            "{}",
            ok_line("No IDEs chosen; ax is not connected to any IDE")
        );
        return Ok(());
    }
    run_installer(
        root,
        InstallOptions {
            yes: true,
            install_all: false,
            targets: choice.chosen,
        },
    )
}

async fn store_dotnet_code_review_in_global_db(
    project_root: &std::path::Path,
) -> Result<(), String> {
    let path = ax_policy::agents_dir(project_root)
        .join("skills")
        .join("dotnet-code-review")
        .join("SKILL.md");
    let raw = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let doc = ax_policy::parse_skill_file(&path, &raw).map_err(|e| format!("{e:?}"))?;
    ax_global_db::policy::upsert_machine_skill(&doc.frontmatter.name, &global_skill_payload(&doc))
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "{}",
        ok_line("Stored dotnet-code-review in ~/.ax/global.db")
    );
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
            let doc =
                ax_policy::parse_skill_file(&path, raw).map_err(|e| format!("{name}: {e:?}"))?;
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
    let machine = ax_utils::paths::home_dir()
        .ok_or("no home dir")?
        .join(".ax");
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

    fn strip_ansi(text: &str) -> String {
        let mut out = String::new();
        let mut chars = text.chars();
        while let Some(c) = chars.next() {
            if c == '\u{1b}' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn stack_menu_text_is_stable() {
        let stack = |id: &str, description: &str| ax_policy::StackInfo {
            id: id.into(),
            version: "1".into(),
            description: description.into(),
            depends_on: Vec::new(),
            files: 0,
        };
        let (rust, go, ts) = (
            stack("rust", "Rust rules"),
            stack("go", "Go rules"),
            stack("typescript", "TS rules"),
        );
        let grouped = vec![
            ("Systems".to_string(), &rust),
            ("Systems".to_string(), &go),
            ("Web".to_string(), &ts),
        ];
        let text: Vec<(Option<usize>, String)> =
            stack_menu_lines(&grouped, &[true, false, false], 1)
                .into_iter()
                .map(|l| (l.item, strip_ansi(&l.text)))
                .collect();
        assert_eq!(
            text,
            vec![
                (None, "  Systems".to_string()),
                (Some(0), "  [x] rust Rust rules".to_string()),
                (Some(1), "❯ [ ] go Go rules".to_string()),
                (None, "  Web".to_string()),
                (Some(2), "  [ ] typescript TS rules".to_string()),
            ]
        );
    }

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
        let rows: Vec<(String, String)> =
            sqlx::query_as("SELECT item_id, payload FROM global_policy_skills WHERE item_id = ?")
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
        let stored = store_seeded_skills_in_global_db(&db, &machine)
            .await
            .unwrap();
        assert_eq!(
            stored,
            vec!["review-loop".to_string(), "pr-review-comments".to_string()]
        );
        store_seeded_skills_in_global_db(&db, &machine)
            .await
            .unwrap();
        let rows = skill_rows(&db, "review-loop").await;
        assert_eq!(
            rows.len(),
            1,
            "a second store updates the row instead of duplicating it"
        );
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
        assert!(
            err.contains(&dir.display().to_string()),
            "error names the path: {err}"
        );
        cleanup(&dir);
    }

    #[test]
    fn blocking_store_works_from_inside_a_runtime() {
        let dir = temp_dir("global-db-blocking");
        let db = dir.join("global.db");
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap();
        let result =
            rt.block_on(async { store_seeded_skills_blocking_at(db.clone(), dir.join(".ax")) });
        assert_eq!(
            result.unwrap(),
            vec!["review-loop".to_string(), "pr-review-comments".to_string()]
        );
        assert!(db.is_file());
        cleanup(&dir);
    }
}
