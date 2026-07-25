use ax_policy::{GuardOp, ImportMode, MatchInput, PolicyStorage};

use crate::commands::resolve_path;

pub async fn run_index(path: Option<String>, force: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let storage = ax_policy::load_policy_config(&root).storage;
    let result = ax.index_policy(force).await.map_err(|e| e.to_string())?;
    match storage {
        ax_policy::PolicyStorage::Database if !force => {
            println!(
                "Database mode: {} rules, {} skills in ax.db (use --force to import from .ax/policy/ files)",
                result.rules_indexed, result.skills_indexed
            );
            if result.rules_indexed == 0 && ax_policy::policy_exists_filesystem(&root) {
                eprintln!(
                    "Hint: .ax/policy/ files exist but DB is empty — run `ax policy import` or `ax policy index --force`"
                );
            }
        }
        ax_policy::PolicyStorage::Database if force => {
            println!(
                "Imported {} rules, {} skills from .ax/policy/ into database (merge)",
                result.rules_indexed, result.skills_indexed
            );
        }
        _ => {
            println!(
                "Indexed {} rules, {} skills",
                result.rules_indexed, result.skills_indexed
            );
        }
    }
    Ok(())
}

pub async fn run_import(path: Option<String>) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let result = ax_policy::import_policy_from_files(ax.db_pool(), &root, ImportMode::Merge)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "Imported {} rules, {} skills from .ax/policy/ (merge — DB-only rows kept)",
        result.rules_indexed, result.skills_indexed
    );
    Ok(())
}

/// Clone a remote git policy registry into `.ax/policy/vendored/<name>/` and re-index.
pub async fn run_pull(
    url: String,
    path: Option<String>,
    name: Option<String>,
) -> Result<(), String> {
    let root = resolve_path(path);
    let ax_dir = root.join(".ax");
    if !ax_dir.is_dir() {
        return Err("project not initialized — run ax init first".into());
    }

    let vendor_name = name.unwrap_or_else(|| {
        url.trim_end_matches('/')
            .trim_end_matches(".git")
            .rsplit(['/', ':'])
            .next()
            .unwrap_or("remote-policy")
            .to_string()
    });
    let vendor_name = vendor_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect::<String>();
    if vendor_name.is_empty() {
        return Err("could not derive vendor name — pass --name".into());
    }

    let dest = root.join(".ax/policy/vendored").join(&vendor_name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest)
            .map_err(|e| format!("failed to clear {}: {e}", dest.display()))?;
    }
    std::fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;

    let status = std::process::Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            "--quiet",
            &url,
            &dest.to_string_lossy(),
        ])
        .status()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !status.success() {
        return Err(format!("git clone failed for {url}"));
    }

    // Prefer nested .ax/policy or top-level rules/skills; otherwise copy whole tree.
    let policy_src = if dest.join(".ax/policy").is_dir() {
        dest.join(".ax/policy")
    } else if dest.join("rules").is_dir() || dest.join("policy").is_dir() {
        if dest.join("policy").is_dir() {
            dest.join("policy")
        } else {
            dest.clone()
        }
    } else {
        dest.clone()
    };

    // Flatten into .ax/policy/rules and skills when the clone has those dirs.
    let rules_src = policy_src.join("rules");
    let skills_src = policy_src.join("skills");
    let rules_dst = root.join(".ax/policy/rules");
    let skills_dst = root.join(".ax/policy/skills");
    std::fs::create_dir_all(&rules_dst).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&skills_dst).map_err(|e| e.to_string())?;

    let mut copied_rules = 0usize;
    let mut copied_skills = 0usize;
    if rules_src.is_dir() {
        for entry in std::fs::read_dir(&rules_src).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("mdc") {
                let dest_file = rules_dst.join(entry.file_name());
                std::fs::copy(&path, &dest_file).map_err(|e| e.to_string())?;
                copied_rules += 1;
            }
        }
    }
    if skills_src.is_dir() {
        for entry in std::fs::read_dir(&skills_src).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            if path.is_dir() {
                let dest_skill = skills_dst.join(entry.file_name());
                copy_dir_recursive(&path, &dest_skill)?;
                copied_skills += 1;
            }
        }
    }

    println!(
        "Pulled policy from {url} → .ax/policy/vendored/{vendor_name}/ (copied {copied_rules} rules, {copied_skills} skills)"
    );

    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let result = ax
        .index_policy(true)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "Re-indexed policy: {} rules, {} skills",
        result.rules_indexed, result.skills_indexed
    );
    Ok(())
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub async fn run_export(path: Option<String>, out: String) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let out_path = if std::path::Path::new(&out).is_absolute() {
        std::path::PathBuf::from(out)
    } else {
        root.join(out)
    };
    let result = ax_policy::export_policy_to_files(ax.db_pool(), &root, &out_path)
        .await
        .map_err(|e| e.to_string())?;
    println!(
        "Exported {} rules, {} skills to {}",
        result.rules_exported, result.skills_exported, result.output_dir
    );
    Ok(())
}

