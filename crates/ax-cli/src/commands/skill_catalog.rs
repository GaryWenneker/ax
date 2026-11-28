use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ax_skill_catalog::{
    agent_brief, audit_directory, audit_directory_present, audit_rule_present, audit_skill_text,
    find_rule_file, find_skill_dir, install_report_url, install_tree, load_latest, load_settings,
    materialize_well_known, parse_search_body, parse_source, parse_verdict, project_briefs,
    require_one_skill, save_settings, stage_one_skill, store_report, LockEntry, LockFile,
    SkillAuditReport, SkillPresence, SkillSource, SEARCH_URL,
};
use clap::{Args, Subcommand};

use super::resolve_project_root;

#[derive(Subcommand)]
pub enum SkillCommands {
    /// Search the skills.sh catalog
    Search {
        query: String,
        #[arg(long)]
        owner: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: u32,
        #[arg(long)]
        json: bool,
    },
    /// Install a skill from GitHub, GitLab, Azure Repos, a well-known URL, or a local directory
    Add(SkillAddArgs),
    /// List skills recorded in the ax lock
    List {
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        global: bool,
        #[arg(long)]
        json: bool,
    },
    /// Re-install recorded skills
    Update {
        names: Vec<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        global: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Remove a skill ax installed
    Remove {
        names: Vec<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        global: bool,
    },
    /// Publish one skill to a git remote or a well-known directory
    Publish {
        name: String,
        #[arg(long)]
        remote: Option<String>,
        #[arg(long)]
        path: Option<String>,
    },
    /// Audit one skill and store the report. The skill stays usable.
    Audit {
        name: String,
        /// Catalog source to audit without installing
        #[arg(long)]
        source: Option<String>,
        /// JSON verdict file from an agent
        #[arg(long)]
        verdict: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        global: bool,
    },
    /// Print stored audit edits for the agent
    Brief {
        name: Option<String>,
        #[arg(long)]
        path: Option<String>,
        #[arg(long)]
        global: bool,
    },
    /// Turn the skills.sh install report on or off
    ReportInstalls {
        #[arg(long, group = "switch")]
        on: bool,
        #[arg(long, group = "switch")]
        off: bool,
        #[arg(long)]
        path: Option<String>,
    },
}

#[derive(Args)]
pub struct SkillAddArgs {
    source: String,
    #[arg(long = "skill")]
    skill: Vec<String>,
    #[arg(long)]
    global: bool,
    #[arg(long)]
    yes: bool,
    #[arg(long)]
    path: Option<String>,
}

pub async fn run(action: SkillCommands) -> Result<(), String> {
    match action {
        SkillCommands::Search {
            query,
            owner,
            limit,
            json,
        } => search(&query, owner.as_deref(), limit, json),
        SkillCommands::Add(args) => add(args).await,
        SkillCommands::List { path, global, json } => list(path, global, json),
        SkillCommands::Update {
            names,
            path,
            global,
            yes,
        } => update(names, path, global, yes).await,
        SkillCommands::Remove {
            names,
            path,
            global,
        } => remove(names, path, global).await,
        SkillCommands::Publish { name, remote, path } => publish(&name, remote, path),
        SkillCommands::Audit {
            name,
            source,
            verdict,
            json,
            path,
            global,
        } => audit(name, source, verdict, json, path, global).await,
        SkillCommands::Brief { name, path, global } => brief(name, path, global),
        SkillCommands::ReportInstalls { on, off, path } => report(on, off, path),
    }
}

fn search(query: &str, owner: Option<&str>, limit: u32, json: bool) -> Result<(), String> {
    if query.trim().chars().count() < 2 {
        return Err("search needs at least 2 characters".into());
    }
    let limit = limit.clamp(1, 50);
    let mut url = format!("{SEARCH_URL}?q={}&limit={limit}", encode(query));
    if let Some(owner) = owner.filter(|o| !o.is_empty()) {
        url.push_str("&owner=");
        url.push_str(&encode(owner));
    }
    let body = http_text(&url)?;
    let mut hits = parse_search_body(&body)?;
    ax_skill_catalog::attach_catalog_cards(&mut hits);
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&hits).unwrap_or_else(|_| "[]".into())
        );
        return Ok(());
    }
    if hits.is_empty() {
        println!("No skills matched.");
        return Ok(());
    }
    for hit in hits {
        let stars = hit
            .stars_label
            .as_deref()
            .map(|label| format!("  ★ {label}"))
            .unwrap_or_default();
        println!(
            "{}  {}  {} installs{stars}",
            hit.name, hit.source, hit.installs
        );
        if let Some(seen) = hit.first_seen.as_deref() {
            println!("  first seen {seen}");
        }
        if let Some(summary) = hit.summary.as_deref() {
            println!("  {summary}");
        }
        for audit in &hit.audits {
            println!("  {} {}  {}", audit.name, audit.status, audit.url);
        }
    }
    Ok(())
}

