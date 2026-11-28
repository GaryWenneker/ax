//! Command Center list of versioned specs.

use axum::{
    extract::{Path, Query, State},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;

use crate::workspace_state::WebHub;

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/", get(list))
        .route("/with-plan", post(create_with_plan))
        .route("/alternative-plan", post(alternative_plan))
        .route("/plan-revision", post(plan_revision))
        .route("/spec-revision", post(spec_revision))
        .route("/session-link", post(session_link))
        .route("/plan-selection", post(plan_selection))
        .route("/plan-compare", post(plan_compare))
        .route("/tree", post(hierarchy_tree))
        .route("/provenance", post(plan_provenance))
        .route("/member-approval", post(member_approval))
        .route("/flag", post(hierarchy_flag))
        .route("/policy-link", post(policy_link))
        .route("/compatibility", post(compatibility))
        .route("/review", get(import_review))
        .route("/import", post(spec_import))
        .route("/classify", post(classify_documents))
        .route("/plan/{id}/revisions", get(plan_revisions))
        .route("/{id}/revisions", get(revisions).post(save))
        .route("/{id}/approve", post(approve))
        .with_state(hub)
}

enum HierarchyWrite {
    Create,
    Alternative,
    Revise,
    Link,
    Select,
    Compare,
    Tree,
    Provenance,
    Policy,
    Compatibility,
    SpecRevision,
    Import,
    Classify,
}

impl HierarchyWrite {
    /// Tree, Provenance and Compare only read, so read-only viewers may use them.
    fn writes(&self) -> bool {
        !matches!(self, Self::Tree | Self::Provenance | Self::Compare)
    }
}

async fn write_hierarchy(
    hub: WebHub,
    body: serde_json::Value,
    kind: HierarchyWrite,
) -> Json<serde_json::Value> {
    if hub.readonly && kind.writes() {
        return Json(serde_json::json!({ "ok": false, "error": "readonly" }));
    }
    let pool = hub.read().await.graph_pool.clone();
    let result = match kind {
        HierarchyWrite::Create => ax_usage::create_hierarchy_from_json(&pool, body).await,
        HierarchyWrite::Alternative => ax_usage::alternative_plan_from_json(&pool, body).await,
        HierarchyWrite::Revise => ax_usage::revise_plan_from_json(&pool, body).await,
        HierarchyWrite::Link => ax_usage::link_session_from_json(&pool, body).await,
        HierarchyWrite::Select => ax_usage::select_plan_from_json(&pool, body).await,
        HierarchyWrite::Compare => ax_usage::compare_plan_from_json(&pool, body).await,
        HierarchyWrite::Tree => ax_usage::list_hierarchy_from_json(&pool, body).await,
        HierarchyWrite::Provenance => ax_usage::provenance_from_json(&pool, body).await,
        HierarchyWrite::Policy => ax_usage::link_policy_from_json(&pool, body).await,
        HierarchyWrite::Compatibility => {
            ax_usage::confirm_compatibility_from_json(&pool, body).await
        }
        HierarchyWrite::SpecRevision => ax_usage::revise_spec_from_json(&pool, body).await,
        HierarchyWrite::Import => ax_usage::import_spec_from_json(&pool, body).await,
        HierarchyWrite::Classify => ax_usage::classify_documents_from_json(&pool, body).await,
    };
    Json(result)
}

async fn spec_import(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Import).await
}

async fn classify_documents(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Classify).await
}

async fn create_with_plan(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Create).await
}

async fn alternative_plan(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Alternative).await
}

async fn spec_revision(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::SpecRevision).await
}

async fn plan_revision(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Revise).await
}

async fn session_link(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Link).await
}

async fn plan_selection(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Select).await
}

async fn plan_compare(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Compare).await
}

async fn hierarchy_tree(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Tree).await
}

async fn plan_provenance(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Provenance).await
}

async fn compatibility(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Compatibility).await
}

async fn policy_link(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    write_hierarchy(hub, body, HierarchyWrite::Policy).await
}