pub async fn run_match(
    path: Option<String>,
    prompt: String,
    files: Vec<String>,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let input = MatchInput {
        prompt,
        cwd: root.clone(),
        open_files: files.iter().map(std::path::PathBuf::from).collect(),
        changed_files: vec![],
    };
    let result = ax.match_policy(input).await.map_err(|e| e.to_string())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
    } else {
        if result.rules.is_empty() && result.skills.is_empty() {
            println!("No rules or skills matched.");
        } else {
            print!("{}", result.inject);
        }
    }
    Ok(())
}

pub async fn run_rules(path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let rules = ax_policy::list_rules(ax.db_pool()).await.map_err(|e| e.to_string())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rules).unwrap_or_default());
    } else {
        for r in rules {
            println!("{} [{}] priority={}", r.id, r.level, r.priority);
        }
    }
    Ok(())
}

pub async fn run_skills(path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let skills = ax_policy::list_skills(ax.db_pool()).await.map_err(|e| e.to_string())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&skills).unwrap_or_default());
    } else {
        for s in skills {
            println!("{} — {}", s.name, s.description);
        }
    }
    Ok(())
}

pub async fn run_skill(path: Option<String>, name: String) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let skill = ax_policy::get_skill(ax.db_pool(), &name)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("skill not found: {name}"))?;
    println!("{}\n\n{}", skill.description, skill.body);
    Ok(())
}

pub async fn run_guard(
    path: Option<String>,
    file_path: String,
    delete: bool,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let _ = ax.ensure_policy_ready().await.map_err(|e| e.to_string())?;
    let target = root.join(&file_path);
    let content = std::fs::read(&target).ok();
    let op = if delete { GuardOp::Delete } else { GuardOp::Write };
    let result = ax
        .guard_operation(
            &target,
            op,
            content.as_deref().map(|v| &v[..]),
        )
        .await
        .map_err(|e| e.to_string())?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result).unwrap_or_default());
    } else if result.allowed {
        println!("allowed");
    } else {
        for v in &result.violations {
            eprintln!("{}: {}", v.rule_id, v.message);
        }
        std::process::exit(1);
    }
    Ok(())
}

