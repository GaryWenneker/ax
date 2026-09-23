use std::sync::Arc;

use ax_policy::{
    build_policy_zip, diff_policy_zip_item, index_policy, preview_policy_zip, restore_policy_zip,
    slug_package_filename, PackSpec, RestoreAction, ZipPkgError, ZIP_PACKAGE_MAX_BYTES,
    CaptureProposal, MatchInput, PolicyRuleDoc, PolicySkillDoc, PolicyStore, RuleFrontmatter,
    SkillFrontmatter, ValidationError, finalize_proposal, propose_rule_from_prompt, get_revision,
    list_revisions, record_restore_writes, parse_rule_file, parse_skill_file, serialize_rule,
    serialize_skill,
};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Json},
    routing::{get, patch, post},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::workspace_state::WebHub;

#[derive(Clone)]
pub struct PolicyApiState {
    pub store: Arc<PolicyStore>,
    pub readonly: bool,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<std::collections::HashMap<String, String>>,
}

#[derive(Deserialize)]
pub struct RulePayload {
    pub frontmatter: RuleFrontmatter,
    pub body: String,
}

#[derive(Deserialize)]
pub struct SkillPayload {
    pub frontmatter: SkillFrontmatter,
    pub body: String,
}

#[derive(Deserialize, Default)]
struct OriginQuery {
    #[serde(default)]
    origin: Option<String>,
    #[serde(default, alias = "projectId")]
    project_id: Option<i64>,
}

impl OriginQuery {
    fn is_global(&self) -> bool {
        self.origin.as_deref() == Some("global")
    }
}

#[derive(Deserialize)]
pub struct MatchPayload {
    pub prompt: String,
    #[serde(default)]
    pub files: Vec<String>,
}

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/rules", get(list_rules).post(create_rule))
        .route("/rules/{id}", get(get_rule).put(update_rule).delete(delete_rule))
        .route("/rules/{id}/enabled", patch(set_rule_enabled))
        .route("/rules/{id}/storage", patch(set_rule_storage))
        .route("/rules/{id}/revisions", get(list_rule_revisions))
        .route(
            "/rules/{id}/revisions/{revId}/restore",
            post(restore_rule_revision),
        )
        .route("/skills", get(list_skills).post(create_skill))
        .route("/skills/{name}", get(get_skill).put(update_skill).delete(delete_skill))
        .route("/skills/{name}/enabled", patch(set_skill_enabled))
        .route("/skills/{name}/storage", patch(set_skill_storage))
        .route("/skills/{name}/revisions", get(list_skill_revisions))
        .route(
            "/skills/{name}/revisions/{revId}/restore",
            post(restore_skill_revision),
        )
        .route("/relocate", post(relocate_policy))
        .route("/copies/delete", post(delete_policy_copy))
        .route("/match", post(match_prompt))
        .route("/capture", post(capture_prompt))
        .route("/reindex", post(reindex))
        .route("/export", post(export_policy))
        .route("/pack/status", get(pack_status))
        .route("/pack/export", post(pack_export))
        .route("/pack/import", post(pack_import))
        .route("/package", post(create_zip_package))
        .route("/package/preview", post(preview_zip_package))
        .route("/package/restore", post(restore_zip_package))
        .route("/package/diff", post(diff_zip_package))
        .route("/review", get(review_list))
        .route("/review/{id}", get(review_show))
        .route("/review/{id}/approve", post(review_approve))
        .route("/review/{id}/reject", post(review_reject))
        .route("/settings", get(policy_settings).put(put_policy_settings))
        .layer(DefaultBodyLimit::max(ZIP_PACKAGE_MAX_BYTES))
        .with_state(hub)
}

#[derive(Deserialize)]
struct EnabledPayload {
    enabled: bool,
}

async fn set_rule_enabled(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Json(payload): Json<EnabledPayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ws.policy.store.set_enabled(&id, payload.enabled).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true, "id": id, "enabled": payload.enabled }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn set_skill_enabled(
    State(hub): State<WebHub>,
    Path(name): Path<String>,
    Json(payload): Json<EnabledPayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ws.policy.store.set_enabled(&name, payload.enabled).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true, "name": name, "enabled": payload.enabled }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoragePayload {
    storage: String,
    #[serde(default)]
    keep_file: bool,
}