async fn add(args: SkillAddArgs) -> Result<(), String> {
    let root = resolve_project_root(args.path);
    let source = parse_source(&args.source)?;
    let dest = skills_dir(&root, args.global)?;
    let only = match args.skill.as_slice() {
        [] => None,
        [one] => Some(one.clone()),
        many => {
            for name in many {
                install_one(&root, &source, &dest, Some(name), args.global, args.yes).await?;
            }
            return Ok(());
        }
    };
    install_one(
        &root,
        &source,
        &dest,
        only.as_deref(),
        args.global,
        args.yes,
    )
    .await
}

async fn install_one(
    root: &Path,
    source: &SkillSource,
    dest: &Path,
    only: Option<&str>,
    global: bool,
    replace: bool,
) -> Result<(), String> {
    let fetched = fetch_source(source)?;
    let receipts = if let Some(index) = fetched.well_known_index.clone() {
        let name = only
            .map(str::to_string)
            .or_else(|| match source {
                SkillSource::WellKnown { skill_name, .. } => skill_name.clone(),
                _ => None,
            })
            .ok_or_else(|| "pass --skill NAME for this well-known source".to_string())?;
        let base = fetched.file_base.clone();
        vec![materialize_well_known(
            &index,
            &name,
            dest,
            replace,
            |file| http_bytes_same_origin(&well_known_file_url(&base, &name, file)?),
        )?]
    } else {
        let tree = only_path(source, &fetched.tree);
        install_tree(&tree, dest, only, replace)?
    };
    let mut lock = LockFile::load(&lock_path(root, global)?)?;
    let settings = load_settings(root).unwrap_or_default();
    for receipt in &receipts {
        println!(
            "installed {}\nsource {}\ncommit {}\nhash {}\ndir {}",
            receipt.name,
            fetched.source_id,
            fetched.sha,
            receipt.hash,
            receipt.directory.display()
        );
        lock.skills.insert(
            receipt.name.clone(),
            LockEntry {
                source: fetched.source_id.clone(),
                source_type: fetched.source_type.clone(),
                sha: fetched.sha.clone(),
                hash: receipt.hash.clone(),
                skill_path: receipt.directory.display().to_string(),
            },
        );
        if let Some(url) =
            install_report_url(settings.report_installs, &fetched.source_id, &receipt.name)
        {
            let _ = http_text(&url);
        }
    }
    lock.save(&lock_path(root, global)?)?;
    if !global {
        reindex(root).await;
    }
    Ok(())
}

struct Fetched {
    tree: PathBuf,
    sha: String,
    source_id: String,
    source_type: String,
    well_known_index: Option<String>,
    file_base: String,
    _temp: Option<tempfile::TempDir>,
}

fn fetch_source(source: &SkillSource) -> Result<Fetched, String> {
    match source {
        SkillSource::Local { path } => Ok(Fetched {
            tree: PathBuf::from(path),
            sha: "local".into(),
            source_id: path.clone(),
            source_type: "local".into(),
            well_known_index: None,
            file_base: String::new(),
            _temp: None,
        }),
        SkillSource::Git {
            clone_url,
            source_id,
            reference,
            host,
            ..
        } => {
            let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
            git_clone(clone_url, reference.as_deref(), temp.path())?;
            let sha = git_rev(temp.path()).unwrap_or_else(|_| "unknown".into());
            let source_type = match host {
                ax_skill_catalog::GitHost::GitHub => "github",
                ax_skill_catalog::GitHost::GitLab => "gitlab",
                ax_skill_catalog::GitHost::Azure => "azure",
            };
            Ok(Fetched {
                tree: temp.path().to_path_buf(),
                sha,
                source_id: source_id.clone(),
                source_type: source_type.into(),
                well_known_index: None,
                file_base: String::new(),
                _temp: Some(temp),
            })
        }
        SkillSource::WellKnown { base_url, .. } => {
            let (index_url, body) = fetch_index(base_url)?;
            let file_base = index_url.trim_end_matches("/index.json").to_string();
            Ok(Fetched {
                tree: PathBuf::new(),
                sha: "well-known".into(),
                source_id: base_url.clone(),
                source_type: "well-known".into(),
                well_known_index: Some(body),
                file_base,
                _temp: None,
            })
        }
    }
}