async fn import_review(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let pool = hub.read().await.graph_pool.clone();
    Json(ax_usage::import_review_from_json(&pool).await)
}

async fn hierarchy_flag(
    State(hub): State<WebHub>,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    if body.get("enabled").is_some() && hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "readonly" }));
    }
    let pool = hub.read().await.graph_pool.clone();
    Json(ax_usage::hierarchy_flag_from_json(&pool, body).await)
}

async fn member_approval(
    State(hub): State<WebHub>,
    headers: axum::http::HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "readonly" }));
    }
    let ws = hub.read().await;
    let project = ws.graph_pool.clone();
    let root = ws.project_root.clone();
    drop(ws);
    let membership = match crate::policy::open_global_pool(&root, None).await {
        Ok((pool, _)) => pool,
        Err(error) => return Json(serde_json::json!({ "ok": false, "error": error })),
    };
    let member = match signed_in_member(&membership, &headers, hub.port).await {
        Ok(member) => member,
        Err(error) => return Json(serde_json::json!({ "ok": false, "error": error })),
    };
    let body = match ax_usage::bind_approval_actor(body, &member) {
        Ok(body) => body,
        Err(error) => return Json(serde_json::json!({ "ok": false, "error": error })),
    };
    Json(ax_usage::approve_from_json(&project, &membership, body).await)
}

async fn signed_in_member(
    db: &sqlx::SqlitePool,
    headers: &axum::http::HeaderMap,
    port: u16,
) -> Result<String, String> {
    if crate::web_auth::web_login_enabled(db).await {
        return crate::web_auth::session_member(db, headers)
            .await
            .map(|(id, _)| id)
            .ok_or_else(|| "sign in required".to_string());
    }
    if !crate::local_owner::browser_serves(headers, port) {
        return Err("local owner session required".into());
    }
    let members = ax_global_db::access::list_members(db)
        .await
        .map_err(|error| error.to_string())?;
    members
        .into_iter()
        .find(|member| {
            member.role == ax_global_db::access::MemberRole::Owner && member.status == "active"
        })
        .map(|member| member.id)
        .ok_or_else(|| "no active owner".to_string())
}

/// How long a listing reuses the last plan import before it rescans the plan
/// folders on disk. `?refresh=1` rescans at once.
const PLAN_IMPORT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// When each project root last imported its plans successfully.
fn last_plan_import(
) -> &'static std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, std::time::Instant>> {
    static LAST: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, std::time::Instant>>,
    > = std::sync::OnceLock::new();
    LAST.get_or_init(Default::default)
}

fn plan_import_due(root: &std::path::Path, refresh: bool) -> bool {
    if refresh {
        return true;
    }
    let last = last_plan_import()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    last.get(root)
        .is_none_or(|at| at.elapsed() >= PLAN_IMPORT_INTERVAL)
}

fn mark_plan_import(root: &std::path::Path) {
    last_plan_import()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .insert(root.to_path_buf(), std::time::Instant::now());
}

#[derive(Deserialize, Default)]
struct ListQuery {
    #[serde(default)]
    refresh: Option<String>,
}