async fn set_rule_storage(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Json(payload): Json<StoragePayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let Some(target) = ax_policy::PolicyStorage::parse(&payload.storage) else {
        return err(StatusCode::BAD_REQUEST, "storage must be files or database");
    };
    let ws = hub.read().await;
    match ws
        .policy
        .store
        .set_item_storage(&id, target, payload.keep_file)
        .await
    {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) if e.to_string().contains("not found") => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn set_skill_storage(
    State(hub): State<WebHub>,
    Path(name): Path<String>,
    Json(payload): Json<StoragePayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let Some(target) = ax_policy::PolicyStorage::parse(&payload.storage) else {
        return err(StatusCode::BAD_REQUEST, "storage must be files or database");
    };
    let ws = hub.read().await;
    match ws
        .policy
        .store
        .set_item_storage(&name, target, payload.keep_file)
        .await
    {
        Ok(v) => (StatusCode::OK, Json(v)).into_response(),
        Err(e) if e.to_string().contains("not found") => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn pack_status(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ax_policy::pack_status(ws.policy.store.pool(), ws.policy.store.project_root()).await {
        Ok(s) => (StatusCode::OK, Json(s)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn pack_export(State(hub): State<WebHub>) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ax_policy::export_pack(ws.policy.store.pool(), ws.policy.store.project_root(), "shared", None)
        .await
    {
        Ok(r) => (StatusCode::OK, Json(r)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

#[derive(Deserialize)]
struct PackImportPayload {
    #[serde(default)]
    force: bool,
}

async fn pack_import(
    State(hub): State<WebHub>,
    Json(payload): Json<PackImportPayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ax_policy::import_pack(
        ws.policy.store.pool(),
        ws.policy.store.project_root(),
        None,
        payload.force,
    )
    .await
    {
        Ok(r) => (StatusCode::OK, Json(r)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn review_list(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ax_policy::list_pending(ws.policy.store.project_root()) {
        Ok(items) => (StatusCode::OK, Json(serde_json::json!({ "items": items }))).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn review_show(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ax_policy::pending_diff(ws.policy.store.project_root(), &id) {
        Ok(diff) => (StatusCode::OK, Json(diff)).into_response(),
        Err(e) => err(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn review_approve(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ax_policy::approve_pending(ws.policy.store.pool(), ws.policy.store.project_root(), &id)
        .await
    {
        Ok(r) => (StatusCode::OK, Json(r)).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn review_reject(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ax_policy::reject_pending(ws.policy.store.pool(), ws.policy.store.project_root(), &id)
        .await
    {
        Ok(r) => (StatusCode::OK, Json(r)).into_response(),
        Err(e) => err(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicySettingsResponse {
    policy_sync: bool,
    require_review: bool,
    storage: String,
    roots: Vec<ax_policy::PolicyRoot>,
    agents_dir: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicySettingsPayload {
    #[serde(default)]
    policy_sync: Option<bool>,
    #[serde(default)]
    require_review: Option<bool>,
    /// Project default storage: `files` | `database`. Does not rewrite per-item overrides.
    #[serde(default)]
    storage: Option<String>,
    /// Folder name for on-disk rules and skills. Default `.agents`.
    #[serde(default)]
    agents_dir: Option<String>,
}

fn settings_response(root: &std::path::Path) -> PolicySettingsResponse {
    let status = ax_policy::policy_storage_status(root);
    PolicySettingsResponse {
        policy_sync: status.policy_sync,
        require_review: status.require_review,
        storage: status.effective,
        roots: status.roots,
        agents_dir: ax_policy::agents_dir_name(root),
    }
}

async fn policy_settings(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    let root = ws.policy.store.project_root();
    (StatusCode::OK, Json(settings_response(root))).into_response()
}

async fn put_policy_settings(
    State(hub): State<WebHub>,
    Json(payload): Json<PolicySettingsPayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    let root = ws.policy.store.project_root();
    if let Some(v) = payload.policy_sync {
        if let Err(e) = ax_policy::write_project_policy_sync(root, v) {
            return err(StatusCode::INTERNAL_SERVER_ERROR, &e);
        }
        // Hooks pick up policySync on the next `ax sync` / `ax init`.
    }
    if let Some(v) = payload.require_review {
        if let Err(e) = ax_policy::write_project_require_review(root, v) {
            return err(StatusCode::INTERNAL_SERVER_ERROR, &e);
        }
    }
    if let Some(ref s) = payload.storage {
        let Some(mode) = ax_policy::PolicyStorage::parse(s) else {
            return err(StatusCode::BAD_REQUEST, "storage must be files or database");
        };
        if let Err(e) = ax_policy::write_project_policy_storage(root, mode) {
            return err(StatusCode::INTERNAL_SERVER_ERROR, &e);
        }
    }
    if let Some(ref name) = payload.agents_dir {
        if let Err(e) = ax_policy::set_agents_dir(root, name) {
            let status = if e.contains("already exists") {
                StatusCode::CONFLICT
            } else {
                StatusCode::BAD_REQUEST
            };
            return err(status, &e);
        }
    }
    (StatusCode::OK, Json(settings_response(root))).into_response()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelocatePayload {
    kind: String,
    id: String,
    #[serde(default)]
    to: String,
    #[serde(default)]
    project_id: Option<i64>,
}

fn policy_kind(kind: &str) -> Option<ax_global_db::policy::PolicyKind> {
    match kind {
        "rule" => Some(ax_global_db::policy::PolicyKind::Rules),
        "skill" => Some(ax_global_db::policy::PolicyKind::Skills),
        _ => None,
    }
}

fn json_string_list(v: &serde_json::Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|i| i.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

fn json_opt_string(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn payload_to_rule(v: &serde_json::Value) -> Result<(RuleFrontmatter, String), String> {
    let body = v
        .get("body")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    if let Some(fm_val) = v.get("frontmatter") {
        if let Ok(doc) = serde_json::from_value::<ax_policy::PolicyRuleDoc>(v.clone()) {
            return Ok((doc.frontmatter, doc.body));
        }
        if let Ok(fm) = serde_json::from_value::<RuleFrontmatter>(fm_val.clone()) {
            return Ok((fm, body));
        }
    }
    let id = v
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "payload missing id".to_string())?
        .to_string();
    Ok((
        RuleFrontmatter {
            id,
            level: v
                .get("level")
                .and_then(|x| x.as_str())
                .unwrap_or("INFO")
                .to_string(),
            always_apply: v.get("alwaysApply").and_then(|x| x.as_bool()).unwrap_or(false),
            globs: json_string_list(v, "globs"),
            triggers: json_string_list(v, "triggers"),
            tags: json_string_list(v, "tags"),
            priority: v.get("priority").and_then(|x| x.as_i64()).unwrap_or(50) as i32,
            enabled: v.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true),
            status: v
                .get("status")
                .and_then(|x| x.as_str())
                .unwrap_or("approved")
                .to_string(),
            share: false,
            scope: v
                .get("scope")
                .and_then(|x| x.as_str())
                .unwrap_or("project")
                .to_string(),
            storage: json_opt_string(v, "storage"),
            source: json_opt_string(v, "source"),
            root_id: json_opt_string(v, "rootId"),
            group: json_opt_string(v, "group"),
        },
        body,
    ))
}

fn payload_to_skill(v: &serde_json::Value) -> Result<(SkillFrontmatter, String), String> {
    let body = v
        .get("body")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    if let Some(fm_val) = v.get("frontmatter") {
        if let Ok(doc) = serde_json::from_value::<ax_policy::PolicySkillDoc>(v.clone()) {
            return Ok((doc.frontmatter, doc.body));
        }
        if let Ok(fm) = serde_json::from_value::<SkillFrontmatter>(fm_val.clone()) {
            return Ok((fm, body));
        }
    }
    let name = v
        .get("name")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "payload missing name".to_string())?
        .to_string();
    Ok((
        SkillFrontmatter {
            name,
            description: v
                .get("description")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            always_apply: v.get("alwaysApply").and_then(|x| x.as_bool()).unwrap_or(false),
            triggers: json_string_list(v, "triggers"),
            tags: json_string_list(v, "tags"),
            priority: v.get("priority").and_then(|x| x.as_i64()).unwrap_or(50) as i32,
            context_task: json_opt_string(v, "contextTask"),
            enabled: v.get("enabled").and_then(|x| x.as_bool()).unwrap_or(true),
            status: v
                .get("status")
                .and_then(|x| x.as_str())
                .unwrap_or("approved")
                .to_string(),
            share: false,
            scope: v
                .get("scope")
                .and_then(|x| x.as_str())
                .unwrap_or("project")
                .to_string(),
            storage: json_opt_string(v, "storage"),
            source: json_opt_string(v, "source"),
            root_id: json_opt_string(v, "rootId"),
            group: json_opt_string(v, "group"),
        },
        body,
    ))
}

async fn open_global_pool(
    root: &std::path::Path,
    project_id: Option<i64>,
) -> Result<(sqlx::SqlitePool, i64), String> {
    let gpath = ax_global_db::global_db_path().map_err(|e| e.to_string())?;
    let gpool = ax_global_db::open_and_init(&gpath)
        .await
        .map_err(|e| e.to_string())?;
    let pid = ax_global_db::policy::resolve_project_id(&gpool, root, project_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok((gpool, pid))
}

fn skill_doc_from_parts(fm: SkillFrontmatter, body: String, source_path: String) -> PolicySkillDoc {
    let raw = serialize_skill(&fm, &body);
    PolicySkillDoc {
        frontmatter: fm,
        body,
        raw,
        source_path,
        stub_path: None,
    }
}

fn rule_doc_from_parts(fm: RuleFrontmatter, body: String, source_path: String) -> PolicyRuleDoc {
    let raw = serialize_rule(&fm, &body);
    PolicyRuleDoc {
        frontmatter: fm,
        body,
        raw,
        source_path,
        stub_path: None,
    }
}

async fn relocate_policy(
    State(hub): State<WebHub>,
    Json(payload): Json<RelocatePayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let Some(kind) = policy_kind(&payload.kind) else {
        return err(StatusCode::BAD_REQUEST, "kind must be rule or skill");
    };
    if payload.id.is_empty() {
        return err(StatusCode::BAD_REQUEST, "id is required");
    }
    let ws = hub.read().await;
    let root = ws.policy.store.project_root().to_path_buf();
    let gpath = match ax_global_db::global_db_path() {
        Ok(p) => p,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let gpool = match ax_global_db::open_and_init(&gpath).await {
        Ok(p) => p,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let pid = match ax_global_db::policy::resolve_project_id(&gpool, &root, payload.project_id).await
    {
        Ok(id) => id,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    match payload.to.as_str() {
        "global" => {
            let exists = match kind {
                ax_global_db::policy::PolicyKind::Rules => {
                    ws.policy.store.get_rule_doc(&payload.id).await.ok().flatten().is_some()
                }
                ax_global_db::policy::PolicyKind::Skills => {
                    ws.policy.store.get_skill_doc(&payload.id).await.ok().flatten().is_some()
                }
            };
            if !exists {
                let parked = ax_global_db::policy::load_policy_item(&gpool, pid, kind, &payload.id)
                    .await
                    .ok()
                    .flatten();
                if let Some(parked) = parked {
                    let flat = ax_global_db::policy::flatten_list_item(parked, kind, &payload.id);
                    if let Err(e) =
                        ax_global_db::policy::upsert_policy_item(&gpool, pid, kind, &payload.id, &flat)
                            .await
                    {
                        return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
                    }
                    return (StatusCode::OK, Json(serde_json::json!({ "ok": true, "already": true }))).into_response();
                }
                return err(StatusCode::NOT_FOUND, "not found in this project");
            }
            let copy = match kind {
                ax_global_db::policy::PolicyKind::Rules => {
                    let doc = ws.policy.store.get_rule_doc(&payload.id).await.ok().flatten().unwrap();
                    ax_global_db::policy::flatten_list_item(
                        serde_json::to_value(&doc).unwrap_or(serde_json::json!({})),
                        kind,
                        &payload.id,
                    )
                }
                ax_global_db::policy::PolicyKind::Skills => {
                    let doc = ws.policy.store.get_skill_doc(&payload.id).await.ok().flatten().unwrap();
                    ax_global_db::policy::flatten_list_item(
                        serde_json::to_value(&doc).unwrap_or(serde_json::json!({})),
                        kind,
                        &payload.id,
                    )
                }
            };
            if let Err(e) =
                ax_global_db::policy::upsert_policy_item(&gpool, pid, kind, &payload.id, &copy).await
            {
                return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
            }
            let deleted = match kind {
                ax_global_db::policy::PolicyKind::Rules => ws.policy.store.delete_rule(&payload.id).await,
                ax_global_db::policy::PolicyKind::Skills => {
                    ws.policy.store.delete_skill(&payload.id).await
                }
            };
            match deleted {
                Ok(_) => (StatusCode::OK, Json(serde_json::json!({ "ok": true, "to": "global" }))).into_response(),
                Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
            }
        }
        "project" => {
            let already = match kind {
                ax_global_db::policy::PolicyKind::Rules => {
                    ws.policy.store.get_rule_doc(&payload.id).await.ok().flatten().is_some()
                }
                ax_global_db::policy::PolicyKind::Skills => {
                    ws.policy.store.get_skill_doc(&payload.id).await.ok().flatten().is_some()
                }
            };
            if already {
                return err(StatusCode::CONFLICT, "already exists in this project");
            }
            let Some(copy) = (match ax_global_db::policy::load_policy_item(&gpool, pid, kind, &payload.id)
                .await
            {
                Ok(v) => v,
                Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
            }) else {
                return err(StatusCode::NOT_FOUND, "not found in global.db");
            };
            let saved = match kind {
                ax_global_db::policy::PolicyKind::Rules => match payload_to_rule(&copy) {
                    Ok((fm, body)) => ws.policy.store.save_rule(fm, body).await.map(|_| ()),
                    Err(e) => return err(StatusCode::BAD_REQUEST, &e),
                },
                ax_global_db::policy::PolicyKind::Skills => match payload_to_skill(&copy) {
                    Ok((fm, body)) => ws.policy.store.save_skill(fm, body).await.map(|_| ()),
                    Err(e) => return err(StatusCode::BAD_REQUEST, &e),
                },
            };
            if let Err(v) = saved {
                return validation_err(v);
            }
            let _ = ax_global_db::policy::delete_policy_item(&gpool, pid, kind, &payload.id).await;
            (StatusCode::OK, Json(serde_json::json!({ "ok": true, "to": "project" }))).into_response()
        }
        _ => err(StatusCode::BAD_REQUEST, "to must be global or project"),
    }
}

async fn delete_policy_copy(
    State(hub): State<WebHub>,
    Json(payload): Json<RelocatePayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let Some(kind) = policy_kind(&payload.kind) else {
        return err(StatusCode::BAD_REQUEST, "kind must be rule or skill");
    };
    let ws = hub.read().await;
    let root = ws.policy.store.project_root().to_path_buf();
    drop(ws);
    let gpath = match ax_global_db::global_db_path() {
        Ok(p) => p,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let gpool = match ax_global_db::open_and_init(&gpath).await {
        Ok(p) => p,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let pid = match ax_global_db::policy::resolve_project_id(&gpool, &root, payload.project_id).await
    {
        Ok(id) => id,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    match ax_global_db::policy::delete_policy_item(&gpool, pid, kind, &payload.id).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_rules(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ws.policy.store.list_rules().await {
        Ok(rules) => {
            let listed = annotate_policy_list(
                &ws.project_root,
                rules,
                ax_global_db::policy::PolicyKind::Rules,
            )
            .await;
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "rules": listed,
                    "groups": ax_policy::skill_groups_json(),
                })),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_rule(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Query(q): Query<OriginQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    if q.is_global() {
        let root = ws.policy.store.project_root().to_path_buf();
        drop(ws);
        let (gpool, pid) = match open_global_pool(&root, q.project_id).await {
            Ok(v) => v,
            Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e),
        };
        return match ax_global_db::policy::load_policy_item(
            &gpool,
            pid,
            ax_global_db::policy::PolicyKind::Rules,
            &id,
        )
        .await
        {
            Ok(Some(v)) => match payload_to_rule(&v) {
                Ok((fm, body)) => (
                    StatusCode::OK,
                    Json(rule_doc_from_parts(
                        fm,
                        body,
                        format!("global.db:{pid}:{id}"),
                    )),
                )
                    .into_response(),
                Err(e) => err(StatusCode::BAD_REQUEST, &e),
            },
            Ok(None) => err(StatusCode::NOT_FOUND, "not found"),
            Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    }
    match ws.policy.store.get_rule_doc(&id).await {
        Ok(Some(doc)) => (StatusCode::OK, Json(doc)).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn create_rule(
    State(hub): State<WebHub>,
    Json(payload): Json<RulePayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ws.policy.store.save_rule(payload.frontmatter, payload.body).await {
        Ok(doc) => (StatusCode::CREATED, Json(doc)).into_response(),
        Err(v) => validation_err(v),
    }
}

async fn update_rule(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Query(q): Query<OriginQuery>,
    Json(payload): Json<RulePayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    if q.is_global() {
        let ws = hub.read().await;
        let root = ws.policy.store.project_root().to_path_buf();
        drop(ws);
        if payload.frontmatter.id != id {
            return err(StatusCode::BAD_REQUEST, "id mismatch");
        }
        let (gpool, pid) = match open_global_pool(&root, q.project_id).await {
            Ok(v) => v,
            Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e),
        };
        let doc = rule_doc_from_parts(
            payload.frontmatter,
            payload.body,
            format!("global.db:{pid}:{id}"),
        );
        let flat = ax_global_db::policy::flatten_list_item(
            serde_json::to_value(&doc).unwrap_or(serde_json::json!({})),
            ax_global_db::policy::PolicyKind::Rules,
            &id,
        );
        return match ax_global_db::policy::upsert_policy_item(
            &gpool,
            pid,
            ax_global_db::policy::PolicyKind::Rules,
            &id,
            &flat,
        )
        .await
        {
            Ok(()) => (StatusCode::OK, Json(doc)).into_response(),
            Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    }
    let ws = hub.read().await;
    let result = if payload.frontmatter.id != id {
        ws.policy
            .store
            .rename_rule(&id, payload.frontmatter, payload.body)
            .await
    } else {
        ws.policy
            .store
            .save_rule(payload.frontmatter, payload.body)
            .await
    };
    match result {
        Ok(doc) => (StatusCode::OK, Json(doc)).into_response(),
        Err(v) => validation_err(v),
    }
}

async fn delete_rule(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ws.policy.store.delete_rule(&id).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_rule_revisions(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ws.policy.store.get_rule_doc(&id).await {
        Ok(None) => return err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        Ok(Some(_)) => {}
    }
    match list_revisions(ws.policy.store.pool(), "rule", &id).await {
        Ok(revisions) => (StatusCode::OK, Json(serde_json::json!({ "revisions": revisions }))).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn restore_rule_revision(
    State(hub): State<WebHub>,
    Path((id, rev_id)): Path<(String, i64)>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ws.policy.store.get_rule_doc(&id).await {
        Ok(None) => return err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        Ok(Some(_)) => {}
    }
    let rev = match get_revision(ws.policy.store.pool(), rev_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    if rev.kind != "rule" || rev.item_id != id {
        return err(StatusCode::NOT_FOUND, "not found");
    }
    let parsed = match parse_rule_file(std::path::Path::new("revision.mdc"), &rev.body) {
        Ok(d) => d,
        Err(v) => return err(StatusCode::BAD_REQUEST, &v.error),
    };
    if parsed.frontmatter.id != id {
        return err(StatusCode::BAD_REQUEST, "revision id does not match rule");
    }
    match ws.policy.store.save_rule(parsed.frontmatter, parsed.body).await {
        Ok(doc) => (StatusCode::OK, Json(doc)).into_response(),
        Err(v) => validation_err(v),
    }
}

async fn list_skill_revisions(
    State(hub): State<WebHub>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    match ws.policy.store.get_skill_doc(&name).await {
        Ok(None) => return err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        Ok(Some(_)) => {}
    }
    match list_revisions(ws.policy.store.pool(), "skill", &name).await {
        Ok(revisions) => (StatusCode::OK, Json(serde_json::json!({ "revisions": revisions }))).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn restore_skill_revision(
    State(hub): State<WebHub>,
    Path((name, rev_id)): Path<(String, i64)>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ws.policy.store.get_skill_doc(&name).await {
        Ok(None) => return err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        Ok(Some(_)) => {}
    }
    let rev = match get_revision(ws.policy.store.pool(), rev_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    if rev.kind != "skill" || rev.item_id != name {
        return err(StatusCode::NOT_FOUND, "not found");
    }
    let parsed = match parse_skill_file(std::path::Path::new("revision.md"), &rev.body) {
        Ok(d) => d,
        Err(v) => return err(StatusCode::BAD_REQUEST, &v.error),
    };
    if parsed.frontmatter.name != name {
        return err(StatusCode::BAD_REQUEST, "revision name does not match skill");
    }
    match ws
        .policy
        .store
        .save_skill(parsed.frontmatter, parsed.body)
        .await
    {
        Ok(doc) => (StatusCode::OK, Json(doc)).into_response(),
        Err(v) => validation_err(v),
    }
}

async fn list_skills(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ws.policy.store.list_skills().await {
        Ok(skills) => {
            let listed = annotate_policy_list(
                &ws.project_root,
                skills,
                ax_global_db::policy::PolicyKind::Skills,
            )
            .await;
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "skills": listed,
                    "groups": ax_policy::skill_groups_json(),
                })),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_skill(
    State(hub): State<WebHub>,
    Path(name): Path<String>,
    Query(q): Query<OriginQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    if q.is_global() {
        let root = ws.policy.store.project_root().to_path_buf();
        drop(ws);
        let (gpool, pid) = match open_global_pool(&root, q.project_id).await {
            Ok(v) => v,
            Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e),
        };
        return match ax_global_db::policy::load_policy_item(
            &gpool,
            pid,
            ax_global_db::policy::PolicyKind::Skills,
            &name,
        )
        .await
        {
            Ok(Some(v)) => match payload_to_skill(&v) {
                Ok((fm, body)) => (
                    StatusCode::OK,
                    Json(skill_doc_from_parts(
                        fm,
                        body,
                        format!("global.db:{pid}:{name}"),
                    )),
                )
                    .into_response(),
                Err(e) => err(StatusCode::BAD_REQUEST, &e),
            },
            Ok(None) => err(StatusCode::NOT_FOUND, "not found"),
            Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    }
    match ws.policy.store.get_skill_doc(&name).await {
        Ok(Some(doc)) => (StatusCode::OK, Json(doc)).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn create_skill(
    State(hub): State<WebHub>,
    Json(payload): Json<SkillPayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ws.policy.store.save_skill(payload.frontmatter, payload.body).await {
        Ok(doc) => (StatusCode::CREATED, Json(doc)).into_response(),
        Err(v) => validation_err(v),
    }
}

async fn update_skill(
    State(hub): State<WebHub>,
    Path(name): Path<String>,
    Query(q): Query<OriginQuery>,
    Json(payload): Json<SkillPayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    if payload.frontmatter.name != name {
        return err(StatusCode::BAD_REQUEST, "name mismatch");
    }
    if q.is_global() {
        let ws = hub.read().await;
        let root = ws.policy.store.project_root().to_path_buf();
        drop(ws);
        let (gpool, pid) = match open_global_pool(&root, q.project_id).await {
            Ok(v) => v,
            Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e),
        };
        let doc = skill_doc_from_parts(
            payload.frontmatter,
            payload.body,
            format!("global.db:{pid}:{name}"),
        );
        let flat = ax_global_db::policy::flatten_list_item(
            serde_json::to_value(&doc).unwrap_or(serde_json::json!({})),
            ax_global_db::policy::PolicyKind::Skills,
            &name,
        );
        return match ax_global_db::policy::upsert_policy_item(
            &gpool,
            pid,
            ax_global_db::policy::PolicyKind::Skills,
            &name,
            &flat,
        )
        .await
        {
            Ok(()) => (StatusCode::OK, Json(doc)).into_response(),
            Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
    }
    let ws = hub.read().await;
    match ws.policy.store.save_skill(payload.frontmatter, payload.body).await {
        Ok(doc) => (StatusCode::OK, Json(doc)).into_response(),
        Err(v) => validation_err(v),
    }
}

async fn delete_skill(
    State(hub): State<WebHub>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ws.policy.store.delete_skill(&name).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn match_prompt(
    State(hub): State<WebHub>,
    Json(payload): Json<MatchPayload>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let input = MatchInput {
        prompt: payload.prompt,
        cwd: ws.policy.store.project_root().to_path_buf(),
        open_files: payload.files.iter().map(std::path::PathBuf::from).collect(),
        changed_files: vec![],
    };
    match ax_policy::match_policy(ws.policy.store.pool(), &input).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

#[derive(Deserialize)]
pub struct CapturePayload {
    pub prompt: String,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default = "default_capture_action")]
    pub action: String,
    #[serde(default)]
    pub rule: Option<RulePayload>,
}

fn default_capture_action() -> String {
    "propose".into()
}

async fn capture_prompt(
    State(hub): State<WebHub>,
    Json(payload): Json<CapturePayload>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    if payload.action == "save" {
        if hub.readonly {
            return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
        }
        let rule = match payload.rule {
            Some(r) => r,
            None => return err(StatusCode::BAD_REQUEST, "rule required for save action"),
        };
        match ws.policy.store.save_rule(rule.frontmatter.clone(), rule.body).await {
            Ok(doc) => {
                let id = doc.frontmatter.id.clone();
                let storage = match ws.policy.store.storage() {
                    ax_policy::PolicyStorage::Database => "database",
                    ax_policy::PolicyStorage::Files => "files",
                };
                (
                    StatusCode::CREATED,
                    Json(serde_json::json!({
                        "ok": true,
                        "action": "save",
                        "id": id,
                        "storage": storage,
                        "path": format!(".agents/rules/{id}.mdc"),
                    })),
                )
                    .into_response()
            }
            Err(v) => validation_err(v),
        }
    } else {
        let mut proposal = propose_rule_from_prompt(&payload.prompt, &payload.files);
        if !proposal.detected {
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "ok": false,
                    "action": "propose",
                    "detected": false,
                    "proposal": proposal,
                })),
            )
                .into_response();
        }

        let existing = match ws.policy.store.list_rules().await {
            Ok(rules) => rules.into_iter().map(|r| r.id).collect::<Vec<_>>(),
            Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        };
        proposal = finalize_proposal(proposal, &existing);

        capture_propose_response(proposal)
    }
}

fn capture_propose_response(proposal: CaptureProposal) -> axum::response::Response {
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "action": "propose",
            "detected": true,
            "proposal": proposal,
            "preview": proposal.preview,
            "questions": proposal.questions,
            "instruction": proposal.interview_instruction,
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
pub struct ExportPayload {
    #[serde(default = "default_export_dir")]
    pub out_dir: String,
}

fn default_export_dir() -> String {
    ".ax/policy/export".into()
}

async fn export_policy(
    State(hub): State<WebHub>,
    Json(payload): Json<ExportPayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    let out = ws.policy.store.project_root().join(&payload.out_dir);
    match ws.policy.store.export_to_files(&out).await {
        Ok(r) => (StatusCode::OK, Json(r)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn reindex(State(hub): State<WebHub>) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let ws = hub.read().await;
    match ws.policy.store.reindex(true).await {
        Ok(r) => (StatusCode::OK, Json(r)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

#[derive(Deserialize)]
struct ZipPackagePayload {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    #[serde(rename = "ruleIds")]
    rule_ids: Vec<String>,
    #[serde(default)]
    #[serde(rename = "skillNames")]
    skill_names: Vec<String>,
}

fn zip_err(e: ZipPkgError) -> axum::response::Response {
    match e {
        ZipPkgError::Empty | ZipPkgError::Unknown(_) => err(StatusCode::UNPROCESSABLE_ENTITY, &e.to_string()),
        ZipPkgError::BadZip(_) | ZipPkgError::TooLarge => err(StatusCode::BAD_REQUEST, &e.to_string()),
        ZipPkgError::Io(_) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn read_multipart_fields(
    multipart: &mut Multipart,
) -> Result<std::collections::HashMap<String, Vec<u8>>, String> {
    let mut fields = std::collections::HashMap::new();
    while let Some(field) = multipart.next_field().await.map_err(|e| e.to_string())? {
        let name = field.name().unwrap_or("").to_string();
        let bytes = field.bytes().await.map_err(|e| e.to_string())?;
        if !name.is_empty() {
            fields.insert(name, bytes.to_vec());
        }
    }
    Ok(fields)
}

async fn read_multipart_zip(multipart: &mut Multipart) -> Result<(Vec<u8>, Option<String>), String> {
    let fields = read_multipart_fields(multipart).await?;
    let zip = fields
        .get("package")
        .cloned()
        .ok_or_else(|| "missing package field".to_string())?;
    let decisions = fields
        .get("decisions")
        .map(|b| String::from_utf8_lossy(b).into_owned());
    Ok((zip, decisions))
}

async fn create_zip_package(
    State(hub): State<WebHub>,
    Json(payload): Json<ZipPackagePayload>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let spec = PackSpec {
        name: payload.name,
        description: payload.description,
        rule_ids: payload.rule_ids,
        skill_names: payload.skill_names,
        ax_version: env!("CARGO_PKG_VERSION").into(),
        package_version: None,
        author: None,
    };
    match build_policy_zip(ws.policy.store.project_root(), &spec) {
        Ok(bytes) => {
            let filename = slug_package_filename(&spec.name);
            axum::http::Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/zip")
                .header(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{filename}\""),
                )
                .body(Body::from(bytes))
                .unwrap_or_else(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "response"))
        }
        Err(e) => zip_err(e),
    }
}

async fn preview_zip_package(State(hub): State<WebHub>, mut multipart: Multipart) -> impl IntoResponse {
    let (bytes, _) = match read_multipart_zip(&mut multipart).await {
        Ok(v) => v,
        Err(e) => return err(StatusCode::BAD_REQUEST, &e),
    };
    let ws = hub.read().await;
    match preview_policy_zip(ws.policy.store.project_root(), &bytes) {
        Ok(p) => (StatusCode::OK, Json(p)).into_response(),
        Err(e) => zip_err(e),
    }
}

async fn restore_zip_package(State(hub): State<WebHub>, mut multipart: Multipart) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let (bytes, decisions_raw) = match read_multipart_zip(&mut multipart).await {
        Ok(v) => v,
        Err(e) => return err(StatusCode::BAD_REQUEST, &e),
    };
    let decisions: std::collections::HashMap<String, RestoreAction> = match decisions_raw {
        Some(s) if !s.trim().is_empty() => match serde_json::from_str(&s) {
            Ok(d) => d,
            Err(e) => return err(StatusCode::BAD_REQUEST, &format!("decisions: {e}")),
        },
        _ => std::collections::HashMap::new(),
    };
    let ws = hub.read().await;
    let root = ws.policy.store.project_root().to_path_buf();
    let pool = ws.policy.store.pool().clone();
    drop(ws);
    match restore_policy_zip(&root, &bytes, &decisions) {
        Ok(result) => {
            if let Err(e) = index_policy(&pool, &root, true).await {
                return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
            }
            if let Err(e) = record_restore_writes(&pool, &root, &result.written).await {
                return err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
            }
            (StatusCode::OK, Json(result)).into_response()
        }
        Err(e) => zip_err(e),
    }
}

async fn diff_zip_package(
    State(hub): State<WebHub>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let fields = match read_multipart_fields(&mut multipart).await {
        Ok(f) => f,
        Err(e) => return err(StatusCode::BAD_REQUEST, &e),
    };
    let bytes = match fields.get("package") {
        Some(b) if b.len() <= ZIP_PACKAGE_MAX_BYTES => b.clone(),
        Some(_) => return err(StatusCode::BAD_REQUEST, "zip exceeds 8 MiB limit"),
        None => return err(StatusCode::BAD_REQUEST, "missing package field"),
    };
    let kind = fields
        .get("kind")
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    let id = fields
        .get("id")
        .map(|b| String::from_utf8_lossy(b).into_owned())
        .unwrap_or_default();
    if kind.is_empty() || id.is_empty() {
        return err(StatusCode::BAD_REQUEST, "kind and id are required");
    }
    let ws = hub.read().await;
    let root = ws.policy.store.project_root().to_path_buf();
    drop(ws);
    match diff_policy_zip_item(&root, &bytes, &kind, &id) {
        Ok(diff) => (StatusCode::OK, Json(diff)).into_response(),
        Err(e) => zip_err(e),
    }
}

async fn annotate_policy_list<T: serde::Serialize>(
    project_root: &std::path::Path,
    items: Vec<T>,
    kind: ax_global_db::policy::PolicyKind,
) -> Vec<serde_json::Value> {
    let name = project_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project");
    let mut out: Vec<serde_json::Value> = items
        .into_iter()
        .map(|item| {
            let v = serde_json::to_value(item).unwrap_or(serde_json::json!({}));
            ax_global_db::policy::annotate_local(v, name, kind)
        })
        .collect();
    if let Ok(gpath) = ax_global_db::global_db_path() {
        if gpath.is_file() {
            if let Ok(gpool) = ax_global_db::open_and_init(&gpath).await {
                let _ = ax_global_db::policy::extend_with_foreign_policy(
                    &gpool,
                    project_root,
                    kind,
                    &mut out,
                )
                .await;
            }
        }
    }
    out
        .into_iter()
        .map(|item| {
            let id = match kind {
                ax_global_db::policy::PolicyKind::Rules => item
                    .pointer("/id")
                    .or_else(|| item.pointer("/frontmatter/id"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                ax_global_db::policy::PolicyKind::Skills => item
                    .pointer("/name")
                    .or_else(|| item.pointer("/frontmatter/name"))
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
            };
            ax_global_db::policy::flatten_list_item(item, kind, &id)
        })
        .collect()
}

fn err(status: StatusCode, msg: &str) -> axum::response::Response {
    (
        status,
        Json(ApiError {
            error: msg.into(),
            fields: None,
        }),
    )
        .into_response()
}

fn validation_err(v: ValidationError) -> axum::response::Response {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: v.error,
            fields: Some(v.fields),
        }),
    )
        .into_response()
}