fn only_path(source: &SkillSource, tree: &Path) -> PathBuf {
    match source {
        SkillSource::Git {
            path: Some(path), ..
        } => tree.join(path),
        _ => tree.to_path_buf(),
    }
}

fn fetch_index(base: &str) -> Result<(String, String), String> {
    let base = base.trim_end_matches('/');
    let mut urls = Vec::new();
    if base.contains("/.well-known/") {
        urls.push(format!("{base}/index.json"));
    } else {
        urls.push(format!("{base}/.well-known/agent-skills/index.json"));
        urls.push(format!("{base}/.well-known/skills/index.json"));
    }
    let mut errors = Vec::new();
    for url in urls {
        match http_text(&url) {
            Ok(body) if body.contains("skills") => return Ok((url, body)),
            Ok(_) => errors.push(format!("{url} was not a skill index")),
            Err(err) => errors.push(err),
        }
    }
    Err(errors.join("; "))
}

fn list(path: Option<String>, global: bool, json: bool) -> Result<(), String> {
    let root = resolve_project_root(path);
    let lock = LockFile::load(&lock_path(&root, global)?)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&lock).unwrap_or_default()
        );
        return Ok(());
    }
    if lock.skills.is_empty() {
        println!("No catalog skills installed.");
        return Ok(());
    }
    for (name, entry) in &lock.skills {
        println!("{name}  {}  {}", entry.source, entry.hash);
    }
    Ok(())
}

async fn update(
    names: Vec<String>,
    path: Option<String>,
    global: bool,
    yes: bool,
) -> Result<(), String> {
    let root = resolve_project_root(path);
    let lock = LockFile::load(&lock_path(&root, global)?)?;
    let selected: Vec<String> = if names.is_empty() {
        lock.skills.keys().cloned().collect()
    } else {
        names
    };
    for name in selected {
        let entry = lock
            .skills
            .get(&name)
            .ok_or_else(|| format!("skill is not in the ax lock: {name}"))?;
        let source = parse_source(&entry.source)?;
        let dest = skills_dir(&root, global)?;
        install_one(&root, &source, &dest, Some(&name), global, yes).await?;
    }
    Ok(())
}

async fn remove(names: Vec<String>, path: Option<String>, global: bool) -> Result<(), String> {
    if names.is_empty() {
        return Err("name the skill to remove".into());
    }
    let root = resolve_project_root(path);
    let lock_file = lock_path(&root, global)?;
    let mut lock = LockFile::load(&lock_file)?;
    let dest = skills_dir(&root, global)?;
    for name in names {
        let entry = lock
            .skills
            .remove(&name)
            .ok_or_else(|| format!("ax did not install {name}"))?;
        let dir = dest.join(&name);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        }
        println!("removed {name} ({})", entry.source);
    }
    lock.save(&lock_file)?;
    if !global {
        reindex(&root).await;
    }
    Ok(())
}

fn publish(name: &str, remote: Option<String>, path: Option<String>) -> Result<(), String> {
    if name.is_empty() || name == "*" || name.contains(',') {
        return Err("publish one skill at a time".into());
    }
    let root = resolve_project_root(path);
    let mut settings = load_settings(&root).unwrap_or_default();
    let remote = remote.unwrap_or_else(|| settings.publish_remote.clone());
    if remote.is_empty() {
        return Err("pass --remote, or save a publish repository on the Skills page".into());
    }
    settings.publish_remote = remote.clone();
    save_settings(&root, &settings)?;
    let skills = skills_dir(&root, false)?;
    if remote.starts_with("well-known:") || Path::new(&remote).is_dir() {
        let output = remote.trim_start_matches("well-known:");
        let output = PathBuf::from(output);
        stage_one_skill(&skills, name, &output)?;
        println!("published {name} into {}", output.display());
        return Ok(());
    }
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    git_clone(&remote, None, temp.path())?;
    stage_one_skill(&skills, name, temp.path())?;
    git_publish(temp.path(), name)?;
    println!("published {name} to {remote}");
    Ok(())
}

