use std::sync::Arc;

use ax_policy::{
    finalize_proposal, propose_rule_from_prompt, CaptureProposal, MatchInput, PolicyStore,
    RuleFrontmatter, SkillFrontmatter, ValidationError,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
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
        .route("/skills", get(list_skills).post(create_skill))
        .route("/skills/{name}", get(get_skill).put(update_skill).delete(delete_skill))
        .route("/skills/{name}/enabled", patch(set_skill_enabled))
        .route("/match", post(match_prompt))
        .route("/capture", post(capture_prompt))
        .route("/reindex", post(reindex))
        .route("/export", post(export_policy))
        .route("/pack/status", get(pack_status))
        .route("/pack/export", post(pack_export))
        .route("/pack/import", post(pack_import))
        .route("/review", get(review_list))
        .route("/review/{id}", get(review_show))
        .route("/review/{id}/approve", post(review_approve))
        .route("/review/{id}/reject", post(review_reject))
        .route("/settings", get(policy_settings).put(put_policy_settings))
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
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicySettingsPayload {
    #[serde(default)]
    policy_sync: Option<bool>,
    #[serde(default)]
    require_review: Option<bool>,
}

async fn policy_settings(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    let root = ws.policy.store.project_root();
    let status = ax_policy::policy_storage_status(root);
    (
        StatusCode::OK,
        Json(PolicySettingsResponse {
            policy_sync: status.policy_sync,
            require_review: status.require_review,
            storage: status.effective,
        }),
    )
        .into_response()
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
    let status = ax_policy::policy_storage_status(root);
    (
        StatusCode::OK,
        Json(PolicySettingsResponse {
            policy_sync: status.policy_sync,
            require_review: status.require_review,
            storage: status.effective,
        }),
    )
        .into_response()
}

async fn list_rules(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ws.policy.store.list_rules().await {
        Ok(rules) => (StatusCode::OK, Json(serde_json::json!({ "rules": rules }))).into_response(),
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
        Ok(skills) => (StatusCode::OK, Json(serde_json::json!({ "skills": skills }))).into_response(),
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
                        "path": format!(".ax/policy/rules/{id}.mdc"),
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