async fn list(
    State(hub): State<WebHub>,
    Query(query): Query<ListQuery>,
) -> Json<serde_json::Value> {
    let refresh = query
        .refresh
        .as_deref()
        .is_some_and(|value| matches!(value, "1" | "true"));
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    let pool = ws.graph_pool.clone();
    drop(ws);
    if ax_usage::hierarchy_enabled(&pool).await.unwrap_or(false) {
        let project: String =
            sqlx::query_scalar("SELECT project_id FROM specs ORDER BY id LIMIT 1")
                .fetch_optional(&pool)
                .await
                .ok()
                .flatten()
                .unwrap_or_default();
        return Json(ax_usage::gated_list(&pool, &project).await);
    }
    // A read-only hub opens ax.db read-only, so it lists what is stored and
    // never imports. Otherwise plans are imported at most once per interval.
    if !hub.readonly && plan_import_due(&root, refresh) {
        if let Err(error) = ax_usage::import_workspace_plans(&pool, &root, "local").await {
            return Json(serde_json::json!({ "specs": [], "error": error }));
        }
        mark_plan_import(&root);
    }
    match ax_usage::list_specs(&pool).await {
        Ok(items) => Json(serde_json::json!({
            "specs": items.iter().map(|item| serde_json::json!({
                "id": item.id,
                "projectId": item.project_id,
                "title": item.title,
                "path": item.markdown_path,
                "sessions": item.sessions,
                "approvedVersion": item.approved_version,
                "latestStatus": item.latest_status,
                "revisionCount": item.revision_count,
                "kind": item.kind,
                "pointerVersion": item.pointer_version,
                "pointerHash": item.pointer_hash,
                "updatedAt": item.updated_at,
                "latestVersion": item.latest_version,
                "latestSource": item.latest_source,
                "latestBasedOn": item.latest_based_on,
                "detectedKind": item.detected_kind,
                "plans": item.plans.iter().map(|plan| serde_json::json!({
                    "id": plan.id,
                    "title": plan.title,
                    "latestVersion": plan.latest_version,
                    "latestStatus": plan.latest_status,
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>()
        })),
        Err(error) => Json(serde_json::json!({ "specs": [], "error": error })),
    }
}

async fn revisions(State(hub): State<WebHub>, Path(id): Path<String>) -> Json<serde_json::Value> {
    let pool = hub.read().await.graph_pool.clone();
    match ax_usage::list_revisions(&pool, &id).await {
        Ok(rows) => Json(serde_json::json!({
            "revisions": rows.iter().map(|row| serde_json::json!({
                "version": row.version,
                "status": row.status,
                "source": row.source,
                "body": row.body,
                "hash": row.content_hash,
                "basedOn": row.based_on,
                "createdAt": row.created_at,
            })).collect::<Vec<_>>()
        })),
        Err(error) => Json(serde_json::json!({ "revisions": [], "error": error })),
    }
}

async fn plan_revisions(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let pool = hub.read().await.graph_pool.clone();
    match ax_usage::list_plan_revisions(&pool, &id).await {
        Ok(rows) => Json(serde_json::json!({
            "revisions": rows.iter().map(|row| serde_json::json!({
                "version": row.version,
                "status": row.status,
                "source": row.source,
                "body": row.body,
                "hash": row.content_hash,
                "basedOn": row.based_on,
                "createdAt": row.created_at,
            })).collect::<Vec<_>>()
        })),
        Err(error) => Json(serde_json::json!({ "revisions": [], "error": error })),
    }
}

#[derive(Deserialize)]
struct SaveBody {
    #[serde(rename = "basedOn")]
    based_on: i64,
    body: String,
}

async fn save(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Json(body): Json<SaveBody>,
) -> Json<serde_json::Value> {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "readonly" }));
    }
    let pool = hub.read().await.graph_pool.clone();
    match ax_usage::save_revision(&pool, &id, body.based_on, &body.body).await {
        Ok(revision) => Json(serde_json::json!({
            "ok": true,
            "version": revision.version,
            "status": revision.status,
            "body": revision.body,
            "hash": revision.content_hash,
        })),
        Err(error) => Json(serde_json::json!({ "ok": false, "error": error })),
    }
}

#[derive(Deserialize)]
struct ApproveBody {
    version: i64,
}

async fn approve(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Json(body): Json<ApproveBody>,
) -> Json<serde_json::Value> {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "readonly" }));
    }
    let ws = hub.read().await;
    let root = ws.project_root.clone();
    let pool = ws.graph_pool.clone();
    drop(ws);
    match ax_usage::approve_revision(&pool, &root, &id, body.version, &[]).await {
        Ok(revision) => Json(serde_json::json!({
            "ok": true,
            "version": revision.version,
            "status": revision.status,
        })),
        Err(error) => Json(serde_json::json!({ "ok": false, "error": error })),
    }
}