fn report(on: bool, off: bool, path: Option<String>) -> Result<(), String> {
    if on == off {
        return Err("pass --on or --off".into());
    }
    let root = resolve_project_root(path);
    let mut settings = load_settings(&root).unwrap_or_default();
    settings.report_installs = on;
    save_settings(&root, &settings)?;
    println!(
        "Report installs to skills.sh: {}",
        if on { "on" } else { "off" }
    );
    Ok(())
}

async fn known_skill_names(root: &Path) -> Vec<String> {
    let Ok(ax) = ax_core::Ax::open(root).await else {
        return Vec::new();
    };
    ax.list_policy_skills()
        .await
        .map(|rows| rows.into_iter().map(|skill| skill.name).collect())
        .unwrap_or_default()
}

async fn stored_skill_raw(root: &Path, name: &str) -> Option<String> {
    let ax = ax_core::Ax::open(root).await.ok()?;
    let store = ax_policy::PolicyStore::new(ax.db_pool().clone(), root.to_path_buf());
    store
        .get_skill_doc(name)
        .await
        .ok()
        .flatten()
        .map(|doc| doc.raw)
}

async fn audit(
    name: String,
    source: Option<String>,
    verdict: Option<String>,
    json: bool,
    path: Option<String>,
    global: bool,
) -> Result<(), String> {
    require_one_skill(&name)?;
    let root = resolve_project_root(path);
    let mut report = if let Some(source) = source {
        audit_candidate(&source, &name)?
    } else {
        let names = known_skill_names(&root).await;
        let mut roots = ax_policy::hierarchy::policy_layer_dirs(&root);
        if global {
            if let Ok(dir) = skills_dir(&root, true) {
                if let Some(parent) = dir.parent() {
                    roots.push(parent.to_path_buf());
                }
            }
        }
        let presence = SkillPresence {
            project: &root,
            roots: &roots,
            database_names: &names,
        };
        let dir = roots.iter().find_map(|layer| {
            let dir = layer.join("skills").join(&name);
            dir.join("SKILL.md").is_file().then_some(dir)
        });
        let mut report = if let Some(dir) = dir {
            audit_directory_present(&dir, &presence)?
        } else if let Some(raw) = stored_skill_raw(&root, &name).await {
            audit_skill_text(&name, &raw, &presence)?
        } else {
            return Err(format!("skill is not installed: {name}"));
        };
        let lock = LockFile::load(&lock_path(&root, global)?)?;
        if let Some(entry) = lock.skills.get(&name) {
            report.source = entry.source.clone();
            report.sha = entry.sha.clone();
            report.hash = entry.hash.clone();
        }
        report
    };
    if let Some(verdict) = verdict {
        let raw = fs::read_to_string(&verdict).map_err(|e| e.to_string())?;
        report.verdict = Some(parse_verdict(&raw, &report.skill_md)?);
    }
    store_report(&audits_dir(&root, global)?, &report)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else {
        print_audit(&report);
    }
    Ok(())
}

fn audit_candidate(source_text: &str, name: &str) -> Result<SkillAuditReport, String> {
    let source = parse_source(source_text)?;
    let fetched = fetch_source(&source)?;
    let dir = if let Some(index) = fetched.well_known_index.clone() {
        let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let dest = temp.path().join("skills");
        let base = fetched.file_base.clone();
        materialize_well_known(&index, name, &dest, true, |file| {
            http_bytes_same_origin(&well_known_file_url(&base, name, file)?)
        })?;
        let mut report = audit_directory(&dest.join(name))?;
        report.candidate = true;
        report.source = fetched.source_id;
        report.sha = fetched.sha;
        return Ok(report);
    } else {
        find_skill_dir(&only_path(&source, &fetched.tree), name)?
    };
    let mut report = audit_directory(&dir)?;
    report.candidate = true;
    report.source = fetched.source_id;
    report.sha = fetched.sha;
    Ok(report)
}