pub async fn run_sync(path: Option<String>, fix: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let ax_dir = root.join(".ax");
    if !ax_dir.is_dir() {
        return Err("project not initialized — run ax init first".into());
    }

    let mut fail_count = 0usize;
    let mut all_fixed: Vec<String> = Vec::new();

    let policy = ax_policy::sync_instructions(&ax_dir, fix).map_err(|e| e.to_string())?;
    for check in &policy.checks {
        if check.optional && !check.path.exists() {
            continue;
        }
        if check.ok {
            println!("  OK   {}", check.label);
        } else {
            eprintln!("  FAIL {} — {}", check.label, check.issues.join("; "));
        }
    }
    fail_count += policy.fail_count;
    all_fixed.extend(policy.fixed);

    let ide = ax_policy::sync_ide_bootstrap(&root, fix).map_err(|e| e.to_string())?;
    println!("IDE bootstrap:");
    for check in &ide.checks {
        if check.optional && !check.path.exists() {
            continue;
        }
        if check.ok {
            println!("  OK   {}", check.label);
        } else {
            eprintln!("  FAIL {} — {}", check.label, check.issues.join("; "));
        }
    }
    fail_count += ide.fail_count;
    all_fixed.extend(ide.fixed);

    if fix && !all_fixed.is_empty() {
        println!("Fixed {} file(s):", all_fixed.len());
        for rel in &all_fixed {
            println!("  {rel}");
        }
    }
    if fail_count > 0 {
        std::process::exit(1);
    }
    let dupes = ax_policy::check_cursor_rule_duplicates(&root);
    for w in &dupes {
        eprintln!("  WARN {w}");
    }
    if !dupes.is_empty() {
        eprintln!("Remove duplicate `.cursor/rules/` files — ax policy is MCP-only (`.ax/policy/` + ax_preflight).");
    }
    Ok(())
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicyTestResult {
    name: String,
    ok: bool,
    detail: String,
}

pub async fn run_test(path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let mut results: Vec<PolicyTestResult> = Vec::new();

    let mut check = |name: &str, ok: bool, detail: &str| {
        results.push(PolicyTestResult {
            name: name.into(),
            ok,
            detail: detail.into(),
        });
    };

    let ready = ax.ensure_policy_ready().await.map_err(|e| e.to_string())?;
    check(
        "ensure_policy_ready",
        ready.rules_indexed > 0,
        &format!("{} rules, {} skills", ready.rules_indexed, ready.skills_indexed),
    );

    let status = ax.policy_status().await.map_err(|e| e.to_string())?;
    check(
        "policy_status",
        status.indexed && status.rules >= 4,
        &format!("mode={} rules={} skills={}", status.mode, status.rules, status.skills),
    );

    let rules = ax_policy::list_rules(ax.db_pool()).await.map_err(|e| e.to_string())?;
    let always: Vec<_> = rules.iter().filter(|r| r.always_apply).collect();
    check(
        "always_apply_rules",
        always.len() >= 4,
        &format!("{} alwaysApply rules", always.len()),
    );

    let subagents_rule = rules.iter().any(|r| r.id == "subagents");
    check("subagents_rule", subagents_rule, "subagents rule indexed");

    let skills = ax_policy::list_skills(ax.db_pool()).await.map_err(|e| e.to_string())?;
    let subagents_skill = skills.iter().any(|s| s.name == "subagents");
    let startup_skill = skills.iter().any(|s| s.name == "startup");
    check("subagents_skill", subagents_skill, "subagents skill indexed");
    check("startup_skill", startup_skill, "startup skill indexed");

    let baseline = MatchInput {
        prompt: "generic session".into(),
        cwd: root.clone(),
        open_files: vec![],
        changed_files: vec![],
    };
    let baseline_match = ax.match_policy(baseline).await.map_err(|e| e.to_string())?;
    check(
        "match_baseline",
        baseline_match.rules.len() >= 4,
        &format!("{} rules matched", baseline_match.rules.len()),
    );

    let release = MatchInput {
        prompt: "deploy release latest.txt".into(),
        cwd: root.clone(),
        open_files: vec![root.join("site/public/releases/latest.txt")],
        changed_files: vec![],
    };
    let release_match = ax.match_policy(release).await.map_err(|e| e.to_string())?;
    let has_release = release_match.rules.iter().any(|r| r.id == "release-all-platforms");
    check(
        "match_release_trigger",
        has_release,
        &format!(
            "release rule matched={has_release}, skills={}",
            release_match.skills.len()
        ),
    );

    let meta = ax_policy::build_preflight_meta(&status, &baseline_match);
    check(
        "preflight_meta",
        meta.guard_required && meta.matched_rules >= 4,
        &format!(
            "guardRequired={} matchedRules={}",
            meta.guard_required, meta.matched_rules
        ),
    );

    let inject_ok = baseline_match.inject.contains("<ax_policy")
        && baseline_match.inject.contains("agent-workflow");
    check("inject_block", inject_ok, "inject contains team policy");

    let guard_target = root.join("crates/ax-cli/src/main.rs");
    let guard_ok = ax
        .guard_operation(&guard_target, GuardOp::Write, std::fs::read(&guard_target).ok().as_deref())
        .await
        .map(|r| r.allowed)
        .unwrap_or(false);
    check("guard_utf8_existing", guard_ok, "existing UTF-8 file allowed");

    let new_target = root.join("target-dev/policy-test-new.rs");
    let bom = [0xEFu8, 0xBB, 0xBF, b'x'];
    let guard_bom = ax
        .guard_operation(&new_target, GuardOp::Write, Some(&bom))
        .await
        .map(|r| !r.allowed)
        .unwrap_or(false);
    check("guard_utf8_bom_blocked", guard_bom, "UTF-8 BOM in proposed content blocked");

    let env_target = root.join(".env");
    let has_secrets_rule = rules
        .iter()
        .any(|r| r.id.contains("secret") || r.tags.iter().any(|t| t == "secrets"));
    if has_secrets_rule {
        let guard_env = ax
            .guard_operation(&env_target, GuardOp::Delete, None)
            .await
            .map(|r| !r.allowed)
            .unwrap_or(false);
        check("guard_sensitive_delete", guard_env, ".env delete blocked by secrets rule");
    } else {
        check(
            "guard_sensitive_delete",
            true,
            "skipped — no secrets rule indexed",
        );
    }

    let ax_dir = root.join(".ax");
    let sync = ax_policy::sync_instructions(&ax_dir, false).map_err(|e| e.to_string())?;
    let startup_ok = sync.checks.iter().any(|c| c.label.contains("startup") && c.ok);
    check("bootstrap_startup", startup_ok, "startup skill file OK");

    let ide = ax_policy::sync_ide_bootstrap(&root, false).map_err(|e| e.to_string())?;
    let cursor_ok = ide
        .checks
        .iter()
        .any(|c| c.label.contains("ax.mdc") && c.ok);
    check("bootstrap_cursor", cursor_ok, ".cursor/rules/ax.mdc OK");

    let failed: Vec<_> = results.iter().filter(|r| !r.ok).collect();
    if json {
        println!("{}", serde_json::to_string_pretty(&results).unwrap_or_default());
    } else {
        for r in &results {
            let mark = if r.ok { "OK" } else { "FAIL" };
            println!("  {mark:<4} {} — {}", r.name, r.detail);
        }
        println!();
        println!(
            "{} passed, {} failed",
            results.len() - failed.len(),
            failed.len()
        );
    }

    if !failed.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}

pub async fn run_storage_status(path: Option<String>, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    let status = ax_policy::policy_storage_status(&root);
    if json {
        println!("{}", serde_json::to_string_pretty(&status).unwrap_or_default());
    } else {
        println!("Policy storage: {}", status.effective);
        println!("  source: {}", status.source);
        println!("  project config: {}", status.config_path);
        println!("  global config: {}", status.global_config_path);
        if let Some(v) = &status.project_value {
            println!("  project value: {v}");
        } else {
            println!("  project value: (not set)");
        }
        if let Some(v) = &status.global_value {
            println!("  global value: {v}");
        } else {
            println!("  global value: (not set)");
        }
        println!();
        println!("Set: ax policy storage database|files [--migrate] [--yes] [--global]");
    }
    Ok(())
}

pub async fn run_storage_set(
    path: Option<String>,
    target: PolicyStorage,
    global: bool,
    migrate: bool,
    yes: bool,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);

    // Propose migration plan without changing storage or importing.
    if migrate && target == PolicyStorage::Database && !yes && !global {
        let plan = ax_policy::scan_policy_candidates(&root);
        if json {
            println!("{}", serde_json::to_string_pretty(&plan).unwrap_or_default());
        } else {
            println!(
                "Migration scan: {} rules, {} skills ({} skipped)",
                plan.rules_found,
                plan.skills_found,
                plan.skipped.len()
            );
            println!();
            for c in &plan.candidates {
                println!(
                    "  [{}] {} — {} ({})",
                    c.kind, c.key, c.source_path, c.source
                );
                for q in &c.questions {
                    println!("    • {} (current: {})", q.question, q.current);
                }
                println!();
            }
            if !plan.skipped.is_empty() {
                println!("Skipped:");
                for s in &plan.skipped {
                    println!("  {} — {}", s.source_path, s.reason);
                }
                println!();
            }
            println!("{}", plan.interview_instruction);
            println!();
            println!("After interview: ax policy storage database --migrate --yes");
        }
        return Ok(());
    }

    let current = ax_policy::load_policy_config(&root).storage;
    if current == target && !migrate {
        let status = ax_policy::policy_storage_status(&root);
        if json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ok": true,
                    "changed": false,
                    "effective": status.effective,
                    "source": status.source,
                }))
                .unwrap_or_default()
            );
        } else {
            println!("Policy storage already set to {}.", target.as_str());
        }
        return Ok(());
    }

    let config_path = if global {
        ax_policy::write_global_policy_storage(target)?
    } else {
        ax_policy::write_project_policy_storage(&root, target)?
    };

    if migrate {
        let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
        match target {
            PolicyStorage::Database => {
                let (plan, apply) = ax_policy::migrate_to_database(ax.db_pool(), &root)
                    .await
                    .map_err(|e| e.to_string())?;
                if !json {
                    println!(
                        "Migrated {} rules, {} skills into database (scanned {} candidates, {} skipped)",
                        apply.rules_imported,
                        apply.skills_imported,
                        plan.candidates.len(),
                        plan.skipped.len()
                    );
                    if !plan.skipped.is_empty() {
                        println!("  skipped {} files (bootstrap, invalid, or duplicate)", plan.skipped.len());
                    }
                }
            }
            PolicyStorage::Files => {
                let out = root.join(".ax").join("policy");
                let result = ax_policy::export_policy_to_files(ax.db_pool(), &root, &out)
                    .await
                    .map_err(|e| e.to_string())?;
                ax.index_policy(true).await.map_err(|e| e.to_string())?;
                if !json {
                    println!(
                        "Exported {} rules, {} skills to {}",
                        result.rules_exported, result.skills_exported, result.output_dir
                    );
                }
            }
        }
    }

    if json {
        let mut payload = serde_json::json!({
            "ok": true,
            "changed": true,
            "storage": target.as_str(),
            "configPath": config_path.display().to_string(),
            "scope": if global { "global" } else { "project" },
            "migrated": migrate,
        });
        if migrate && target == PolicyStorage::Database {
            let plan = ax_policy::scan_policy_candidates(&root);
            payload["rulesFound"] = serde_json::json!(plan.rules_found);
            payload["skillsFound"] = serde_json::json!(plan.skills_found);
            payload["candidates"] = serde_json::json!(plan.candidates.len());
        }
        println!("{}", serde_json::to_string_pretty(&payload).unwrap_or_default());
    } else {
        println!("Policy storage set to {}.", target.as_str());
        println!("  updated: {}", config_path.display());
        if !migrate {
            match target {
                PolicyStorage::Database => {
                    println!("  hint: run `ax policy import` to load .ax/policy/ files into ax.db");
                }
                PolicyStorage::Files => {
                    println!("  hint: run `ax policy export --out .ax/policy` to write DB rules to disk");
                }
            }
        }
    }
    Ok(())
}

