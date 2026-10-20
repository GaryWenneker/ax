use std::path::PathBuf;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::workspace_state::WebHub;

pub fn routes() -> Router<WebHub> {
    Router::new()
        .route("/skills/catalog/search", get(search))
        .route("/skills/catalog/install", post(install))
        .route("/skills/catalog/installed", get(list_installed))
        .route(
            "/skills/catalog/settings",
            get(get_settings).put(put_settings),
        )
        .route("/skills/catalog/audits", get(list_audits))
        .route("/rules/catalog/audits", get(list_rule_audits))
        .route("/rules/{id}/audit", post(audit_rule_one))
        .route("/skills/catalog/audit", post(audit_candidate))
        .route("/skills/{name}/publish", post(publish_one))
        .route("/skills/{name}/audit", get(get_audit).post(audit_one))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct InstallBody {
    source: String,
    #[serde(default)]
    skill: Option<String>,
    #[serde(default)]
    yes: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsBody {
    #[serde(default)]
    report_installs: bool,
    #[serde(default)]
    publish_remote: String,
}

async fn search(
    State(hub): State<WebHub>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<ax_skill_catalog::SearchHit>>, (StatusCode, String)> {
    let _ = hub;
    if query.q.trim().chars().count() < 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            "search needs at least 2 characters".into(),
        ));
    }
    let owner = query.owner.clone();
    let limit = query.limit.unwrap_or(20);
    let q = query.q.clone();
    let hits = tokio::task::spawn_blocking(move || {
        let body = ax_skill_catalog::search_body(&q, owner.as_deref(), limit)?;
        let mut hits = ax_skill_catalog::parse_search_body(&body)?;
        ax_skill_catalog::attach_catalog_cards(&mut hits);
        Ok::<_, String>(hits)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::BAD_GATEWAY, e))?;
    Ok(Json(hits))
}

async fn get_settings(State(hub): State<WebHub>) -> Result<Json<Value>, (StatusCode, String)> {
    let root = hub.read().await.project_root.clone();
    let settings = ax_skill_catalog::load_settings(&root).map_err(bad)?;
    Ok(Json(json!({
        "reportInstalls": settings.report_installs,
        "publishRemote": settings.publish_remote,
    })))
}

async fn put_settings(
    State(hub): State<WebHub>,
    Json(body): Json<SettingsBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let root = hub.read().await.project_root.clone();
    let settings = ax_skill_catalog::CatalogSettings {
        report_installs: body.report_installs,
        publish_remote: body.publish_remote,
    };
    ax_skill_catalog::save_settings(&root, &settings).map_err(bad)?;
    Ok(Json(json!({
        "reportInstalls": settings.report_installs,
        "publishRemote": settings.publish_remote,
    })))
}

async fn list_installed(State(hub): State<WebHub>) -> Result<Json<Vec<String>>, (StatusCode, String)> {
    let root = hub.read().await.project_root.clone();
    let lock = ax_skill_catalog::LockFile::load(&root.join(".ax").join("skills-lock.json")).map_err(bad)?;
    Ok(Json(lock.installed_ids()))
}

