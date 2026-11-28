//! Owner classification of listed documents, the stored plans under each spec for the list,
//! and import of a spec that has no plan yet.

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::json;
use sqlx::SqlitePool;

use crate::spec_hierarchy::{import_spec_without_plan, NewSpecWithoutPlan};

/// Append-only: the newest row per document is its classification; older rows stay as history.
const TYPES_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS spec_document_types (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    spec_id TEXT NOT NULL,
    doc_type TEXT NOT NULL CHECK (doc_type IN ('spec', 'plan')),
    actor TEXT NOT NULL,
    reason TEXT NOT NULL,
    created_at INTEGER NOT NULL
)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedPlan {
    pub id: String,
    pub title: String,
    pub latest_version: i64,
    pub latest_status: String,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

async fn table_exists(pool: &SqlitePool, name: &str) -> Result<bool, String> {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .map(|count| count > 0)
    .map_err(|error| error.to_string())
}

/// The newest owner classification per document. Reading never creates the table.
pub async fn document_types(pool: &SqlitePool) -> Result<HashMap<String, String>, String> {
    if !table_exists(pool, "spec_document_types").await? {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT spec_id, doc_type FROM spec_document_types AS t
         WHERE seq = (SELECT MAX(seq) FROM spec_document_types WHERE spec_id = t.spec_id)",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows.into_iter().collect())
}

/// Plans stored under each spec, oldest first. Reading never creates the hierarchy tables.
pub async fn plans_by_spec(pool: &SqlitePool) -> Result<HashMap<String, Vec<ListedPlan>>, String> {
    if !table_exists(pool, "plans").await? || !table_exists(pool, "plan_revisions").await? {
        return Ok(HashMap::new());
    }
    let rows: Vec<(String, String, String, i64, String)> = sqlx::query_as(
        "SELECT p.owner_spec_id, p.id, p.title, r.version, r.status
         FROM plans AS p
         JOIN plan_revisions AS r ON r.plan_id = p.id
          AND r.version = (SELECT MAX(version) FROM plan_revisions WHERE plan_id = p.id)
         ORDER BY p.owner_spec_id, p.created_at, p.rowid",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let mut plans: HashMap<String, Vec<ListedPlan>> = HashMap::new();
    for (spec_id, id, title, latest_version, latest_status) in rows {
        plans.entry(spec_id).or_default().push(ListedPlan {
            id,
            title,
            latest_version,
            latest_status,
        });
    }
    Ok(plans)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClassifyBody {
    #[serde(default)]
    ids: Vec<String>,
    #[serde(default)]
    all_plan_mode: bool,
    doc_type: String,
    actor: String,
    reason: String,
}

/// Records an owner decision that documents are specs or plans. `allPlanMode` selects every
/// document with a plan-mode revision. Body: `{ ids | allPlanMode, docType, actor, reason }`.
pub async fn classify_documents_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    match classify(pool, body).await {
        Ok(count) => json!({ "ok": true, "classified": count }),
        Err(error) => json!({ "ok": false, "error": error }),
    }
}

async fn classify(pool: &SqlitePool, body: serde_json::Value) -> Result<usize, String> {
    let parsed: ClassifyBody = serde_json::from_value(body).map_err(|error| error.to_string())?;
    if parsed.doc_type != "spec" && parsed.doc_type != "plan" {
        return Err("docType must be spec or plan".into());
    }
    if parsed.actor.trim().is_empty() || parsed.reason.trim().is_empty() {
        return Err("an actor and a reason are required".into());
    }
    let explicit = !parsed.ids.is_empty();
    if explicit == parsed.all_plan_mode {
        return Err("pass either ids or allPlanMode".into());
    }
    crate::spec_registry::ensure_spec_schema(pool).await?;
    let ids: Vec<String> = if explicit {
        parsed.ids
    } else {
        sqlx::query_scalar(
            "SELECT DISTINCT spec_id FROM spec_revisions WHERE source = 'plan_mode' ORDER BY spec_id",
        )
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?
    };
    for id in &ids {
        let known: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM specs WHERE id = ?1")
            .bind(id)
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?;
        if known == 0 {
            return Err(format!("unknown document {id}"));
        }
    }
    sqlx::query(TYPES_SCHEMA)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    let created_at = now_ms();
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    for id in &ids {
        sqlx::query(
            "INSERT INTO spec_document_types (spec_id, doc_type, actor, reason, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(id)
        .bind(&parsed.doc_type)
        .bind(parsed.actor.trim())
        .bind(parsed.reason.trim())
        .bind(created_at)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    }
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(ids.len())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportBody {
    project_id: String,
    spec_id: String,
    spec_title: String,
    spec_body: String,
    session_id: String,
    turn_id: String,
}

/// Imports a spec without a plan. It is stored as a draft that needs a plan.
pub async fn import_spec_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: ImportBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return json!({ "ok": false, "error": error.to_string() }),
    };
    let spec_id = parsed.spec_id.clone();
    let input = NewSpecWithoutPlan {
        project_id: parsed.project_id,
        spec_id: parsed.spec_id,
        spec_title: parsed.spec_title,
        spec_body: parsed.spec_body,
        session_id: parsed.session_id,
        turn_id: parsed.turn_id,
    };
    match import_spec_without_plan(pool, input).await {
        Ok(()) => json!({ "ok": true, "specId": spec_id, "specVersion": 1 }),
        Err(error) => json!({ "ok": false, "error": error }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec_hierarchy::{
        create_alternative_plan, create_spec_with_plan, NewAlternativePlan, NewSpecPlan,
    };
    use crate::spec_registry::{ensure_spec_schema, list_specs};

    async fn open() -> (tempfile::TempDir, SqlitePool) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join(".ax")).unwrap();
        let database = ax_db::Database::open(&dir.path().join(".ax/ax.db"))
            .await
            .unwrap();
        let pool = database.pool().clone();
        std::mem::forget(database);
        (dir, pool)
    }

    async fn plan_mode_document(pool: &SqlitePool, id: &str) {
        ensure_spec_schema(pool).await.unwrap();
        sqlx::query(
            "INSERT INTO specs (id, project_id, title, markdown_path, created_at)
             VALUES (?1, 'local', ?1, 'docs/specs/' || ?1 || '/SPEC.md', 1)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO spec_revisions
             (spec_id, version, status, body, content_hash, source, session_id, turn_id, based_on, created_at)
             VALUES (?1, 1, 'draft', '# Plan', 'hash', 'plan_mode', NULL, NULL, NULL, 1)",
        )
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn listed(pool: &SqlitePool, id: &str) -> crate::spec_registry::SpecListItem {
        list_specs(pool)
            .await
            .unwrap()
            .into_iter()
            .find(|item| item.id == id)
            .unwrap()
    }

    async fn type_rows(pool: &SqlitePool) -> i64 {
        if !table_exists(pool, "spec_document_types").await.unwrap() {
            return -1;
        }
        sqlx::query_scalar("SELECT COUNT(*) FROM spec_document_types")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn the_list_nests_stored_plans_under_their_spec() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(
            &pool,
            NewSpecPlan {
                project_id: "local".into(),
                spec_id: "spec-billing".into(),
                spec_title: "Billing".into(),
                spec_body: "Charge the recorded amount.".into(),
                plan_id: "plan-ledger".into(),
                plan_title: "Ledger plan".into(),
                plan_body: "Write the ledger first.".into(),
                session_id: "session-1".into(),
                turn_id: "turn-1".into(),
                agent_id: None,
            },
        )
        .await
        .unwrap();
        create_alternative_plan(
            &pool,
            NewAlternativePlan {
                project_id: "local".into(),
                spec_id: "spec-billing".into(),
                plan_id: "plan-invoices".into(),
                plan_title: "Invoice plan".into(),
                plan_body: "Send invoices first.".into(),
                session_id: "session-1".into(),
                turn_id: "turn-2".into(),
                agent_id: None,
            },
        )
        .await
        .unwrap();
        plan_mode_document(&pool, "old-plan").await;
        let spec = listed(&pool, "spec-billing").await;
        assert_eq!(spec.kind, "spec");
        assert_eq!(
            spec.plans
                .iter()
                .map(|plan| (
                    plan.id.as_str(),
                    plan.title.as_str(),
                    plan.latest_version,
                    plan.latest_status.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("plan-ledger", "Ledger plan", 1, "draft"),
                ("plan-invoices", "Invoice plan", 1, "draft"),
            ]
        );
        assert!(listed(&pool, "old-plan").await.plans.is_empty());
    }

    #[tokio::test]
    async fn plans_with_the_same_timestamp_stay_in_the_order_they_were_stored() {
        // Windows clocks tick in ~15 ms steps, so two plans stored in a row
        // often share created_at. The later one must still come second.
        let (_dir, pool) = open().await;
        create_spec_with_plan(
            &pool,
            NewSpecPlan {
                project_id: "local".into(),
                spec_id: "spec-billing".into(),
                spec_title: "Billing".into(),
                spec_body: "Charge the recorded amount.".into(),
                plan_id: "plan-ledger".into(),
                plan_title: "Ledger plan".into(),
                plan_body: "Write the ledger first.".into(),
                session_id: "session-1".into(),
                turn_id: "turn-1".into(),
                agent_id: None,
            },
        )
        .await
        .unwrap();
        create_alternative_plan(
            &pool,
            NewAlternativePlan {
                project_id: "local".into(),
                spec_id: "spec-billing".into(),
                plan_id: "plan-invoices".into(),
                plan_title: "Invoice plan".into(),
                plan_body: "Send invoices first.".into(),
                session_id: "session-1".into(),
                turn_id: "turn-2".into(),
                agent_id: None,
            },
        )
        .await
        .unwrap();
        sqlx::query("UPDATE plans SET created_at = 1")
            .execute(&pool)
            .await
            .unwrap();
        let spec = listed(&pool, "spec-billing").await;
        assert_eq!(
            spec.plans
                .iter()
                .map(|plan| plan.id.as_str())
                .collect::<Vec<_>>(),
            vec!["plan-ledger", "plan-invoices"]
        );
    }

    #[tokio::test]
    async fn a_classified_plan_mode_document_is_listed_as_a_spec() {
        let (_dir, pool) = open().await;
        plan_mode_document(&pool, "old-plan").await;
        plan_mode_document(&pool, "other-plan").await;
        let before = listed(&pool, "old-plan").await;
        assert_eq!(
            (before.kind.as_str(), before.detected_kind.as_str()),
            ("plan", "plan")
        );

        let one = classify_documents_from_json(
            &pool,
            json!({ "ids": ["old-plan"], "docType": "spec", "actor": "owner", "reason": "The owner said these are specifications." }),
        )
        .await;
        assert_eq!(one, json!({ "ok": true, "classified": 1 }));
        let after = listed(&pool, "old-plan").await;
        assert_eq!(
            (after.kind.as_str(), after.detected_kind.as_str()),
            ("spec", "plan")
        );
        assert_eq!(listed(&pool, "other-plan").await.kind, "plan");

        let all = classify_documents_from_json(
            &pool,
            json!({ "allPlanMode": true, "docType": "spec", "actor": "owner", "reason": "Bulk, confirmed by the owner." }),
        )
        .await;
        assert_eq!(all, json!({ "ok": true, "classified": 2 }));
        assert_eq!(listed(&pool, "other-plan").await.kind, "spec");
        assert_eq!(
            type_rows(&pool).await,
            3,
            "earlier classifications stay as history"
        );
    }

    #[tokio::test]
    async fn listing_creates_no_classification_table() {
        let (_dir, pool) = open().await;
        plan_mode_document(&pool, "old-plan").await;
        list_specs(&pool).await.unwrap();
        assert_eq!(type_rows(&pool).await, -1);
    }

    #[tokio::test]
    async fn classification_refuses_bad_input_and_writes_nothing() {
        let (_dir, pool) = open().await;
        plan_mode_document(&pool, "old-plan").await;
        let cases = [
            (
                json!({ "ids": ["old-plan"], "docType": "folder", "actor": "owner", "reason": "x" }),
                "docType",
            ),
            (
                json!({ "ids": ["old-plan"], "docType": "spec", "actor": "owner", "reason": " " }),
                "reason",
            ),
            (
                json!({ "ids": ["old-plan"], "allPlanMode": true, "docType": "spec", "actor": "owner", "reason": "x" }),
                "either",
            ),
            (
                json!({ "docType": "spec", "actor": "owner", "reason": "x" }),
                "either",
            ),
            (
                json!({ "ids": ["missing"], "docType": "spec", "actor": "owner", "reason": "x" }),
                "unknown document missing",
            ),
        ];
        for (body, expected) in cases {
            let result = classify_documents_from_json(&pool, body).await;
            assert_eq!(result["ok"], json!(false));
            let error = result["error"].as_str().unwrap();
            assert!(
                error.contains(expected),
                "{error} should mention {expected}"
            );
        }
        assert_eq!(type_rows(&pool).await, -1);
    }

    #[tokio::test]
    async fn an_imported_spec_has_no_plan_yet() {
        let (_dir, pool) = open().await;
        let result = import_spec_from_json(
            &pool,
            json!({
                "projectId": "local",
                "specId": "specs-hierarchy-ui",
                "specTitle": "Specs hierarchy UI",
                "specBody": "Specs first, then plans.",
                "sessionId": "session-1",
                "turnId": "turn-1",
            }),
        )
        .await;
        assert_eq!(
            result,
            json!({ "ok": true, "specId": "specs-hierarchy-ui", "specVersion": 1 })
        );
        let spec = listed(&pool, "specs-hierarchy-ui").await;
        assert_eq!(
            (spec.kind.as_str(), spec.latest_source.as_str()),
            ("spec", "import")
        );
        assert!(spec.plans.is_empty());
        let needs_plan: i64 =
            sqlx::query_scalar("SELECT needs_plan FROM specs WHERE id = 'specs-hierarchy-ui'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(needs_plan, 1);
        let again = import_spec_from_json(
            &pool,
            json!({ "projectId": "local", "specId": "specs-hierarchy-ui", "specTitle": "T", "specBody": "B", "sessionId": "s", "turnId": "t" }),
        )
        .await;
        assert_eq!(again["ok"], json!(false));
    }
}
