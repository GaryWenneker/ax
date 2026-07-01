use std::sync::Arc;

use ax_policy::{
    MatchInput, PolicyStore, RuleFrontmatter, SkillFrontmatter, ValidationError,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};

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

pub fn router(state: PolicyApiState) -> Router {
    Router::new()
        .route("/rules", get(list_rules).post(create_rule))
        .route("/rules/{id}", get(get_rule).put(update_rule).delete(delete_rule))
        .route("/skills", get(list_skills).post(create_skill))
        .route("/skills/{name}", get(get_skill).put(update_skill).delete(delete_skill))
        .route("/match", post(match_prompt))
        .route("/reindex", post(reindex))
        .route("/export", post(export_policy))
        .with_state(state)
}

async fn list_rules(State(s): State<PolicyApiState>) -> impl IntoResponse {
    match s.store.list_rules().await {
        Ok(rules) => (StatusCode::OK, Json(serde_json::json!({ "rules": rules }))).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_rule(State(s): State<PolicyApiState>, Path(id): Path<String>) -> impl IntoResponse {
    match s.store.get_rule_doc(&id).await {
        Ok(Some(doc)) => (StatusCode::OK, Json(serde_json::to_value(doc).unwrap())).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn create_rule(
    State(s): State<PolicyApiState>,
    Json(payload): Json<RulePayload>,
) -> impl IntoResponse {
    if s.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    match s.store.save_rule(payload.frontmatter, payload.body).await {
        Ok(doc) => (StatusCode::CREATED, Json(serde_json::to_value(doc).unwrap())).into_response(),
        Err(v) => validation_err(v),
    }
}

async fn update_rule(
    State(s): State<PolicyApiState>,
    Path(id): Path<String>,
    Json(payload): Json<RulePayload>,
) -> impl IntoResponse {
    if s.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    if payload.frontmatter.id != id {
        return err(StatusCode::BAD_REQUEST, "id mismatch");
    }
    match s.store.save_rule(payload.frontmatter, payload.body).await {
        Ok(doc) => (StatusCode::OK, Json(serde_json::to_value(doc).unwrap())).into_response(),
        Err(v) => validation_err(v),
    }
}

async fn delete_rule(State(s): State<PolicyApiState>, Path(id): Path<String>) -> impl IntoResponse {
    if s.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    match s.store.delete_rule(&id).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_skills(State(s): State<PolicyApiState>) -> impl IntoResponse {
    match s.store.list_skills().await {
        Ok(skills) => (StatusCode::OK, Json(serde_json::json!({ "skills": skills }))).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_skill(State(s): State<PolicyApiState>, Path(name): Path<String>) -> impl IntoResponse {
    match s.store.get_skill_doc(&name).await {
        Ok(Some(doc)) => (StatusCode::OK, Json(serde_json::to_value(doc).unwrap())).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn create_skill(
    State(s): State<PolicyApiState>,
    Json(payload): Json<SkillPayload>,
) -> impl IntoResponse {
    if s.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    match s.store.save_skill(payload.frontmatter, payload.body).await {
        Ok(doc) => (StatusCode::CREATED, Json(serde_json::to_value(doc).unwrap())).into_response(),
        Err(v) => validation_err(v),
    }
}

async fn update_skill(
    State(s): State<PolicyApiState>,
    Path(name): Path<String>,
    Json(payload): Json<SkillPayload>,
) -> impl IntoResponse {
    if s.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    if payload.frontmatter.name != name {
        return err(StatusCode::BAD_REQUEST, "name mismatch");
    }
    match s.store.save_skill(payload.frontmatter, payload.body).await {
        Ok(doc) => (StatusCode::OK, Json(serde_json::to_value(doc).unwrap())).into_response(),
        Err(v) => validation_err(v),
    }
}

async fn delete_skill(
    State(s): State<PolicyApiState>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if s.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    match s.store.delete_skill(&name).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn match_prompt(
    State(s): State<PolicyApiState>,
    Json(payload): Json<MatchPayload>,
) -> impl IntoResponse {
    let input = MatchInput {
        prompt: payload.prompt,
        cwd: s.store.project_root().to_path_buf(),
        open_files: payload.files.iter().map(std::path::PathBuf::from).collect(),
        changed_files: vec![],
    };
    match ax_policy::match_policy(s.store.pool(), &input).await {
        Ok(result) => (StatusCode::OK, Json(serde_json::to_value(result).unwrap())).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
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
    State(s): State<PolicyApiState>,
    Json(payload): Json<ExportPayload>,
) -> impl IntoResponse {
    if s.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    let out = s.store.project_root().join(&payload.out_dir);
    match s.store.export_to_files(&out).await {
        Ok(r) => (StatusCode::OK, Json(serde_json::to_value(r).unwrap())).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn reindex(State(s): State<PolicyApiState>) -> impl IntoResponse {
    if s.readonly {
        return err(StatusCode::FORBIDDEN, "AX_WEB_READONLY=1");
    }
    match s.store.reindex(true).await {
        Ok(r) => (StatusCode::OK, Json(serde_json::to_value(r).unwrap())).into_response(),
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