fn print_audit(report: &SkillAuditReport) {
    let warns = report
        .mechanical
        .iter()
        .filter(|f| f.severity == "warn")
        .count();
    let fails = report
        .mechanical
        .iter()
        .filter(|f| f.severity == "fail")
        .count();
    println!("audited {}", report.name);
    if report.candidate {
        println!("candidate");
    }
    if !report.source.is_empty() {
        println!("source {}", report.source);
    }
    if !report.sha.is_empty() {
        println!("commit {}", report.sha);
    }
    if !report.hash.is_empty() {
        println!("hash {}", report.hash);
    }
    println!("dir {}", report.directory);
    println!("mechanical: {warns} warn, {fails} fail");
    for finding in &report.mechanical {
        println!(
            "{} {}: {}",
            finding.severity, finding.check, finding.evidence
        );
    }
    if let Some(verdict) = &report.verdict {
        println!(
            "verdict {} clarity {} trigger {} steps {} files {} safety {}",
            verdict.verdict,
            verdict.clarity,
            verdict.trigger,
            verdict.steps,
            verdict.files,
            verdict.safety
        );
    }
    println!("skill stays usable");
}

pub async fn audit_rule(
    id: String,
    verdict: Option<String>,
    json: bool,
    path: Option<String>,
) -> Result<(), String> {
    require_one_skill(&id)?;
    let root = resolve_project_root(path);
    let names = known_skill_names(&root).await;
    let roots = ax_policy::hierarchy::policy_layer_dirs(&root);
    let file = find_rule_file(&roots, &id)?;
    let mut report = audit_rule_present(
        &file,
        &SkillPresence {
            project: &root,
            roots: &roots,
            database_names: &names,
        },
    )?;
    if let Some(verdict) = verdict {
        let raw = fs::read_to_string(verdict).map_err(|e| e.to_string())?;
        report.verdict = Some(parse_verdict(&raw, &report.skill_md)?);
    }
    store_report(&root.join(".ax").join("rule-audits"), &report)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
        );
    } else {
        print!("{}", agent_brief(&report, &report.edit_file));
    }
    Ok(())
}

fn brief(name: Option<String>, path: Option<String>, global: bool) -> Result<(), String> {
    let root = resolve_project_root(path);
    let audits = audits_dir(&root, global)?;
    match name {
        Some(name) => {
            require_one_skill(&name)?;
            let report = load_latest(&audits, &name)?
                .or(load_latest(&root.join(".ax").join("rule-audits"), &name)?)
                .ok_or_else(|| format!("no audit for {name}"))?;
            let file = if report.edit_file.is_empty() {
                format!("{}/SKILL.md", report.directory)
            } else {
                report.edit_file.clone()
            };
            print!("{}", agent_brief(&report, &file));
        }
        None => {
            let text = project_briefs(&root)?;
            if text.is_empty() {
                println!("No skill or rule audits need edits.");
            } else {
                print!("{text}");
            }
        }
    }
    Ok(())
}

fn audits_dir(root: &Path, global: bool) -> Result<PathBuf, String> {
    if global {
        return Ok(home()?.join(".ax").join("skill-audits"));
    }
    Ok(root.join(".ax").join("skill-audits"))
}

fn skills_dir(root: &Path, global: bool) -> Result<PathBuf, String> {
    if global {
        return Ok(home()?.join(".agents").join("skills"));
    }
    let name = ax_policy::configured_agents_dir(root)
        .unwrap_or_else(|| ax_policy::DEFAULT_AGENTS_DIR.to_string());
    Ok(root.join(name).join("skills"))
}

fn lock_path(root: &Path, global: bool) -> Result<PathBuf, String> {
    if global {
        return Ok(home()?.join(".ax").join("skills-lock.json"));
    }
    Ok(root.join(".ax").join("skills-lock.json"))
}

fn home() -> Result<PathBuf, String> {
    ax_utils::paths::home_dir().ok_or_else(|| "home directory is not set".into())
}

async fn reindex(root: &Path) {
    if let Ok(ax) = ax_core::Ax::open(root).await {
        let _ = ax.index_policy(false).await;
    }
}

