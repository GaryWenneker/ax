use std::sync::Arc;

use ax_policy::{
    build_policy_zip, diff_policy_zip_item, index_policy, preview_policy_zip, restore_policy_zip,
    slug_package_filename, PackSpec, RestoreAction, ZipPkgError, ZIP_PACKAGE_MAX_BYTES,
    CaptureProposal, MatchInput, PolicyStore, RuleFrontmatter, SkillFrontmatter, ValidationError,
    finalize_proposal, propose_rule_from_prompt,
};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Multipart, Path, State},
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
        .route("/skills", get(list_skills).post(create_skill))
        .route("/skills/{name}", get(get_skill).put(update_skill).delete(delete_skill))
        .route("/skills/{name}/enabled", patch(set_skill_enabled))
        .route("/skills/{name}/storage", patch(set_skill_storage))
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
}

fn settings_response(root: &std::path::Path) -> PolicySettingsResponse {
    let status = ax_policy::policy_storage_status(root);
    PolicySettingsResponse {
        policy_sync: status.policy_sync,
        require_review: status.require_review,
        storage: status.effective,
        roots: status.roots,
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
    (StatusCode::OK, Json(settings_response(root))).into_response()
}

async fn list_rules(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ws.policy.store.list_rules().await {
        Ok(rules) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "rules": rules,
                "groups": ax_policy::skill_groups_json(),
            })),
        )
            .into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_rule(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    let ws = hub.read().await;
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
    Json(payload): Json<RulePayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
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

async fn list_skills(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ws.policy.store.list_skills().await {
        Ok(skills) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "skills": skills,
                "groups": ax_policy::skill_groups_json(),
            })),
        )
            .into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_skill(State(hub): State<WebHub>, Path(name): Path<String>) -> impl IntoResponse {
    let ws = hub.read().await;
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
    Json(payload): Json<SkillPayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    if payload.frontmatter.name != name {
        return err(StatusCode::BAD_REQUEST, "name mismatch");
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
