use ax_policy::{GuardOp, ImportMode, MatchInput};

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
    write: bool,
    json: bool,
) -> Result<(), String> {
    let root = resolve_path(path);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let target = root.join(&file_path);
    let content = std::fs::read(&target).ok();
    let op = if write { GuardOp::Write } else { GuardOp::Delete };
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
    let result = ax_policy::sync_instructions(&ax_dir, fix).map_err(|e| e.to_string())?;
    for check in &result.checks {
        if check.optional && !check.path.exists() {
            continue;
        }
        if check.ok {
            println!("  OK   {}", check.label);
        } else {
            eprintln!("  FAIL {} — {}", check.label, check.issues.join("; "));
        }
    }
    if fix && !result.fixed.is_empty() {
        println!("Fixed {} file(s):", result.fixed.len());
        for rel in &result.fixed {
            println!("  {rel}");
        }
    }
    if result.fail_count > 0 {
        std::process::exit(1);
    }
    Ok(())
}