/// Shallow-clone `url`. The URL comes after `--` so git never reads it as an option.
fn git_clone(url: &str, reference: Option<&str>, dest: &Path) -> Result<(), String> {
    if url.trim().is_empty() || url.starts_with('-') {
        return Err(format!("not a git URL: {url}"));
    }
    let mut cmd = Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(reference) = reference {
        if reference.starts_with('-') {
            return Err(format!("not a git ref: {reference}"));
        }
        cmd.arg("--branch").arg(reference);
    }
    cmd.arg("--").arg(url).arg(dest);
    let output = cmd.output().map_err(|e| format!("git clone: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git clone failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn git_rev(dest: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dest)
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("git rev-parse failed".into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_publish(repo: &Path, name: &str) -> Result<(), String> {
    let add = Command::new("git")
        .args(["add", "--", &format!("skills/{name}")])
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !add.status.success() {
        return Err(String::from_utf8_lossy(&add.stderr).trim().to_string());
    }
    let commit = Command::new("git")
        .args(["commit", "-m", &format!("Publish skill {name}")])
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        if !stderr.contains("nothing to commit")
            && !String::from_utf8_lossy(&commit.stdout).contains("nothing to commit")
        {
            return Err(stderr.trim().to_string());
        }
    }
    let push = Command::new("git")
        .args(["push"])
        .current_dir(repo)
        .output()
        .map_err(|e| e.to_string())?;
    if !push.status.success() {
        return Err(format!(
            "git push failed: {}",
            String::from_utf8_lossy(&push.stderr).trim()
        ));
    }
    Ok(())
}

/// The URL of one file listed in a well-known skill index. A relative entry
/// resolves under the index; an absolute entry must share the index's origin
/// (scheme, host and port), so an index cannot point the download at another
/// server or downgrade it from https to http.
fn well_known_file_url(base: &str, name: &str, file: &str) -> Result<String, String> {
    if !(file.starts_with("http://") || file.starts_with("https://")) {
        return Ok(format!("{base}/{name}/{file}"));
    }
    let index = reqwest::Url::parse(base).map_err(|e| format!("bad index URL {base}: {e}"))?;
    let target = reqwest::Url::parse(file).map_err(|e| format!("bad file URL {file}: {e}"))?;
    if index.origin() != target.origin() {
        return Err(format!(
            "skill file {file} is not on the index origin {}",
            index.origin().ascii_serialization()
        ));
    }
    Ok(file.to_string())
}

fn http_text(url: &str) -> Result<String, String> {
    let bytes = http_bytes(url)?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

/// The largest download a skill command accepts.
const MAX_DOWNLOAD: u64 = 25 * 1024 * 1024;

/// GET `url`, reading at most [`MAX_DOWNLOAD`] bytes of body.
fn http_bytes(url: &str) -> Result<Vec<u8>, String> {
    fetch_bounded(url, reqwest::redirect::Policy::default())
}

/// GET a file listed in a well-known skill index. Redirects are followed only
/// while they stay on the origin (scheme, host and port) of the request, so a
/// same-origin entry cannot hand the download to another server or to http.
fn http_bytes_same_origin(url: &str) -> Result<Vec<u8>, String> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 {
            return attempt.error("too many redirects");
        }
        let same = attempt
            .previous()
            .first()
            .is_some_and(|first| first.origin() == attempt.url().origin());
        if same {
            attempt.follow()
        } else {
            let to = attempt.url().origin().ascii_serialization();
            attempt.error(format!("redirect to {to} leaves the index origin"))
        }
    });
    fetch_bounded(url, policy)
}

/// GET `url` with `policy` for redirects, reading at most [`MAX_DOWNLOAD`]
/// bytes of body.
fn fetch_bounded(url: &str, policy: reqwest::redirect::Policy) -> Result<Vec<u8>, String> {
    let fetch = move |url: &str| {
        let client = reqwest::blocking::Client::builder()
            .redirect(policy)
            .build()
            .map_err(|e| e.to_string())?;
        let response = client.get(url).send().map_err(|e| {
            let mut msg = e.to_string();
            let mut source = std::error::Error::source(&e);
            while let Some(inner) = source {
                msg.push_str(": ");
                msg.push_str(&inner.to_string());
                source = inner.source();
            }
            msg
        })?;
        if !response.status().is_success() {
            return Err(format!("GET {url} returned {}", response.status()));
        }
        let too_large = || "download is larger than 25 MiB".to_string();
        if response.content_length().is_some_and(|len| len > MAX_DOWNLOAD) {
            return Err(too_large());
        }
        ax_utils::read_to_end_bounded(response, MAX_DOWNLOAD).map_err(|e| {
            if e.kind() == std::io::ErrorKind::InvalidData {
                too_large()
            } else {
                e.to_string()
            }
        })
    };
    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::task::block_in_place(|| fetch(url))
    } else {
        fetch(url)
    }
}