async fn install(
    State(hub): State<WebHub>,
    Json(body): Json<InstallBody>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let root = hub.read().await.project_root.clone();
    let source = body.source.clone();
    let skill = body.skill.clone();
    let yes = body.yes;
    let root_for_io = root.clone();
    let receipts = tokio::task::spawn_blocking(move || {
        install_blocking(&root_for_io, &source, skill.as_deref(), yes)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(bad)?;
    if let Ok(ax) = ax_core::Ax::open(&root).await {
        let _ = ax.index_policy(false).await;
    }
    Ok(Json(json!({ "installed": receipts })))
}

#[derive(Deserialize, Default)]
struct AuditBody {
    #[serde(default)]
    verdict: Option<Value>,
}

#[derive(Deserialize)]
struct CandidateAuditBody {
    source: String,
    skill: String,
}

async fn list_audits(
    State(hub): State<WebHub>,
) -> Result<Json<Vec<ax_skill_catalog::AuditSummary>>, (StatusCode, String)> {
    let root = hub.read().await.project_root.clone();
    let summaries =
        ax_skill_catalog::list_latest(&root.join(".ax").join("skill-audits")).map_err(bad)?;
    Ok(Json(summaries))
}

async fn list_rule_audits(
    State(hub): State<WebHub>,
) -> Result<Json<Vec<ax_skill_catalog::AuditSummary>>, (StatusCode, String)> {
    let root = hub.read().await.project_root.clone();
    let summaries =
        ax_skill_catalog::list_latest(&root.join(".ax").join("rule-audits")).map_err(bad)?;
    Ok(Json(summaries))
}

async fn known_skill_names(hub: &WebHub) -> (PathBuf, Vec<String>) {
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    let mut names = Vec::new();
    if let Ok(skills) = ws.policy.store.list_skills().await {
        names.extend(skills.into_iter().map(|skill| skill.name));
    }
    drop(ws);
    if let Ok(path) = ax_global_db::global_db_path() {
        if let Ok(pool) = ax_global_db::open_and_init(&path).await {
            if let Ok(rows) = ax_global_db::policy::list_skill_payloads(&pool).await {
                names.extend(rows.into_iter().map(|(id, _)| id));
            }
        }
    }
    (root, names)
}

fn skill_dir_in_layers(roots: &[PathBuf], name: &str) -> Option<PathBuf> {
    roots.iter().find_map(|layer| {
        let dir = layer.join("skills").join(name);
        dir.join("SKILL.md").is_file().then_some(dir)
    })
}

async fn audit_rule_one(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
) -> Result<Json<ax_skill_catalog::SkillAuditReport>, (StatusCode, String)> {
    ax_skill_catalog::require_one_skill(&id).map_err(bad)?;
    let (root, names) = known_skill_names(&hub).await;
    let rule_id = id.clone();
    let report = tokio::task::spawn_blocking(move || {
        let roots = ax_policy::hierarchy::policy_layer_dirs(&root);
        let file = ax_skill_catalog::find_rule_file(&roots, &rule_id)?;
        let presence = ax_skill_catalog::SkillPresence {
            project: &root,
            roots: &roots,
            database_names: &names,
        };
        let report = ax_skill_catalog::audit_rule_present(&file, &presence)?;
        ax_skill_catalog::store_report(&root.join(".ax").join("rule-audits"), &report)?;
        Ok::<_, String>(report)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(bad)?;
    Ok(Json(report))
}

async fn get_audit(
    State(hub): State<WebHub>,
    Path(name): Path<String>,
) -> Result<Json<ax_skill_catalog::SkillAuditReport>, (StatusCode, String)> {
    ax_skill_catalog::require_one_skill(&name).map_err(bad)?;
    let root = hub.read().await.project_root.clone();
    match ax_skill_catalog::load_latest(&root.join(".ax").join("skill-audits"), &name)
        .map_err(bad)?
    {
        Some(report) => Ok(Json(report)),
        None => Err((StatusCode::NOT_FOUND, format!("no audit for {name}"))),
    }
}

async fn audit_one(
    State(hub): State<WebHub>,
    Path(name): Path<String>,
    Json(body): Json<AuditBody>,
) -> Result<Json<ax_skill_catalog::SkillAuditReport>, (StatusCode, String)> {
    ax_skill_catalog::require_one_skill(&name).map_err(bad)?;
    let (root, names) = known_skill_names(&hub).await;
    let stored = {
        let ws = hub.read().await;
        ws.policy
            .store
            .get_skill_doc(&name)
            .await
            .ok()
            .flatten()
            .map(|doc| doc.raw)
    };
    let skill_name = name.clone();
    let verdict = body.verdict.clone();
    let report = tokio::task::spawn_blocking(move || {
        audit_installed(&root, &skill_name, verdict, &names, stored)
    })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(bad)?;
    Ok(Json(report))
}

async fn audit_candidate(
    State(hub): State<WebHub>,
    Json(body): Json<CandidateAuditBody>,
) -> Result<Json<ax_skill_catalog::SkillAuditReport>, (StatusCode, String)> {
    ax_skill_catalog::require_one_skill(&body.skill).map_err(bad)?;
    let source = body.source.clone();
    let skill = body.skill.clone();
    let report = tokio::task::spawn_blocking(move || audit_source(&source, &skill))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(bad)?;
    let root = hub.read().await.project_root.clone();
    ax_skill_catalog::store_report(&root.join(".ax").join("skill-audits"), &report).map_err(bad)?;
    Ok(Json(report))
}

async fn publish_one(
    State(hub): State<WebHub>,
    Path(name): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    if name.is_empty() || name == "*" || name.contains(',') || name.contains(' ') {
        return Err((
            StatusCode::BAD_REQUEST,
            "publish one skill at a time".into(),
        ));
    }
    let root = hub.read().await.project_root.clone();
    let skill_name = name.clone();
    let message = tokio::task::spawn_blocking(move || publish_blocking(&root, &skill_name))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(bad)?;
    Ok(Json(json!({ "name": name, "message": message })))
}

fn bad(err: String) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, err)
}

fn install_blocking(
    root: &std::path::Path,
    source_text: &str,
    skill: Option<&str>,
    replace: bool,
) -> Result<Vec<Value>, String> {
    let source = ax_skill_catalog::parse_source(source_text)?;
    let name = ax_policy::configured_agents_dir(root)
        .unwrap_or_else(|| ax_policy::DEFAULT_AGENTS_DIR.to_string());
    let dest = root.join(name).join("skills");
    let fetched = ax_skill_catalog::fetch_source(&source)?;
    let settings = ax_skill_catalog::load_settings(root).unwrap_or_default();
    let receipts = if let Some(index) = fetched.well_known_index.clone() {
        let skill_name = skill
            .map(str::to_string)
            .or_else(|| match &source {
                ax_skill_catalog::SkillSource::WellKnown { skill_name, .. } => skill_name.clone(),
                _ => None,
            })
            .ok_or_else(|| "name the skill to install".to_string())?;
        let base = fetched.file_base.clone();
        vec![ax_skill_catalog::materialize_well_known(
            &index,
            &skill_name,
            &dest,
            replace,
            |file| {
                let url = if file.starts_with("http://") || file.starts_with("https://") {
                    file.to_string()
                } else {
                    format!("{base}/{skill_name}/{file}")
                };
                ax_skill_catalog::http_bytes(&url)
            },
        )?]
    } else {
        let tree = match &source {
            ax_skill_catalog::SkillSource::Git {
                path: Some(path), ..
            } => fetched.tree.join(path),
            _ => fetched.tree.clone(),
        };
        ax_skill_catalog::install_tree(&tree, &dest, skill, replace)?
    };
    let lock_path = root.join(".ax").join("skills-lock.json");
    let mut lock = ax_skill_catalog::LockFile::load(&lock_path)?;
    let mut out = Vec::new();
    for receipt in receipts {
        if let Some(url) = ax_skill_catalog::install_report_url(
            settings.report_installs,
            &fetched.source_id,
            &receipt.name,
        ) {
            let _ = ax_skill_catalog::http_text(&url);
        }
        lock.skills.insert(
            receipt.name.clone(),
            ax_skill_catalog::LockEntry {
                source: fetched.source_id.clone(),
                source_type: fetched.source_type.clone(),
                sha: fetched.sha.clone(),
                hash: receipt.hash.clone(),
                skill_path: receipt.directory.display().to_string(),
            },
        );
        out.push(json!({
            "name": receipt.name,
            "source": fetched.source_id,
            "commit": fetched.sha,
            "hash": receipt.hash,
            "directory": receipt.directory,
        }));
    }
    lock.save(&lock_path)?;
    Ok(out)
}

fn audit_installed(
    root: &std::path::Path,
    name: &str,
    verdict: Option<Value>,
    names: &[String],
    stored: Option<String>,
) -> Result<ax_skill_catalog::SkillAuditReport, String> {
    let roots = ax_policy::hierarchy::policy_layer_dirs(root);
    let presence = ax_skill_catalog::SkillPresence {
        project: root,
        roots: &roots,
        database_names: names,
    };
    let mut report = if let Some(dir) = skill_dir_in_layers(&roots, name) {
        ax_skill_catalog::audit_directory_present(&dir, &presence)?
    } else if let Some(raw) = stored {
        ax_skill_catalog::audit_skill_text(name, &raw, &presence)?
    } else if names.iter().any(|known| known.eq_ignore_ascii_case(name)) {
        return Err(format!("skill is in the database but has no body: {name}"));
    } else {
        return Err(format!("skill is not installed: {name}"));
    };
    let lock = ax_skill_catalog::LockFile::load(&root.join(".ax").join("skills-lock.json"))?;
    if let Some(entry) = lock.skills.get(name) {
        report.source = entry.source.clone();
        report.sha = entry.sha.clone();
        report.hash = entry.hash.clone();
    }
    if let Some(verdict) = verdict {
        let raw = serde_json::to_string(&verdict).map_err(|e| e.to_string())?;
        report.verdict = Some(ax_skill_catalog::parse_verdict(&raw, &report.skill_md)?);
    }
    ax_skill_catalog::store_report(&root.join(".ax").join("skill-audits"), &report)?;
    Ok(report)
}

fn audit_source(
    source_text: &str,
    name: &str,
) -> Result<ax_skill_catalog::SkillAuditReport, String> {
    let source = ax_skill_catalog::parse_source(source_text)?;
    let fetched = ax_skill_catalog::fetch_source(&source)?;
    let dir = if let Some(index) = fetched.well_known_index.clone() {
        let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
        let dest = temp.path().join("skills");
        let base = fetched.file_base.clone();
        ax_skill_catalog::materialize_well_known(&index, name, &dest, true, |file| {
            let url = if file.starts_with("http://") || file.starts_with("https://") {
                file.to_string()
            } else {
                format!("{base}/{name}/{file}")
            };
            ax_skill_catalog::http_bytes(&url)
        })?;
        let mut report = ax_skill_catalog::audit_directory(&dest.join(name))?;
        report.candidate = true;
        report.source = fetched.source_id;
        report.sha = fetched.sha;
        return Ok(report);
    } else {
        let tree = match &source {
            ax_skill_catalog::SkillSource::Git {
                path: Some(path), ..
            } => fetched.tree.join(path),
            _ => fetched.tree.clone(),
        };
        ax_skill_catalog::find_skill_dir(&tree, name)?
    };
    let mut report = ax_skill_catalog::audit_directory(&dir)?;
    report.candidate = true;
    report.source = fetched.source_id;
    report.sha = fetched.sha;
    Ok(report)
}

fn publish_blocking(root: &std::path::Path, name: &str) -> Result<String, String> {
    let mut settings = ax_skill_catalog::load_settings(root).unwrap_or_default();
    if settings.publish_remote.is_empty() {
        return Err("Set a publish repository before publishing this skill.".into());
    }
    let agents = ax_policy::configured_agents_dir(root)
        .unwrap_or_else(|| ax_policy::DEFAULT_AGENTS_DIR.to_string());
    let skills = root.join(agents).join("skills");
    let remote = settings.publish_remote.clone();
    if remote.starts_with("well-known:") || PathBuf::from(&remote).is_dir() {
        let output = PathBuf::from(remote.trim_start_matches("well-known:"));
        ax_skill_catalog::stage_one_skill(&skills, name, &output)?;
        return Ok(format!("published {name} into {}", output.display()));
    }
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    ax_skill_catalog::git_clone(&remote, None, temp.path())?;
    ax_skill_catalog::stage_one_skill(&skills, name, temp.path())?;
    ax_skill_catalog::git_publish(temp.path(), name)?;
    settings.publish_remote = remote.clone();
    ax_skill_catalog::save_settings(root, &settings)?;
    Ok(format!("published {name} to {remote}"))
}