pub async fn run_capture(
    path: Option<String>,
    prompt: String,
    files: Vec<String>,
    yes: bool,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;

    let mut proposal = ax_policy::propose_rule_from_prompt(&prompt, &files);
    if !proposal.detected {
        if json {
            println!("{}", serde_json::to_string_pretty(&proposal).unwrap_or_default());
        } else {
            println!("No directive detected in prompt.");
        }
        return Ok(());
    }

    let existing: Vec<String> = ax_policy::list_rules(ax.db_pool())
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| r.id)
        .collect();
    proposal = ax_policy::finalize_proposal(proposal, &existing);

    if !yes {
        if json {
            println!("{}", serde_json::to_string_pretty(&proposal).unwrap_or_default());
        } else {
            println!("Directive detected (confidence: {})", proposal.confidence);
            println!("Suggested id: {}", proposal.suggested_id);
            println!("Path: {}", proposal.preview_path);
            println!("Triggers: {}", proposal.frontmatter.triggers.join(", "));
            println!();
            print!("{}", proposal.preview);
            println!();
            println!("Ask the user about rule options before saving:");
            for q in &proposal.questions {
                let opts = if q.options.is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", q.options.join(", "))
                };
                println!("  • {} (current: {}){}", q.question, q.current, opts);
            }
            println!();
            println!("Run with --yes to save with defaults (skip interview).");
        }
        return Ok(());
    }

    let store = ax_policy::PolicyStore::new(ax.db_pool().clone(), root.clone());
    let storage = store.storage();
    let doc = store
        .save_rule(proposal.frontmatter.clone(), proposal.body.clone())
        .await
        .map_err(|e| e.error)?;

    let storage_label = match storage {
        ax_policy::PolicyStorage::Database => "database",
        ax_policy::PolicyStorage::Files => "files",
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "ok": true,
                "id": doc.frontmatter.id,
                "storage": storage_label,
                "path": proposal.preview_path,
            }))
            .unwrap_or_default()
        );
    } else {
        println!("Saved rule: {} ({storage_label})", doc.frontmatter.id);
        println!("  {}", proposal.preview_path);
    }
    Ok(())
}