fn encode(value: &str) -> String {
    let mut out = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serve one HTTP response: `head` (status line and headers), then
    /// `body_len` bytes of body, then keep the connection open for 40 s.
    fn stalling_server(head: &'static str, body_len: usize) -> String {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(head.as_bytes());
            let chunk = vec![b'a'; 64 * 1024];
            let mut sent = 0;
            while sent < body_len {
                let n = chunk.len().min(body_len - sent);
                if stream.write_all(&chunk[..n]).is_err() {
                    return;
                }
                sent += n;
            }
            std::thread::sleep(std::time::Duration::from_secs(40));
        });
        format!("http://{addr}/file")
    }

    #[test]
    fn http_bytes_rejects_a_declared_oversize_body_without_reading_it() {
        let url = stalling_server(
            "HTTP/1.1 200 OK\r\nContent-Length: 104857600\r\n\r\n",
            1024,
        );
        let started = std::time::Instant::now();
        let err = http_bytes(&url).unwrap_err();
        assert!(err.contains("25 MiB"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    #[test]
    fn http_bytes_stops_reading_an_endless_body_at_the_limit() {
        let url = stalling_server(
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n",
            26 * 1024 * 1024,
        );
        let started = std::time::Instant::now();
        let err = http_bytes(&url).unwrap_err();
        assert!(err.contains("25 MiB"), "{err}");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
    }

    /// Answer one request with `response`, return the server's base URL.
    fn one_shot_server(response: String) -> String {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{addr}")
    }

    #[test]
    fn well_known_file_fetch_refuses_a_redirect_to_another_origin() {
        let other = one_shot_server(
            "HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nevil!".to_string(),
        );
        let start = one_shot_server(format!(
            "HTTP/1.1 302 Found\r\nLocation: {other}/SKILL.md\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ));
        let err = http_bytes_same_origin(&format!("{start}/x/SKILL.md")).unwrap_err();
        assert!(err.contains("origin"), "{err}");
    }

    #[test]
    fn well_known_file_fetch_follows_a_same_origin_redirect() {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let replies = [
                "HTTP/1.1 302 Found\r\nLocation: /moved/SKILL.md\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
            ];
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        let got = http_bytes_same_origin(&format!("http://{addr}/x/SKILL.md")).unwrap();
        assert_eq!(got, b"ok");
    }

    #[test]
    fn well_known_file_urls_stay_on_the_index_origin() {
        let base = "https://skills.example.org/.well-known/agent-skills";
        assert_eq!(
            well_known_file_url(base, "x", "SKILL.md").unwrap(),
            "https://skills.example.org/.well-known/agent-skills/x/SKILL.md"
        );
        assert_eq!(
            well_known_file_url(base, "x", "https://skills.example.org/cdn/x/SKILL.md").unwrap(),
            "https://skills.example.org/cdn/x/SKILL.md"
        );
        let err = well_known_file_url(base, "x", "https://evil.example.net/SKILL.md").unwrap_err();
        assert!(err.contains("origin"), "{err}");
        let err =
            well_known_file_url(base, "x", "http://skills.example.org/x/SKILL.md").unwrap_err();
        assert!(err.contains("origin"), "{err}");
    }

    #[test]
    fn git_clone_refuses_a_url_that_looks_like_an_option() {
        let tmp = tempfile::tempdir().unwrap();
        let err = git_clone("--upload-pack=true", None, &tmp.path().join("x")).unwrap_err();
        assert!(err.contains("not a git URL"), "{err}");
        let err = git_clone(
            "https://example.com/r.git",
            Some("-x"),
            &tmp.path().join("y"),
        )
        .unwrap_err();
        assert!(err.contains("not a git ref"), "{err}");
    }
}
