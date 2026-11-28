//! First-class specifications and plans.

use serde::Deserialize;
use sha2::Digest;
#[cfg(test)]
use sqlx::Row;
use sqlx::SqlitePool;

/// The spec tables both the registry and the hierarchy use. One definition,
/// so the table shape does not depend on which module creates it first.
pub(crate) const SPEC_TABLES: &str = r#"
CREATE TABLE IF NOT EXISTS specs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    title TEXT NOT NULL,
    markdown_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    needs_plan INTEGER NOT NULL DEFAULT 0,
    execution_ready INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS spec_revisions (
    spec_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    status TEXT NOT NULL,
    body TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    source TEXT NOT NULL,
    session_id TEXT,
    turn_id TEXT,
    based_on INTEGER,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (spec_id, version)
);
"#;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS plans (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    owner_spec_id TEXT NOT NULL,
    title TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS plan_revisions (
    plan_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    status TEXT NOT NULL,
    body TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    source TEXT NOT NULL,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    agent_id TEXT,
    based_on INTEGER,
    spec_id TEXT NOT NULL,
    spec_version INTEGER NOT NULL,
    spec_content_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (plan_id, version)
);
CREATE TABLE IF NOT EXISTS approval_events (
    id TEXT PRIMARY KEY,
    artifact_kind TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    action TEXT NOT NULL,
    actor_member_id TEXT NOT NULL,
    actor_kind TEXT NOT NULL,
    reason TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS spec_flags (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
INSERT OR IGNORE INTO spec_flags (key, value) VALUES ('hierarchy_v2', '0');
CREATE TABLE IF NOT EXISTS capture_keys (
    artifact_kind TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    source TEXT NOT NULL,
    version INTEGER NOT NULL,
    PRIMARY KEY (artifact_kind, artifact_id, session_id, turn_id, source)
);
CREATE TABLE IF NOT EXISTS spec_session_links (
    session_id TEXT NOT NULL,
    spec_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (session_id, spec_id)
);
CREATE TABLE IF NOT EXISTS workstreams (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    spec_id TEXT NOT NULL,
    name TEXT NOT NULL,
    UNIQUE (project_id, spec_id, name)
);
CREATE TABLE IF NOT EXISTS artifact_policy_links (
    artifact_kind TEXT NOT NULL,
    artifact_id TEXT NOT NULL,
    policy_revision_id INTEGER NOT NULL,
    policy_kind TEXT NOT NULL,
    policy_item_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (artifact_kind, artifact_id, policy_revision_id)
);
CREATE TABLE IF NOT EXISTS plan_selections (
    id TEXT PRIMARY KEY,
    workstream_id TEXT NOT NULL,
    plan_id TEXT NOT NULL,
    plan_version INTEGER NOT NULL,
    session_id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    compatibility TEXT NOT NULL DEFAULT 'current',
    UNIQUE (workstream_id, plan_id, plan_version, session_id, turn_id)
);
CREATE TABLE IF NOT EXISTS import_review (
    id TEXT NOT NULL,
    turn_id TEXT NOT NULL,
    source TEXT NOT NULL,
    project_id TEXT NOT NULL,
    title TEXT NOT NULL,
    body TEXT NOT NULL,
    session_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (id, turn_id, source)
);
"#;

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn hash_text(text: &str) -> String {
    hex::encode(sha2::Sha256::digest(text.as_bytes()))
}

#[derive(Debug, Clone)]
pub struct NewSpecPlan {
    pub project_id: String,
    pub spec_id: String,
    pub spec_title: String,
    pub spec_body: String,
    pub plan_id: String,
    pub plan_title: String,
    pub plan_body: String,
    pub session_id: String,
    pub turn_id: String,
    pub agent_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedSpecPlan {
    pub spec_id: String,
    pub spec_version: i64,
    pub plan_id: String,
    pub plan_version: i64,
}

/// Remembers which pools already ran a schema migration, so the migration
/// (which writes) runs once per pool and later reads stay read-only.
///
/// A pool is identified by its shared connect options. Each entry keeps a weak
/// reference to them, so the address cannot be reused by a new pool while the
/// entry exists; entries of dropped pools are pruned.
pub(crate) struct MigratedPools(
    std::sync::Mutex<Vec<std::sync::Weak<sqlx::sqlite::SqliteConnectOptions>>>,
);

impl MigratedPools {
    pub(crate) const fn new() -> Self {
        Self(std::sync::Mutex::new(Vec::new()))
    }

    pub(crate) fn contains(&self, pool: &SqlitePool) -> bool {
        let options = pool.connect_options();
        let mut seen = self.0.lock().unwrap_or_else(|poison| poison.into_inner());
        seen.retain(|entry| entry.strong_count() > 0);
        seen.iter()
            .any(|entry| std::ptr::eq(entry.as_ptr(), std::sync::Arc::as_ptr(&options)))
    }

    pub(crate) fn insert(&self, pool: &SqlitePool) {
        if self.contains(pool) {
            return;
        }
        let options = pool.connect_options();
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .push(std::sync::Arc::downgrade(&options));
    }
}

static HIERARCHY_MIGRATED: MigratedPools = MigratedPools::new();

/// Columns added after the first schema; each must exist for the schema to be complete.
const ADDED_COLUMNS: &[(&str, &str, &str)] = &[
    ("specs", "needs_plan", "INTEGER NOT NULL DEFAULT 0"),
    ("specs", "execution_ready", "INTEGER NOT NULL DEFAULT 0"),
    (
        "plan_selections",
        "compatibility",
        "TEXT NOT NULL DEFAULT 'current'",
    ),
];

/// True when every table, the flag row and every added column already exist,
/// so nothing needs writing. A read-only connection depends on this: it can
/// read a complete schema but cannot run the `INSERT` or `ALTER` statements.
async fn hierarchy_schema_complete(pool: &SqlitePool) -> bool {
    let tables: Vec<&str> = [SPEC_TABLES, SCHEMA]
        .iter()
        .flat_map(|script| script.split("CREATE TABLE IF NOT EXISTS ").skip(1))
        .filter_map(|rest| rest.split_whitespace().next())
        .collect();
    let present: Vec<String> =
        match sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
            .fetch_all(pool)
            .await
        {
            Ok(names) => names,
            Err(_) => return false,
        };
    if !tables
        .iter()
        .all(|table| present.iter().any(|name| name == table))
    {
        return false;
    }
    let flag: Option<String> =
        sqlx::query_scalar("SELECT key FROM spec_flags WHERE key = 'hierarchy_v2'")
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    if flag.is_none() {
        return false;
    }
    for (table, column, _) in ADDED_COLUMNS {
        let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?1)")
            .bind(*table)
            .fetch_all(pool)
            .await
            .unwrap_or_default();
        if !columns.iter().any(|name| name == column) {
            return false;
        }
    }
    true
}

/// Create or upgrade the hierarchy tables. The work runs once per pool; later
/// calls return at once, so read paths never take the write lock. A schema
/// that is already complete is not written to, so a read-only pool works too.
pub async fn ensure_hierarchy_schema(pool: &SqlitePool) -> Result<(), String> {
    if HIERARCHY_MIGRATED.contains(pool) {
        return Ok(());
    }
    if !hierarchy_schema_complete(pool).await {
        migrate_hierarchy_schema(pool).await?;
    }
    HIERARCHY_MIGRATED.insert(pool);
    Ok(())
}

/// Run each `;`-separated statement of a schema script.
pub(crate) async fn run_schema(pool: &SqlitePool, script: &'static str) -> Result<(), String> {
    for statement in script.split(';') {
        let statement = statement.trim();
        if statement.is_empty() {
            continue;
        }
        sqlx::query(statement)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

async fn migrate_hierarchy_schema(pool: &SqlitePool) -> Result<(), String> {
    run_schema(pool, SPEC_TABLES).await?;
    run_schema(pool, SCHEMA).await?;
    for (table, column, definition) in ADDED_COLUMNS {
        add_column_if_missing(pool, table, column, definition).await?;
    }
    Ok(())
}

async fn add_column_if_missing(
    pool: &SqlitePool,
    table: &'static str,
    column: &'static str,
    definition: &'static str,
) -> Result<(), String> {
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info(?1)")
        .bind(table)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    if columns.iter().any(|name| name == column) {
        return Ok(());
    }
    // Safe: table, column and definition come from the `ADDED_COLUMNS` constant.
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "ALTER TABLE {table} ADD COLUMN {column} {definition}"
    )))
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn create_spec_with_plan(
    pool: &SqlitePool,
    input: NewSpecPlan,
) -> Result<CreatedSpecPlan, String> {
    ensure_hierarchy_schema(pool).await?;
    let plan_title = input.plan_title.trim();
    let plan_body = input.plan_body.trim();
    if plan_title.is_empty() || plan_body.is_empty() {
        return Err("empty plan".into());
    }
    let spec_title = input.spec_title.trim();
    let spec_body = input.spec_body.trim();
    if spec_title.is_empty() || spec_body.is_empty() {
        return Err("empty spec".into());
    }
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    if let Some(existing) = recorded_create(&mut tx, &input).await? {
        tx.commit().await.map_err(|error| error.to_string())?;
        return Ok(existing);
    }
    let created = match write_pair(
        &mut tx, &input, spec_title, spec_body, plan_title, plan_body,
    )
    .await
    {
        Ok(created) => created,
        Err(error) => return Err(error),
    };
    record_capture(&mut tx, &input, &created).await?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(created)
}

async fn write_pair(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    input: &NewSpecPlan,
    spec_title: &str,
    spec_body: &str,
    plan_title: &str,
    plan_body: &str,
) -> Result<CreatedSpecPlan, String> {
    let created_at = now_ms();
    let spec_hash = hash_text(spec_body);
    let plan_hash = hash_text(plan_body);
    let markdown_path = format!("docs/specs/{}/SPEC.md", input.spec_id);
    sqlx::query(
        "INSERT INTO specs (id, project_id, title, markdown_path, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&input.spec_id)
    .bind(&input.project_id)
    .bind(spec_title)
    .bind(&markdown_path)
    .bind(created_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| map_constraint(error, &input.spec_id, &input.plan_id))?;
    sqlx::query(
        "INSERT INTO spec_revisions
         (spec_id, version, status, body, content_hash, source, session_id, turn_id, based_on, created_at)
         VALUES (?1, 1, 'draft', ?2, ?3, 'create', ?4, ?5, NULL, ?6)",
    )
    .bind(&input.spec_id)
    .bind(spec_body)
    .bind(&spec_hash)
    .bind(&input.session_id)
    .bind(&input.turn_id)
    .bind(created_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "INSERT INTO plans (id, project_id, owner_spec_id, title, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&input.plan_id)
    .bind(&input.project_id)
    .bind(&input.spec_id)
    .bind(plan_title)
    .bind(created_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| map_constraint(error, &input.spec_id, &input.plan_id))?;
    sqlx::query(
        "INSERT INTO plan_revisions
         (plan_id, version, status, body, content_hash, source, session_id, turn_id, agent_id,
          based_on, spec_id, spec_version, spec_content_hash, created_at)
         VALUES (?1, 1, 'draft', ?2, ?3, 'create', ?4, ?5, ?6, NULL, ?7, 1, ?8, ?9)",
    )
    .bind(&input.plan_id)
    .bind(plan_body)
    .bind(&plan_hash)
    .bind(&input.session_id)
    .bind(&input.turn_id)
    .bind(&input.agent_id)
    .bind(&input.spec_id)
    .bind(&spec_hash)
    .bind(created_at)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(CreatedSpecPlan {
        spec_id: input.spec_id.clone(),
        spec_version: 1,
        plan_id: input.plan_id.clone(),
        plan_version: 1,
    })
}

#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    pub artifact_kind: String,
    pub artifact_id: String,
    pub version: i64,
    pub actor_member_id: String,
    pub actor_kind: String,
    pub roles: Vec<String>,
    pub reason: String,
}

/// Role check on caller-supplied roles, kept for tests of the approval rules.
/// Production approval goes through `approve_for_member`, which reads the
/// roles from the membership database.
#[cfg(test)]
pub(crate) async fn approve_artifact(
    pool: &SqlitePool,
    request: ApprovalRequest,
) -> Result<(), String> {
    ensure_hierarchy_schema(pool).await?;
    if request.actor_kind != "human" {
        return Err("agent cannot approve".into());
    }
    if !request
        .roles
        .iter()
        .any(|role| role == "owner" || role == "approver" || role == "admin")
    {
        return Err("approver role required".into());
    }
    if request.reason.trim().is_empty() {
        return Err("approval reason required".into());
    }
    write_approval(pool, &request).await
}

pub async fn confirm_plan_compatibility(
    pool: &SqlitePool,
    selection_id: &str,
) -> Result<(), String> {
    ensure_hierarchy_schema(pool).await?;
    let result = sqlx::query(
        "UPDATE plan_selections
         SET compatibility = 'current'
         WHERE id = ?1 AND compatibility = 'compatibility_review_required'",
    )
    .bind(selection_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    if result.rows_affected() != 1 {
        return Err("missing selection".into());
    }
    Ok(())
}

pub async fn confirm_compatibility_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let Some(selection_id) = body.get("selectionId").and_then(|value| value.as_str()) else {
        return serde_json::json!({ "ok": false, "error": "selectionId required" });
    };
    if selection_id.is_empty() {
        return serde_json::json!({ "ok": false, "error": "selectionId required" });
    }
    match confirm_plan_compatibility(pool, selection_id).await {
        Ok(()) => serde_json::json!({
            "ok": true,
            "selectionId": selection_id,
            "compatibility": "current",
        }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

async fn write_approval(pool: &SqlitePool, request: &ApprovalRequest) -> Result<(), String> {
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let row = load_revision(&mut tx, request).await?;
    let Some((body_hash,)) = row else {
        return Err("missing revision".into());
    };
    let previous_approved: Option<String> = if request.artifact_kind == "spec" {
        sqlx::query_scalar(
            "SELECT content_hash FROM spec_revisions
             WHERE spec_id = ?1 AND status = 'approved' AND version != ?2
             ORDER BY version DESC LIMIT 1",
        )
        .bind(&request.artifact_id)
        .bind(request.version)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| error.to_string())?
    } else {
        None
    };
    supersede_previous(&mut tx, request).await?;
    mark_approved(&mut tx, request).await?;
    if request.artifact_kind == "spec" {
        if let Some(previous) = previous_approved {
            if previous != body_hash {
                sqlx::query(
                    "UPDATE plan_selections
                     SET compatibility = 'compatibility_review_required'
                     WHERE id IN (
                         SELECT s.id
                         FROM plan_selections s
                         JOIN plans p ON p.id = s.plan_id
                         JOIN plan_revisions r
                           ON r.plan_id = s.plan_id AND r.version = s.plan_version
                         WHERE p.owner_spec_id = ?1
                           AND r.spec_content_hash != ?2
                     )",
                )
                .bind(&request.artifact_id)
                .bind(&body_hash)
                .execute(&mut *tx)
                .await
                .map_err(|error| error.to_string())?;
            }
        }
    }
    let created_at = now_ms();
    let event_id = format!(
        "{}:{}:{}:approved:{created_at}",
        request.artifact_kind, request.artifact_id, request.version
    );
    sqlx::query(
        "INSERT INTO approval_events
         (id, artifact_kind, artifact_id, version, action, actor_member_id, actor_kind, reason, content_hash, created_at)
         VALUES (?1, ?2, ?3, ?4, 'approved', ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&event_id)
    .bind(&request.artifact_kind)
    .bind(&request.artifact_id)
    .bind(request.version)
    .bind(&request.actor_member_id)
    .bind(&request.actor_kind)
    .bind(request.reason.trim())
    .bind(&body_hash)
    .bind(created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn load_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    request: &ApprovalRequest,
) -> Result<Option<(String,)>, String> {
    let sql = match request.artifact_kind.as_str() {
        "plan" => "SELECT content_hash FROM plan_revisions WHERE plan_id = ?1 AND version = ?2",
        "spec" => "SELECT content_hash FROM spec_revisions WHERE spec_id = ?1 AND version = ?2",
        _ => return Err("unknown artifact".into()),
    };
    sqlx::query_as(sql)
        .bind(&request.artifact_id)
        .bind(request.version)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| error.to_string())
}

async fn supersede_previous(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    request: &ApprovalRequest,
) -> Result<(), String> {
    let sql = match request.artifact_kind.as_str() {
        "plan" => "UPDATE plan_revisions SET status = 'superseded' WHERE plan_id = ?1 AND status = 'approved' AND version != ?2",
        "spec" => "UPDATE spec_revisions SET status = 'superseded' WHERE spec_id = ?1 AND status = 'approved' AND version != ?2",
        _ => return Err("unknown artifact".into()),
    };
    sqlx::query(sql)
        .bind(&request.artifact_id)
        .bind(request.version)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

async fn mark_approved(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    request: &ApprovalRequest,
) -> Result<(), String> {
    let sql = match request.artifact_kind.as_str() {
        "plan" => {
            "UPDATE plan_revisions SET status = 'approved' WHERE plan_id = ?1 AND version = ?2"
        }
        "spec" => {
            "UPDATE spec_revisions SET status = 'approved' WHERE spec_id = ?1 AND version = ?2"
        }
        _ => return Err("unknown artifact".into()),
    };
    let result = sqlx::query(sql)
        .bind(&request.artifact_id)
        .bind(request.version)
        .execute(&mut **tx)
        .await
        .map_err(|error| error.to_string())?;
    if result.rows_affected() != 1 {
        return Err("missing revision".into());
    }
    Ok(())
}

/// The error a revision returns when its `based_on` is not the latest version.
pub(crate) const CONFLICT: &str = "conflict";

/// A unique violation on the revision key means another edit stored the same
/// version first, which is a conflict, not a database failure.
fn revision_insert_error(error: sqlx::Error) -> String {
    match &error {
        sqlx::Error::Database(db) if db.is_unique_violation() => CONFLICT.into(),
        _ => error.to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct PlanEdit {
    pub plan_id: String,
    pub based_on: i64,
    pub body: String,
    pub session_id: String,
    pub turn_id: String,
    pub agent_id: Option<String>,
}

pub async fn revise_plan(pool: &SqlitePool, edit: PlanEdit) -> Result<i64, String> {
    ensure_hierarchy_schema(pool).await?;
    let body = edit.body.trim();
    if body.is_empty() {
        return Err("empty plan".into());
    }
    // IMMEDIATE takes the write lock before the read, so two edits of one
    // version run one after the other and the second sees a conflict.
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|error| error.to_string())?;
    let current: Option<(i64, String, i64, String)> = sqlx::query_as(
        "SELECT version, spec_id, spec_version, spec_content_hash
         FROM plan_revisions WHERE plan_id = ?1
         ORDER BY version DESC LIMIT 1",
    )
    .bind(&edit.plan_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let Some((latest, spec_id, spec_version, spec_hash)) = current else {
        return Err("missing plan".into());
    };
    if edit.based_on != latest {
        return Err(CONFLICT.into());
    }
    let next = latest + 1;
    // An edit keeps the spec revision its parent was written against. Only a
    // compatibility confirmation on a selection accepts it for a newer spec;
    // rebasing here would hide the spec change from review.
    sqlx::query(
        "INSERT INTO plan_revisions
         (plan_id, version, status, body, content_hash, source, session_id, turn_id, agent_id,
          based_on, spec_id, spec_version, spec_content_hash, created_at)
         VALUES (?1, ?2, 'draft', ?3, ?4, 'edit', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )
    .bind(&edit.plan_id)
    .bind(next)
    .bind(body)
    .bind(hash_text(body))
    .bind(&edit.session_id)
    .bind(&edit.turn_id)
    .bind(&edit.agent_id)
    .bind(edit.based_on)
    .bind(&spec_id)
    .bind(spec_version)
    .bind(&spec_hash)
    .bind(now_ms())
    .execute(&mut *tx)
    .await
    .map_err(revision_insert_error)?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(next)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyReport {
    pub queued: Vec<String>,
    pub spec_count: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyFingerprint {
    pub table: String,
    pub rows: i64,
    pub bytes: u64,
    pub sha256: String,
}

#[cfg(test)]
const LEGACY_FINGERPRINTS: &[(&str, &str)] = &[
    (
        "specs",
        "SELECT id, project_id, title, markdown_path, created_at FROM specs ORDER BY id",
    ),
    (
        "spec_revisions",
        "SELECT spec_id, version, status, body, content_hash, source, session_id, turn_id, based_on, created_at FROM spec_revisions ORDER BY spec_id, version",
    ),
    (
        "spec_bindings",
        "SELECT session_id, spec_id FROM spec_bindings ORDER BY session_id",
    ),
    (
        "spec_deliveries",
        "SELECT session_id, turn_id, spec_id, version, content_hash FROM spec_deliveries ORDER BY session_id, turn_id",
    ),
    (
        "spec_file_seals",
        "SELECT spec_id, path, content_hash, sealed_version FROM spec_file_seals ORDER BY spec_id, path",
    ),
];

#[cfg(test)]
fn empty_sha256() -> String {
    hex::encode(sha2::Sha256::digest(b""))
}

#[cfg(test)]
fn fingerprint_cell(row: &sqlx::sqlite::SqliteRow, index: usize) -> String {
    if let Ok(value) = row.try_get::<Option<String>, _>(index) {
        return value.unwrap_or_default();
    }
    if let Ok(value) = row.try_get::<Option<i64>, _>(index) {
        return value.map(|number| number.to_string()).unwrap_or_default();
    }
    String::new()
}

#[cfg(test)]
pub async fn legacy_table_fingerprints(
    pool: &SqlitePool,
) -> Result<Vec<LegacyFingerprint>, String> {
    let mut fingerprints = Vec::with_capacity(LEGACY_FINGERPRINTS.len());
    for (table, sql) in LEGACY_FINGERPRINTS {
        let present: Option<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?1")
                .bind(*table)
                .fetch_optional(pool)
                .await
                .map_err(|error| error.to_string())?;
        if present.is_none() {
            fingerprints.push(LegacyFingerprint {
                table: (*table).to_string(),
                rows: 0,
                bytes: 0,
                sha256: empty_sha256(),
            });
            continue;
        }
        let rows = sqlx::query(*sql)
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?;
        let mut payload = Vec::new();
        for row in &rows {
            let width = row.columns().len();
            for index in 0..width {
                if index > 0 {
                    payload.push(0);
                }
                payload.extend(fingerprint_cell(row, index).into_bytes());
            }
        }
        fingerprints.push(LegacyFingerprint {
            table: (*table).to_string(),
            rows: rows.len() as i64,
            bytes: payload.len() as u64,
            sha256: hex::encode(sha2::Sha256::digest(&payload)),
        });
    }
    Ok(fingerprints)
}

pub async fn dry_run_legacy(pool: &SqlitePool) -> Result<LegacyReport, String> {
    ensure_hierarchy_schema(pool).await?;
    let queued: Vec<String> = sqlx::query_scalar(
        "SELECT s.id FROM specs s
         WHERE EXISTS (
             SELECT 1 FROM spec_revisions r
             WHERE r.spec_id = s.id AND r.source = 'plan_mode'
         )
         AND NOT EXISTS (
             SELECT 1 FROM plans p WHERE p.owner_spec_id = s.id
         )
         ORDER BY s.id",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    let spec_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM specs")
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(LegacyReport { queued, spec_count })
}

pub async fn hierarchy_enabled(pool: &SqlitePool) -> Result<bool, String> {
    ensure_hierarchy_schema(pool).await?;
    let value: Option<String> =
        sqlx::query_scalar("SELECT value FROM spec_flags WHERE key = 'hierarchy_v2'")
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
    Ok(value.as_deref() == Some("1"))
}

pub async fn set_hierarchy_flag(pool: &SqlitePool, enabled: bool) -> Result<(), String> {
    ensure_hierarchy_schema(pool).await?;
    let value = if enabled { "1" } else { "0" };
    sqlx::query(
        "INSERT INTO spec_flags (key, value) VALUES ('hierarchy_v2', ?1)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .bind(value)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

async fn recorded_create(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    input: &NewSpecPlan,
) -> Result<Option<CreatedSpecPlan>, String> {
    let spec_version = capture_version(tx, "spec", &input.spec_id, input).await?;
    let plan_version = capture_version(tx, "plan", &input.plan_id, input).await?;
    match (spec_version, plan_version) {
        (Some(spec_version), Some(plan_version)) => Ok(Some(CreatedSpecPlan {
            spec_id: input.spec_id.clone(),
            spec_version,
            plan_id: input.plan_id.clone(),
            plan_version,
        })),
        _ => Ok(None),
    }
}

async fn capture_version(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    kind: &str,
    artifact_id: &str,
    input: &NewSpecPlan,
) -> Result<Option<i64>, String> {
    sqlx::query_scalar(
        "SELECT version FROM capture_keys
         WHERE artifact_kind = ?1 AND artifact_id = ?2
           AND session_id = ?3 AND turn_id = ?4 AND source = 'create'",
    )
    .bind(kind)
    .bind(artifact_id)
    .bind(&input.session_id)
    .bind(&input.turn_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(|error| error.to_string())
}

async fn record_capture(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    input: &NewSpecPlan,
    created: &CreatedSpecPlan,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO capture_keys
         (artifact_kind, artifact_id, session_id, turn_id, source, version)
         VALUES ('spec', ?1, ?2, ?3, 'create', ?4),
                ('plan', ?5, ?2, ?3, 'create', ?6)",
    )
    .bind(&input.spec_id)
    .bind(&input.session_id)
    .bind(&input.turn_id)
    .bind(created.spec_version)
    .bind(&input.plan_id)
    .bind(created.plan_version)
    .execute(&mut **tx)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct NewAlternativePlan {
    pub project_id: String,
    pub spec_id: String,
    pub plan_id: String,
    pub plan_title: String,
    pub plan_body: String,
    pub session_id: String,
    pub turn_id: String,
    pub agent_id: Option<String>,
}

pub async fn create_alternative_plan(
    pool: &SqlitePool,
    input: NewAlternativePlan,
) -> Result<i64, String> {
    ensure_hierarchy_schema(pool).await?;
    let title = input.plan_title.trim();
    let body = input.plan_body.trim();
    if title.is_empty() || body.is_empty() {
        return Err("empty plan".into());
    }
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    let spec: Option<(i64, String, String)> = sqlx::query_as(
        "SELECT r.version, r.content_hash, s.project_id
         FROM spec_revisions r
         JOIN specs s ON s.id = r.spec_id
         WHERE r.spec_id = ?1
         ORDER BY r.version DESC
         LIMIT 1",
    )
    .bind(&input.spec_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let Some((spec_version, spec_hash, project_id)) = spec else {
        return Err("missing spec".into());
    };
    if project_id != input.project_id {
        return Err("project mismatch".into());
    }
    let created_at = now_ms();
    let plan_hash = hash_text(body);
    sqlx::query(
        "INSERT INTO plans (id, project_id, owner_spec_id, title, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&input.plan_id)
    .bind(&input.project_id)
    .bind(&input.spec_id)
    .bind(title)
    .bind(created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| map_constraint(error, &input.spec_id, &input.plan_id))?;
    sqlx::query(
        "INSERT INTO plan_revisions
         (plan_id, version, status, body, content_hash, source, session_id, turn_id, agent_id,
          based_on, spec_id, spec_version, spec_content_hash, created_at)
         VALUES (?1, 1, 'draft', ?2, ?3, 'alternative', ?4, ?5, ?6, NULL, ?7, ?8, ?9, ?10)",
    )
    .bind(&input.plan_id)
    .bind(body)
    .bind(&plan_hash)
    .bind(&input.session_id)
    .bind(&input.turn_id)
    .bind(&input.agent_id)
    .bind(&input.spec_id)
    .bind(spec_version)
    .bind(&spec_hash)
    .bind(created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query(
        "INSERT INTO capture_keys
         (artifact_kind, artifact_id, session_id, turn_id, source, version)
         VALUES ('plan', ?1, ?2, ?3, 'alternative', 1)",
    )
    .bind(&input.plan_id)
    .bind(&input.session_id)
    .bind(&input.turn_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("UPDATE specs SET needs_plan = 0 WHERE id = ?1")
        .bind(&input.spec_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(1)
}

pub async fn link_session(
    pool: &SqlitePool,
    session_id: &str,
    spec_id: &str,
) -> Result<(), String> {
    ensure_hierarchy_schema(pool).await?;
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM specs WHERE id = ?1")
        .bind(spec_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
    if exists.is_none() {
        return Err("missing spec".into());
    }
    sqlx::query(
        "INSERT OR IGNORE INTO spec_session_links (session_id, spec_id, created_at)
         VALUES (?1, ?2, ?3)",
    )
    .bind(session_id)
    .bind(spec_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct NewWorkstream {
    pub id: String,
    pub project_id: String,
    pub spec_id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct PlanSelection {
    pub workstream_id: String,
    pub plan_id: String,
    pub plan_version: i64,
    pub session_id: String,
    pub turn_id: String,
}

pub async fn create_workstream(pool: &SqlitePool, input: NewWorkstream) -> Result<(), String> {
    ensure_hierarchy_schema(pool).await?;
    let name = input.name.trim();
    if name.is_empty() {
        return Err("empty workstream".into());
    }
    let project: Option<String> = sqlx::query_scalar("SELECT project_id FROM specs WHERE id = ?1")
        .bind(&input.spec_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
    let Some(project) = project else {
        return Err("missing spec".into());
    };
    if project != input.project_id {
        return Err("project mismatch".into());
    }
    sqlx::query("INSERT INTO workstreams (id, project_id, spec_id, name) VALUES (?1, ?2, ?3, ?4)")
        .bind(&input.id)
        .bind(&input.project_id)
        .bind(&input.spec_id)
        .bind(name)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn select_plan(pool: &SqlitePool, input: PlanSelection) -> Result<(), String> {
    ensure_hierarchy_schema(pool).await?;
    let owner: Option<(String, String)> = sqlx::query_as(
        "SELECT p.owner_spec_id, r.spec_content_hash
         FROM plans p
         JOIN plan_revisions r ON r.plan_id = p.id AND r.version = ?2
         WHERE p.id = ?1",
    )
    .bind(&input.plan_id)
    .bind(input.plan_version)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let Some((owner, written_against)) = owner else {
        return Err("missing plan".into());
    };
    let spec_id: Option<String> =
        sqlx::query_scalar("SELECT spec_id FROM workstreams WHERE id = ?1")
            .bind(&input.workstream_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
    let Some(spec_id) = spec_id else {
        return Err("missing workstream".into());
    };
    if owner != spec_id {
        return Err("plan belongs to another spec".into());
    }
    // A plan revision written against another spec body than the approved one
    // needs a compatibility review, whenever it is selected.
    let approved: Option<String> = sqlx::query_scalar(
        "SELECT content_hash FROM spec_revisions
         WHERE spec_id = ?1 AND status = 'approved'
         ORDER BY version DESC LIMIT 1",
    )
    .bind(&spec_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let compatibility = match approved {
        Some(hash) if hash != written_against => "compatibility_review_required",
        _ => "current",
    };
    let id = format!(
        "{}:{}:{}:{}:{}",
        input.workstream_id, input.plan_id, input.plan_version, input.session_id, input.turn_id
    );
    sqlx::query(
        "INSERT OR IGNORE INTO plan_selections
         (id, workstream_id, plan_id, plan_version, session_id, turn_id, created_at, compatibility)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&id)
    .bind(&input.workstream_id)
    .bind(&input.plan_id)
    .bind(input.plan_version)
    .bind(&input.session_id)
    .bind(&input.turn_id)
    .bind(now_ms())
    .bind(compatibility)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn selected_plans(
    pool: &SqlitePool,
    workstream_id: &str,
) -> Result<Vec<(String, i64)>, String> {
    ensure_hierarchy_schema(pool).await?;
    sqlx::query_as(
        "SELECT plan_id, plan_version
         FROM plan_selections
         WHERE workstream_id = ?1
         ORDER BY plan_id, plan_version",
    )
    .bind(workstream_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

pub async fn specs_for_session(pool: &SqlitePool, session_id: &str) -> Result<Vec<String>, String> {
    ensure_hierarchy_schema(pool).await?;
    sqlx::query_scalar(
        "SELECT spec_id FROM spec_session_links WHERE session_id = ?1 ORDER BY spec_id",
    )
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())
}

#[derive(Debug, Clone)]
pub struct NewSpecWithoutPlan {
    pub project_id: String,
    pub spec_id: String,
    pub spec_title: String,
    pub spec_body: String,
    pub session_id: String,
    pub turn_id: String,
}

pub async fn import_spec_without_plan(
    pool: &SqlitePool,
    input: NewSpecWithoutPlan,
) -> Result<(), String> {
    ensure_hierarchy_schema(pool).await?;
    let title = input.spec_title.trim();
    let body = input.spec_body.trim();
    if title.is_empty() || body.is_empty() {
        return Err("empty spec".into());
    }
    let created_at = now_ms();
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;
    sqlx::query(
        "INSERT INTO specs
         (id, project_id, title, markdown_path, created_at, needs_plan, execution_ready)
         VALUES (?1, ?2, ?3, ?4, ?5, 1, 0)",
    )
    .bind(&input.spec_id)
    .bind(&input.project_id)
    .bind(title)
    .bind(format!("docs/specs/{}/SPEC.md", input.spec_id))
    .bind(created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| map_constraint(error, &input.spec_id, ""))?;
    sqlx::query(
        "INSERT INTO spec_revisions
         (spec_id, version, status, body, content_hash, source, session_id, turn_id, based_on, created_at)
         VALUES (?1, 1, 'draft', ?2, ?3, 'import', ?4, ?5, NULL, ?6)",
    )
    .bind(&input.spec_id)
    .bind(body)
    .bind(hash_text(body))
    .bind(&input.session_id)
    .bind(&input.turn_id)
    .bind(created_at)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn mark_execution_ready(pool: &SqlitePool, spec_id: &str) -> Result<(), String> {
    ensure_hierarchy_schema(pool).await?;
    let row: Option<(i64, i64)> = sqlx::query_as(
        "SELECT needs_plan,
                (SELECT COUNT(*) FROM plans WHERE owner_spec_id = specs.id)
         FROM specs
         WHERE id = ?1",
    )
    .bind(spec_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?;
    let Some((needs_plan, plan_count)) = row else {
        return Err("missing spec".into());
    };
    if needs_plan != 0 || plan_count == 0 {
        return Err("plan required".into());
    }
    sqlx::query("UPDATE specs SET execution_ready = 1 WHERE id = ?1")
        .bind(spec_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn approve_for_member(
    project: &SqlitePool,
    membership: &SqlitePool,
    project_id: &str,
    request: ApprovalRequest,
) -> Result<(), String> {
    ensure_hierarchy_schema(project).await?;
    if request.actor_kind != "human" {
        return Err("agent cannot approve".into());
    }
    if request.reason.trim().is_empty() {
        return Err("approval reason required".into());
    }
    let roles: Vec<String> = sqlx::query_scalar(
        "SELECT r.role
         FROM member_roles r
         JOIN team_members m ON m.id = r.member_id
         WHERE r.member_id = ?1 AND m.status = 'active'",
    )
    .bind(&request.actor_member_id)
    .fetch_all(membership)
    .await
    .map_err(|error| error.to_string())?;
    if !roles
        .iter()
        .any(|role| role == "owner" || role == "approver" || role == "admin")
    {
        return Err("approver role required".into());
    }
    let artifact_project = artifact_project(project, &request).await?;
    if artifact_project != project_id {
        return Err("project mismatch".into());
    }
    write_approval(project, &request).await
}

async fn artifact_project(pool: &SqlitePool, request: &ApprovalRequest) -> Result<String, String> {
    let sql = match request.artifact_kind.as_str() {
        "plan" => "SELECT project_id FROM plans WHERE id = ?1",
        "spec" => "SELECT project_id FROM specs WHERE id = ?1",
        _ => return Err("unknown artifact".into()),
    };
    let project: Option<String> = sqlx::query_scalar(sql)
        .bind(&request.artifact_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
    project.ok_or_else(|| "missing revision".into())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HierarchyCreateBody {
    project_id: String,
    spec_id: String,
    spec_title: String,
    spec_body: String,
    plan_id: String,
    plan_title: String,
    plan_body: String,
    session_id: String,
    turn_id: String,
    #[serde(default)]
    agent_id: Option<String>,
}

pub async fn create_hierarchy_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: HierarchyCreateBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    match create_spec_with_plan(
        pool,
        NewSpecPlan {
            project_id: parsed.project_id,
            spec_id: parsed.spec_id,
            spec_title: parsed.spec_title,
            spec_body: parsed.spec_body,
            plan_id: parsed.plan_id,
            plan_title: parsed.plan_title,
            plan_body: parsed.plan_body,
            session_id: parsed.session_id,
            turn_id: parsed.turn_id,
            agent_id: parsed.agent_id,
        },
    )
    .await
    {
        Ok(created) => serde_json::json!({
            "ok": true,
            "specId": created.spec_id,
            "specVersion": created.spec_version,
            "planId": created.plan_id,
            "planVersion": created.plan_version,
        }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlternativeBody {
    project_id: String,
    spec_id: String,
    plan_id: String,
    plan_title: String,
    plan_body: String,
    session_id: String,
    turn_id: String,
    #[serde(default)]
    agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReviseBody {
    plan_id: String,
    based_on: i64,
    body: String,
    session_id: String,
    turn_id: String,
    #[serde(default)]
    agent_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LinkBody {
    session_id: String,
    spec_id: String,
}

pub async fn alternative_plan_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: AlternativeBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    match create_alternative_plan(
        pool,
        NewAlternativePlan {
            project_id: parsed.project_id,
            spec_id: parsed.spec_id,
            plan_id: parsed.plan_id.clone(),
            plan_title: parsed.plan_title,
            plan_body: parsed.plan_body,
            session_id: parsed.session_id,
            turn_id: parsed.turn_id,
            agent_id: parsed.agent_id,
        },
    )
    .await
    {
        Ok(version) => serde_json::json!({
            "ok": true,
            "planId": parsed.plan_id,
            "planVersion": version,
        }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

#[derive(Debug, Clone)]
pub struct SpecEdit {
    pub spec_id: String,
    pub based_on: i64,
    pub body: String,
    pub session_id: String,
    pub turn_id: String,
}

pub async fn revise_spec(pool: &SqlitePool, edit: SpecEdit) -> Result<i64, String> {
    ensure_hierarchy_schema(pool).await?;
    let body = edit.body.trim();
    if body.is_empty() {
        return Err("empty spec".into());
    }
    // IMMEDIATE takes the write lock before the read; see revise_plan.
    let mut tx = pool
        .begin_with("BEGIN IMMEDIATE")
        .await
        .map_err(|error| error.to_string())?;
    // `MAX(version)` always returns a row and decodes NULL as 0, so read the
    // head row itself, and only for a spec that exists.
    let latest: Option<i64> = sqlx::query_scalar(
        "SELECT r.version FROM spec_revisions r JOIN specs s ON s.id = r.spec_id
         WHERE r.spec_id = ?1 ORDER BY r.version DESC LIMIT 1",
    )
    .bind(&edit.spec_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;
    let Some(latest) = latest else {
        return Err("missing spec".into());
    };
    if edit.based_on != latest {
        return Err(CONFLICT.into());
    }
    let next = latest + 1;
    sqlx::query(
        "INSERT INTO spec_revisions
         (spec_id, version, status, body, content_hash, source, session_id, turn_id, based_on, created_at)
         VALUES (?1, ?2, 'draft', ?3, ?4, 'edit', ?5, ?6, ?7, ?8)",
    )
    .bind(&edit.spec_id)
    .bind(next)
    .bind(body)
    .bind(hash_text(body))
    .bind(&edit.session_id)
    .bind(&edit.turn_id)
    .bind(edit.based_on)
    .bind(now_ms())
    .execute(&mut *tx)
    .await
    .map_err(revision_insert_error)?;
    tx.commit().await.map_err(|error| error.to_string())?;
    Ok(next)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpecReviseBody {
    spec_id: String,
    based_on: i64,
    body: String,
    session_id: String,
    turn_id: String,
}

pub async fn revise_spec_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: SpecReviseBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    match revise_spec(
        pool,
        SpecEdit {
            spec_id: parsed.spec_id.clone(),
            based_on: parsed.based_on,
            body: parsed.body.clone(),
            session_id: parsed.session_id.clone(),
            turn_id: parsed.turn_id.clone(),
        },
    )
    .await
    {
        Ok(version) => serde_json::json!({
            "ok": true,
            "specId": parsed.spec_id,
            "specVersion": version,
        }),
        Err(error) => {
            conflict_json(
                pool,
                Revisions::Spec,
                &parsed.spec_id,
                parsed.based_on,
                &parsed.body,
                error,
            )
            .await
        }
    }
}

pub async fn revise_plan_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: ReviseBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    match revise_plan(
        pool,
        PlanEdit {
            plan_id: parsed.plan_id.clone(),
            based_on: parsed.based_on,
            body: parsed.body.clone(),
            session_id: parsed.session_id.clone(),
            turn_id: parsed.turn_id.clone(),
            agent_id: parsed.agent_id.clone(),
        },
    )
    .await
    {
        Ok(version) => serde_json::json!({
            "ok": true,
            "planId": parsed.plan_id,
            "planVersion": version,
        }),
        Err(error) => {
            conflict_json(
                pool,
                Revisions::Plan,
                &parsed.plan_id,
                parsed.based_on,
                &parsed.body,
                error,
            )
            .await
        }
    }
}

/// Which revision table a conflict or a side-by-side view reads.
#[derive(Clone, Copy)]
enum Revisions {
    Plan,
    Spec,
}

impl Revisions {
    fn head_sql(self) -> &'static str {
        match self {
            Self::Plan => {
                "SELECT version FROM plan_revisions WHERE plan_id = ?1 ORDER BY version DESC LIMIT 1"
            }
            Self::Spec => {
                "SELECT version FROM spec_revisions WHERE spec_id = ?1 ORDER BY version DESC LIMIT 1"
            }
        }
    }

    fn side_sql(self) -> &'static str {
        match self {
            Self::Plan => {
                "SELECT body, content_hash FROM plan_revisions WHERE plan_id = ?1 AND version = ?2"
            }
            Self::Spec => {
                "SELECT body, content_hash FROM spec_revisions WHERE spec_id = ?1 AND version = ?2"
            }
        }
    }
}

/// Body and hash of one revision.
async fn revision_side(
    pool: &SqlitePool,
    kind: Revisions,
    id: &str,
    version: i64,
) -> Result<Option<(String, String)>, String> {
    ensure_hierarchy_schema(pool).await?;
    sqlx::query_as(kind.side_sql())
        .bind(id)
        .bind(version)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())
}

/// The error payload of a failed edit. A conflict also carries the incoming
/// body, the base it was written against and the current head.
async fn conflict_json(
    pool: &SqlitePool,
    kind: Revisions,
    id: &str,
    based_on: i64,
    incoming: &str,
    error: String,
) -> serde_json::Value {
    if error != CONFLICT {
        return serde_json::json!({ "ok": false, "error": error });
    }
    let head_version: Option<i64> = sqlx::query_scalar(kind.head_sql())
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
    let mut value = serde_json::json!({
        "ok": false,
        "error": error,
        "incoming": { "body": incoming.trim() },
    });
    if let Ok(Some((body, hash))) = revision_side(pool, kind, id, based_on).await {
        value["base"] = serde_json::json!({
            "version": based_on,
            "body": body,
            "hash": hash,
        });
    }
    if let Some(version) = head_version {
        if let Ok(Some((body, hash))) = revision_side(pool, kind, id, version).await {
            value["head"] = serde_json::json!({
                "version": version,
                "body": body,
                "hash": hash,
            });
        }
    }
    value
}

pub async fn link_session_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: LinkBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    if let Err(error) = link_session(pool, &parsed.session_id, &parsed.spec_id).await {
        return serde_json::json!({ "ok": false, "error": error });
    }
    match specs_for_session(pool, &parsed.session_id).await {
        Ok(spec_ids) => serde_json::json!({
            "ok": true,
            "sessionId": parsed.session_id,
            "specIds": spec_ids,
        }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PolicyLinkBody {
    artifact_kind: String,
    artifact_id: String,
    policy_revision_id: i64,
    #[serde(default)]
    content_hash: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

pub async fn link_policy_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: PolicyLinkBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    if parsed.body.is_some() {
        return serde_json::json!({
            "ok": false,
            "error": "policy body is not stored on a spec",
        });
    }
    match store_policy_link(pool, &parsed).await {
        Ok(stored) => serde_json::json!({
            "ok": true,
            "artifactKind": parsed.artifact_kind,
            "artifactId": parsed.artifact_id,
            "policyRevisionId": parsed.policy_revision_id,
            "policyKind": stored.policy_kind,
            "policyItemId": stored.policy_item_id,
            "contentHash": stored.content_hash,
        }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

struct StoredPolicyLink {
    policy_kind: String,
    policy_item_id: String,
    content_hash: String,
}

async fn store_policy_link(
    pool: &SqlitePool,
    parsed: &PolicyLinkBody,
) -> Result<StoredPolicyLink, String> {
    ensure_hierarchy_schema(pool).await?;
    let exists_sql = match parsed.artifact_kind.as_str() {
        "spec" => "SELECT id FROM specs WHERE id = ?1",
        "plan" => "SELECT id FROM plans WHERE id = ?1",
        _ => return Err("unknown artifact".into()),
    };
    let exists: Option<String> = sqlx::query_scalar(exists_sql)
        .bind(&parsed.artifact_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?;
    if exists.is_none() {
        return Err("missing artifact".into());
    }
    let revision: Option<(String, String, String)> =
        sqlx::query_as("SELECT kind, item_id, content_hash FROM policy_revisions WHERE id = ?1")
            .bind(parsed.policy_revision_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| error.to_string())?;
    let Some((policy_kind, policy_item_id, content_hash)) = revision else {
        return Err("missing policy revision".into());
    };
    if let Some(claimed) = &parsed.content_hash {
        if claimed != &content_hash {
            return Err("hash mismatch".into());
        }
    }
    sqlx::query(
        "INSERT INTO artifact_policy_links
         (artifact_kind, artifact_id, policy_revision_id, policy_kind, policy_item_id, content_hash, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(artifact_kind, artifact_id, policy_revision_id) DO NOTHING",
    )
    .bind(&parsed.artifact_kind)
    .bind(&parsed.artifact_id)
    .bind(parsed.policy_revision_id)
    .bind(&policy_kind)
    .bind(&policy_item_id)
    .bind(&content_hash)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(StoredPolicyLink {
        policy_kind,
        policy_item_id,
        content_hash,
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectBody {
    workstream_id: String,
    plan_id: String,
    plan_version: i64,
    session_id: String,
    turn_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompareBody {
    plan_id: String,
    left: i64,
    right: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ListBody {
    project_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProvenanceBody {
    plan_id: String,
    version: i64,
}

pub async fn select_plan_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: SelectBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    if let Err(error) = select_plan(
        pool,
        PlanSelection {
            workstream_id: parsed.workstream_id.clone(),
            plan_id: parsed.plan_id,
            plan_version: parsed.plan_version,
            session_id: parsed.session_id,
            turn_id: parsed.turn_id,
        },
    )
    .await
    {
        return serde_json::json!({ "ok": false, "error": error });
    }
    match selected_plans(pool, &parsed.workstream_id).await {
        Ok(rows) => serde_json::json!({
            "ok": true,
            "workstreamId": parsed.workstream_id,
            "selections": rows.into_iter().map(|(plan_id, plan_version)| serde_json::json!({
                "planId": plan_id,
                "planVersion": plan_version,
            })).collect::<Vec<_>>(),
        }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

pub async fn compare_plan_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: CompareBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    let left = match revision_side(pool, Revisions::Plan, &parsed.plan_id, parsed.left).await {
        Ok(Some(side)) => side,
        Ok(None) => return serde_json::json!({ "ok": false, "error": "missing revision" }),
        Err(error) => return serde_json::json!({ "ok": false, "error": error }),
    };
    let right = match revision_side(pool, Revisions::Plan, &parsed.plan_id, parsed.right).await {
        Ok(Some(side)) => side,
        Ok(None) => return serde_json::json!({ "ok": false, "error": "missing revision" }),
        Err(error) => return serde_json::json!({ "ok": false, "error": error }),
    };
    serde_json::json!({
        "ok": true,
        "planId": parsed.plan_id,
        "left": { "version": parsed.left, "body": left.0, "hash": left.1 },
        "right": { "version": parsed.right, "body": right.0, "hash": right.1 },
    })
}

pub async fn list_plan_revisions(
    pool: &SqlitePool,
    plan_id: &str,
) -> Result<Vec<crate::spec_registry::SpecHistoryRow>, String> {
    ensure_hierarchy_schema(pool).await?;
    let rows: Vec<crate::spec_registry::RevisionColumns> = sqlx::query_as(
        "SELECT version, status, source, body, content_hash, based_on, created_at
         FROM plan_revisions WHERE plan_id = ?1 ORDER BY version",
    )
    .bind(plan_id)
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(rows
        .into_iter()
        .map(
            |(version, status, source, body, content_hash, based_on, created_at)| {
                crate::spec_registry::SpecHistoryRow {
                    version,
                    status,
                    source,
                    body,
                    content_hash,
                    based_on,
                    created_at,
                }
            },
        )
        .collect())
}

pub async fn list_hierarchy_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: ListBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    if let Err(error) = ensure_hierarchy_schema(pool).await {
        return serde_json::json!({ "ok": false, "error": error });
    }
    let rows: Vec<(String, Option<String>, Option<i64>)> = match sqlx::query_as(
        "SELECT s.id, p.id, MAX(r.version)
         FROM specs s
         LEFT JOIN plans p ON p.owner_spec_id = s.id
         LEFT JOIN plan_revisions r ON r.plan_id = p.id
         WHERE s.project_id = ?1
         GROUP BY s.id, p.id
         ORDER BY s.id, p.id",
    )
    .bind(&parsed.project_id)
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    let mut specs: Vec<serde_json::Value> = Vec::new();
    for (spec_id, plan_id, version) in rows {
        let same = specs
            .last()
            .and_then(|spec| spec.get("specId"))
            .and_then(|value| value.as_str())
            == Some(spec_id.as_str());
        if !same {
            specs.push(serde_json::json!({ "specId": spec_id, "plans": [] }));
        }
        if let (Some(plan_id), Some(version)) = (plan_id, version) {
            if let Some(plans) = specs
                .last_mut()
                .and_then(|spec| spec.get_mut("plans"))
                .and_then(|plans| plans.as_array_mut())
            {
                plans.push(serde_json::json!({
                    "planId": plan_id,
                    "latestVersion": version,
                }));
            }
        }
    }
    serde_json::json!({ "ok": true, "projectId": parsed.project_id, "specs": specs })
}

type ProvenanceColumns = (String, String, Option<String>, String, i64);

pub async fn provenance_from_json(pool: &SqlitePool, body: serde_json::Value) -> serde_json::Value {
    let parsed: ProvenanceBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    if let Err(error) = ensure_hierarchy_schema(pool).await {
        return serde_json::json!({ "ok": false, "error": error });
    }
    let row: Result<Option<ProvenanceColumns>, String> = sqlx::query_as(
        "SELECT session_id, turn_id, agent_id, spec_id, spec_version
         FROM plan_revisions WHERE plan_id = ?1 AND version = ?2",
    )
    .bind(&parsed.plan_id)
    .bind(parsed.version)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string());
    match row {
        Ok(Some((session_id, turn_id, agent_id, spec_id, spec_version))) => serde_json::json!({
            "ok": true,
            "planId": parsed.plan_id,
            "version": parsed.version,
            "sessionId": session_id,
            "turnId": turn_id,
            "agentId": agent_id,
            "specId": spec_id,
            "specVersion": spec_version,
        }),
        Ok(None) => serde_json::json!({ "ok": false, "error": "missing revision" }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApproveBody {
    project_id: String,
    artifact_kind: String,
    artifact_id: String,
    version: i64,
    actor_member_id: String,
    actor_kind: String,
    reason: String,
}

pub fn bind_approval_actor(
    mut body: serde_json::Value,
    member_id: &str,
) -> Result<serde_json::Value, String> {
    let Some(object) = body.as_object_mut() else {
        return Err("approval body must be an object".into());
    };
    if let Some(claimed) = object.get("actorMemberId").and_then(|value| value.as_str()) {
        if claimed != member_id {
            return Err("member mismatch".into());
        }
    }
    object.insert(
        "actorMemberId".into(),
        serde_json::Value::String(member_id.to_string()),
    );
    object.insert(
        "actorKind".into(),
        serde_json::Value::String("human".into()),
    );
    Ok(body)
}

pub async fn approve_from_json(
    project: &SqlitePool,
    membership: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    let parsed: ApproveBody = match serde_json::from_value(body) {
        Ok(parsed) => parsed,
        Err(error) => return serde_json::json!({ "ok": false, "error": error.to_string() }),
    };
    match approve_for_member(
        project,
        membership,
        &parsed.project_id,
        ApprovalRequest {
            artifact_kind: parsed.artifact_kind,
            artifact_id: parsed.artifact_id,
            version: parsed.version,
            actor_member_id: parsed.actor_member_id,
            actor_kind: parsed.actor_kind,
            roles: Vec::new(),
            reason: parsed.reason,
        },
    )
    .await
    {
        Ok(()) => serde_json::json!({ "ok": true }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

pub async fn gated_preflight_block(
    pool: &SqlitePool,
    root: &std::path::Path,
    session_id: &str,
) -> String {
    if hierarchy_enabled(pool).await.unwrap_or(false) {
        return hierarchy_preflight_block(pool, session_id).await;
    }
    crate::spec_registry::spec_context_block(pool, root, session_id).await
}

async fn hierarchy_preflight_block(pool: &SqlitePool, session_id: &str) -> String {
    let specs = specs_for_session(pool, session_id)
        .await
        .unwrap_or_default();
    if specs.is_empty() {
        return "hierarchy: on\nnone".into();
    }
    let mut lines = vec!["hierarchy: on".into()];
    for spec_id in specs {
        lines.push(format!("spec: {spec_id}"));
        let plans: Vec<(String, Option<i64>)> = sqlx::query_as(
            "SELECT p.id, MAX(r.version)
             FROM plans p
             LEFT JOIN plan_revisions r ON r.plan_id = p.id
             WHERE p.owner_spec_id = ?1
             GROUP BY p.id
             ORDER BY p.id",
        )
        .bind(&spec_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        for (plan_id, version) in plans {
            lines.push(format!("plan: {plan_id}@{}", version.unwrap_or(0)));
        }
    }
    lines.join("\n")
}

pub async fn gated_list(pool: &SqlitePool, project_id: &str) -> serde_json::Value {
    if !hierarchy_enabled(pool).await.unwrap_or(false) {
        return serde_json::json!({ "mode": "legacy" });
    }
    let mut listed =
        list_hierarchy_from_json(pool, serde_json::json!({ "projectId": project_id })).await;
    if let Some(object) = listed.as_object_mut() {
        object.insert("mode".into(), serde_json::json!("hierarchy"));
    }
    listed
}

pub async fn capture_plan_draft(pool: &SqlitePool, body: serde_json::Value) -> serde_json::Value {
    if !hierarchy_enabled(pool).await.unwrap_or(false) {
        return serde_json::json!({ "ok": false, "error": "hierarchy off" });
    }
    revise_plan_from_json(pool, body).await
}

/// Queue a finished plan for import review. A body equal to the latest queued
/// body for the same id is not stored again, so an unchanged file does not
/// add a row on every turn.
#[allow(clippy::too_many_arguments)]
pub async fn queue_finished_plan(
    pool: &SqlitePool,
    project_id: &str,
    spec_id: &str,
    title: &str,
    body: &str,
    session_id: &str,
    turn_id: &str,
    source: &str,
) -> Result<(), String> {
    ensure_hierarchy_schema(pool).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO import_review
         (id, turn_id, source, project_id, title, body, session_id, created_at)
         SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8
         WHERE NOT EXISTS (
             SELECT 1 FROM (
                 SELECT body FROM import_review WHERE id = ?1 ORDER BY rowid DESC LIMIT 1
             ) WHERE body = ?6
         )",
    )
    .bind(spec_id)
    .bind(turn_id)
    .bind(source)
    .bind(project_id)
    .bind(title)
    .bind(body)
    .bind(session_id)
    .bind(now_ms())
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    Ok(())
}

pub async fn import_review_from_json(pool: &SqlitePool) -> serde_json::Value {
    match import_review_ids(pool).await {
        Ok(ids) => serde_json::json!({ "ok": true, "ids": ids }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

pub async fn import_review_ids(pool: &SqlitePool) -> Result<Vec<String>, String> {
    ensure_hierarchy_schema(pool).await?;
    sqlx::query_scalar("SELECT DISTINCT id FROM import_review ORDER BY id")
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())
}

pub async fn hierarchy_flag_from_json(
    pool: &SqlitePool,
    body: serde_json::Value,
) -> serde_json::Value {
    if let Some(value) = body.get("enabled") {
        let Some(enabled) = value.as_bool() else {
            return serde_json::json!({ "ok": false, "error": "enabled must be true or false" });
        };
        if let Err(error) = set_hierarchy_flag(pool, enabled).await {
            return serde_json::json!({ "ok": false, "error": error });
        }
    }
    match hierarchy_enabled(pool).await {
        Ok(enabled) => serde_json::json!({ "ok": true, "enabled": enabled, "key": "hierarchy_v2" }),
        Err(error) => serde_json::json!({ "ok": false, "error": error }),
    }
}

fn map_constraint(error: sqlx::Error, spec_id: &str, plan_id: &str) -> String {
    let text = error.to_string();
    if text.contains("plans.id") {
        format!("plan {plan_id} already exists")
    } else if text.contains("specs.id") {
        format!("spec {spec_id} already exists")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    async fn count(pool: &SqlitePool, table: &'static str) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn sample(plan_body: &str) -> NewSpecPlan {
        NewSpecPlan {
            project_id: "ax".into(),
            spec_id: "spec-billing".into(),
            spec_title: "Billing".into(),
            spec_body: "Charge the recorded amount.".into(),
            plan_id: "plan-ledger".into(),
            plan_title: "Ledger plan".into(),
            plan_body: plan_body.into(),
            session_id: "session-1".into(),
            turn_id: "turn-1".into(),
            agent_id: Some("agent-ax".into()),
        }
    }

    #[tokio::test]
    async fn an_empty_plan_is_rejected_and_writes_no_rows() {
        let (_dir, pool) = open().await;
        ensure_hierarchy_schema(&pool).await.unwrap();
        let error = create_spec_with_plan(&pool, sample("  "))
            .await
            .unwrap_err();
        assert!(error.contains("plan"), "{error}");
        assert_eq!(count(&pool, "specs").await, 0);
        assert_eq!(count(&pool, "plans").await, 0);
    }

    #[tokio::test]
    async fn a_spec_and_its_first_plan_are_different_rows() {
        let (_dir, pool) = open().await;
        let created = create_spec_with_plan(&pool, sample("Write the ledger first."))
            .await
            .unwrap();
        assert_eq!(created.spec_id, "spec-billing");
        assert_eq!(created.plan_id, "plan-ledger");
        assert_ne!(created.spec_id, created.plan_id);
        assert_eq!(created.spec_version, 1);
        assert_eq!(created.plan_version, 1);
        assert_eq!(count(&pool, "specs").await, 1);
        assert_eq!(count(&pool, "plans").await, 1);
        let owner: String =
            sqlx::query_scalar("SELECT owner_spec_id FROM plans WHERE id = 'plan-ledger'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(owner, "spec-billing");
    }

    #[tokio::test]
    async fn a_duplicate_plan_id_rolls_back_the_second_spec() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Write the ledger first."))
            .await
            .unwrap();
        let mut second = sample("Take the other path.");
        second.spec_id = "spec-other".into();
        second.spec_title = "Other".into();
        let error = create_spec_with_plan(&pool, second).await.unwrap_err();
        assert!(error.contains("plan"), "{error}");
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM specs ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
        assert_eq!(ids, vec!["spec-billing".to_string()]);
    }

    fn approval(kind: &str, roles: &[&str]) -> ApprovalRequest {
        ApprovalRequest {
            artifact_kind: "plan".into(),
            artifact_id: "plan-ledger".into(),
            version: 1,
            actor_member_id: "member-1".into(),
            actor_kind: kind.into(),
            roles: roles.iter().map(|role| (*role).to_string()).collect(),
            reason: "ready".into(),
        }
    }

    #[tokio::test]
    async fn an_agent_cannot_approve_a_plan() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Write the ledger first."))
            .await
            .unwrap();
        let error = approve_artifact(&pool, approval("agent", &["owner"]))
            .await
            .unwrap_err();
        assert!(error.contains("agent"), "{error}");
        let status: String =
            sqlx::query_scalar("SELECT status FROM plan_revisions WHERE plan_id = 'plan-ledger'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "draft");
    }

    #[tokio::test]
    async fn an_owner_approval_keeps_the_plan_body() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Write the ledger first."))
            .await
            .unwrap();
        let before: String =
            sqlx::query_scalar("SELECT body FROM plan_revisions WHERE plan_id = 'plan-ledger'")
                .fetch_one(&pool)
                .await
                .unwrap();
        approve_artifact(&pool, approval("human", &["owner"]))
            .await
            .unwrap();
        let after: (String, String) = sqlx::query_as(
            "SELECT status, body FROM plan_revisions WHERE plan_id = 'plan-ledger' AND version = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after.0, "approved");
        assert_eq!(after.1, before);
    }

    #[tokio::test]
    async fn revising_an_approved_plan_appends_a_draft_and_keeps_the_approved_row() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Write the ledger first."))
            .await
            .unwrap();
        approve_artifact(&pool, approval("human", &["owner"]))
            .await
            .unwrap();
        let approved: (String, String, String) = sqlx::query_as(
            "SELECT status, body, content_hash FROM plan_revisions
             WHERE plan_id = 'plan-ledger' AND version = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(approved.0, "approved");
        let next = revise_plan(
            &pool,
            PlanEdit {
                plan_id: "plan-ledger".into(),
                based_on: 1,
                body: "Write the ledger, then the receipt.".into(),
                session_id: "web-ui".into(),
                turn_id: "local".into(),
                agent_id: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(next, 2);
        let rows: Vec<(i64, String, String, String)> = sqlx::query_as(
            "SELECT version, status, body, content_hash FROM plan_revisions
             WHERE plan_id = 'plan-ledger' ORDER BY version",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (rows[0].1.clone(), rows[0].2.clone(), rows[0].3.clone()),
            approved
        );
        assert_eq!(rows[1].1, "draft");
        assert_eq!(rows[1].2, "Write the ledger, then the receipt.");
    }

    #[tokio::test]
    async fn a_stale_plan_parent_is_a_conflict() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Write the ledger first."))
            .await
            .unwrap();
        let edit = PlanEdit {
            plan_id: "plan-ledger".into(),
            based_on: 1,
            body: "Write the ledger, then the receipt.".into(),
            session_id: "session-1".into(),
            turn_id: "turn-2".into(),
            agent_id: Some("agent-ax".into()),
        };
        assert_eq!(revise_plan(&pool, edit.clone()).await.unwrap(), 2);
        let error = revise_plan(&pool, edit).await.unwrap_err();
        assert!(error.contains("conflict"), "{error}");
        let latest: i64 = sqlx::query_scalar(
            "SELECT MAX(version) FROM plan_revisions WHERE plan_id = 'plan-ledger'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(latest, 2);
    }

    #[tokio::test]
    async fn legacy_plan_rows_stay_in_the_review_queue() {
        let (_dir, pool) = open().await;
        ensure_hierarchy_schema(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO specs (id, project_id, title, markdown_path, created_at)
             VALUES ('old-plan', 'ax', 'Old', 'docs/specs/old-plan/SPEC.md', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO spec_revisions
             (spec_id, version, status, body, content_hash, source, session_id, turn_id, based_on, created_at)
             VALUES ('old-plan', 1, 'draft', 'body', 'hash', 'plan_mode', '', '', NULL, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let before = count(&pool, "specs").await;
        let report = dry_run_legacy(&pool).await.unwrap();
        assert_eq!(report.queued, vec!["old-plan".to_string()]);
        assert_eq!(count(&pool, "specs").await, before);
        assert_eq!(count(&pool, "plans").await, 0);
    }

    #[tokio::test]
    async fn the_hierarchy_flag_defaults_off() {
        let (_dir, pool) = open().await;
        ensure_hierarchy_schema(&pool).await.unwrap();
        assert!(!hierarchy_enabled(&pool).await.unwrap());
        set_hierarchy_flag(&pool, true).await.unwrap();
        assert!(hierarchy_enabled(&pool).await.unwrap());
        set_hierarchy_flag(&pool, false).await.unwrap();
        assert!(!hierarchy_enabled(&pool).await.unwrap());
        assert_eq!(count(&pool, "plans").await, 0);
    }

    #[tokio::test]
    async fn the_same_capture_key_writes_one_pair() {
        let (_dir, pool) = open().await;
        let first = create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        let mut retry = sample("Record the ledger.");
        retry.spec_body = "A different body must not replace the first.".into();
        let second = create_spec_with_plan(&pool, retry).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(count(&pool, "specs").await, 1);
        assert_eq!(count(&pool, "spec_revisions").await, 1);
        assert_eq!(count(&pool, "plans").await, 1);
        assert_eq!(count(&pool, "plan_revisions").await, 1);
        let body: String =
            sqlx::query_scalar("SELECT body FROM spec_revisions WHERE spec_id = 'spec-billing'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(body, "Charge the recorded amount.");
    }

    #[tokio::test]
    async fn list_plan_revisions_returns_ordered_history() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        revise_plan(
            &pool,
            PlanEdit {
                plan_id: "plan-ledger".into(),
                based_on: 1,
                body: "Record the ledger and seal it.".into(),
                session_id: "session-1".into(),
                turn_id: "turn-2".into(),
                agent_id: Some("agent-ax".into()),
            },
        )
        .await
        .unwrap();
        let rows = list_plan_revisions(&pool, "plan-ledger").await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].version, 1);
        assert_eq!(rows[0].body, "Record the ledger.");
        assert_eq!(rows[1].version, 2);
        assert_eq!(rows[1].based_on, Some(1));
        assert!(list_plan_revisions(&pool, "missing")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn an_alternative_plan_keeps_its_own_history() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        let version = create_alternative_plan(
            &pool,
            NewAlternativePlan {
                project_id: "ax".into(),
                spec_id: "spec-billing".into(),
                plan_id: "plan-cash".into(),
                plan_title: "Cash plan".into(),
                plan_body: "Take cash.".into(),
                session_id: "session-1".into(),
                turn_id: "turn-2".into(),
                agent_id: Some("agent-ax".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(version, 1);
        assert_eq!(count(&pool, "specs").await, 1);
        assert_eq!(count(&pool, "plans").await, 2);
        let owner: String =
            sqlx::query_scalar("SELECT owner_spec_id FROM plans WHERE id = 'plan-cash'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(owner, "spec-billing");
        revise_plan(
            &pool,
            PlanEdit {
                plan_id: "plan-cash".into(),
                based_on: 1,
                body: "Take cash and receipt it.".into(),
                session_id: "session-1".into(),
                turn_id: "turn-3".into(),
                agent_id: Some("agent-ax".into()),
            },
        )
        .await
        .unwrap();
        let ledger: i64 = sqlx::query_scalar(
            "SELECT MAX(version) FROM plan_revisions WHERE plan_id = 'plan-ledger'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let cash: i64 = sqlx::query_scalar(
            "SELECT MAX(version) FROM plan_revisions WHERE plan_id = 'plan-cash'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(ledger, 1);
        assert_eq!(cash, 2);
    }

    #[tokio::test]
    async fn one_session_can_link_two_specs() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        let mut other = sample("File the return.");
        other.spec_id = "spec-tax".into();
        other.spec_title = "Tax".into();
        other.spec_body = "File the recorded return.".into();
        other.plan_id = "plan-tax".into();
        other.turn_id = "turn-2".into();
        create_spec_with_plan(&pool, other).await.unwrap();
        link_session(&pool, "session-9", "spec-billing")
            .await
            .unwrap();
        link_session(&pool, "session-9", "spec-tax").await.unwrap();
        let mut linked = specs_for_session(&pool, "session-9").await.unwrap();
        linked.sort();
        assert_eq!(
            linked,
            vec!["spec-billing".to_string(), "spec-tax".to_string()]
        );
    }

    fn cash_plan() -> NewAlternativePlan {
        NewAlternativePlan {
            project_id: "ax".into(),
            spec_id: "spec-billing".into(),
            plan_id: "plan-cash".into(),
            plan_title: "Cash plan".into(),
            plan_body: "Take cash.".into(),
            session_id: "session-1".into(),
            turn_id: "turn-2".into(),
            agent_id: Some("agent-ax".into()),
        }
    }

    #[tokio::test]
    async fn a_workstream_can_select_two_plans() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        create_alternative_plan(&pool, cash_plan()).await.unwrap();
        create_workstream(
            &pool,
            NewWorkstream {
                id: "ws-billing".into(),
                project_id: "ax".into(),
                spec_id: "spec-billing".into(),
                name: "Billing".into(),
            },
        )
        .await
        .unwrap();
        for plan_id in ["plan-ledger", "plan-cash"] {
            select_plan(
                &pool,
                PlanSelection {
                    workstream_id: "ws-billing".into(),
                    plan_id: plan_id.into(),
                    plan_version: 1,
                    session_id: "session-1".into(),
                    turn_id: "turn-4".into(),
                },
            )
            .await
            .unwrap();
        }
        let selected = selected_plans(&pool, "ws-billing").await.unwrap();
        assert_eq!(
            selected,
            vec![("plan-cash".to_string(), 1), ("plan-ledger".to_string(), 1),]
        );
    }

    #[tokio::test]
    async fn a_workstream_rejects_a_plan_from_another_spec() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        let mut other = sample("File the return.");
        other.spec_id = "spec-tax".into();
        other.spec_title = "Tax".into();
        other.spec_body = "File the recorded return.".into();
        other.plan_id = "plan-tax".into();
        other.turn_id = "turn-2".into();
        create_spec_with_plan(&pool, other).await.unwrap();
        create_workstream(
            &pool,
            NewWorkstream {
                id: "ws-billing".into(),
                project_id: "ax".into(),
                spec_id: "spec-billing".into(),
                name: "Billing".into(),
            },
        )
        .await
        .unwrap();
        let error = select_plan(
            &pool,
            PlanSelection {
                workstream_id: "ws-billing".into(),
                plan_id: "plan-tax".into(),
                plan_version: 1,
                session_id: "session-1".into(),
                turn_id: "turn-4".into(),
            },
        )
        .await
        .unwrap_err();
        assert!(error.contains("plan"), "{error}");
        assert!(selected_plans(&pool, "ws-billing")
            .await
            .unwrap()
            .is_empty());
    }

    fn gap_spec() -> NewSpecWithoutPlan {
        NewSpecWithoutPlan {
            project_id: "ax".into(),
            spec_id: "spec-gap".into(),
            spec_title: "Gap".into(),
            spec_body: "Describe the charge.".into(),
            session_id: "session-1".into(),
            turn_id: "turn-1".into(),
        }
    }

    async fn membership() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE team_members (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                email TEXT,
                role TEXT NOT NULL,
                status TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE member_roles (
                member_id TEXT NOT NULL,
                role TEXT NOT NULL,
                PRIMARY KEY (member_id, role)
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn member(pool: &SqlitePool, id: &str, role: &str) {
        sqlx::query(
            "INSERT INTO team_members (id, display_name, email, role, status)
             VALUES (?1, ?1, NULL, ?2, 'active')",
        )
        .bind(id)
        .bind(role)
        .execute(pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO member_roles (member_id, role) VALUES (?1, ?2)")
            .bind(id)
            .bind(role)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn a_spec_without_a_plan_cannot_become_execution_ready() {
        let (_dir, pool) = open().await;
        ensure_hierarchy_schema(&pool).await.unwrap();
        import_spec_without_plan(&pool, gap_spec()).await.unwrap();
        assert_eq!(count(&pool, "plans").await, 0);
        let error = mark_execution_ready(&pool, "spec-gap").await.unwrap_err();
        assert!(error.contains("plan"), "{error}");
        let needs_plan: i64 =
            sqlx::query_scalar("SELECT needs_plan FROM specs WHERE id = 'spec-gap'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(needs_plan, 1);
        let ready: i64 =
            sqlx::query_scalar("SELECT execution_ready FROM specs WHERE id = 'spec-gap'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(ready, 0);
        create_alternative_plan(
            &pool,
            NewAlternativePlan {
                project_id: "ax".into(),
                spec_id: "spec-gap".into(),
                plan_id: "plan-gap".into(),
                plan_title: "Gap plan".into(),
                plan_body: "Repair the gap.".into(),
                session_id: "session-1".into(),
                turn_id: "turn-2".into(),
                agent_id: None,
            },
        )
        .await
        .unwrap();
        let needs_plan: i64 =
            sqlx::query_scalar("SELECT needs_plan FROM specs WHERE id = 'spec-gap'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(needs_plan, 0);
        mark_execution_ready(&pool, "spec-gap").await.unwrap();
        let ready: i64 =
            sqlx::query_scalar("SELECT execution_ready FROM specs WHERE id = 'spec-gap'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(ready, 1);
    }

    #[tokio::test]
    async fn a_claimed_owner_role_without_membership_cannot_approve() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Write the ledger first."))
            .await
            .unwrap();
        let members = membership().await;
        member(&members, "pat", "member").await;
        let mut request = approval("human", &["owner"]);
        request.actor_member_id = "pat".into();
        let error = approve_for_member(&pool, &members, "ax", request)
            .await
            .unwrap_err();
        assert!(error.contains("approver"), "{error}");
        let status: String =
            sqlx::query_scalar("SELECT status FROM plan_revisions WHERE plan_id = 'plan-ledger'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "draft");
    }

    #[tokio::test]
    async fn an_admin_membership_can_approve_a_plan() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Write the ledger first."))
            .await
            .unwrap();
        let members = membership().await;
        member(&members, "a4c2d177-d0ca-4d7d-8789-cba7ebcae5c7", "admin").await;
        let mut request = approval("human", &["admin"]);
        request.actor_member_id = "a4c2d177-d0ca-4d7d-8789-cba7ebcae5c7".into();
        approve_for_member(&pool, &members, "ax", request)
            .await
            .unwrap();
        let status: String =
            sqlx::query_scalar("SELECT status FROM plan_revisions WHERE plan_id = 'plan-ledger'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "approved");
    }

    #[tokio::test]
    async fn an_approver_membership_is_limited_to_the_spec_project() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Write the ledger first."))
            .await
            .unwrap();
        let before: String =
            sqlx::query_scalar("SELECT body FROM plan_revisions WHERE plan_id = 'plan-ledger'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let members = membership().await;
        member(&members, "ada", "approver").await;
        let mut request = approval("human", &["member"]);
        request.actor_member_id = "ada".into();
        let error = approve_for_member(&pool, &members, "other", request.clone())
            .await
            .unwrap_err();
        assert!(error.contains("project"), "{error}");
        approve_for_member(&pool, &members, "ax", request)
            .await
            .unwrap();
        let after: (String, String) = sqlx::query_as(
            "SELECT status, body FROM plan_revisions WHERE plan_id = 'plan-ledger' AND version = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after.0, "approved");
        assert_eq!(after.1, before);
    }

    fn create_body() -> serde_json::Value {
        serde_json::json!({
            "projectId": "ax",
            "specId": "spec-billing",
            "specTitle": "Billing",
            "specBody": "Charge the recorded amount.",
            "planId": "plan-ledger",
            "planTitle": "Ledger plan",
            "planBody": "Record the ledger.",
            "sessionId": "session-1",
            "turnId": "turn-1",
            "agentId": "agent-ax"
        })
    }

    #[tokio::test]
    async fn hierarchy_create_json_matches_on_http_cli_and_mcp() {
        let (_dir, pool) = open().await;
        let body = create_body();
        let first = create_hierarchy_from_json(&pool, body.clone()).await;
        let second = create_hierarchy_from_json(&pool, body).await;
        assert_eq!(first, second);
        assert_eq!(first["ok"], true);
        assert_eq!(first["specId"], "spec-billing");
        assert_eq!(first["specVersion"], 1);
        assert_eq!(first["planId"], "plan-ledger");
        assert_eq!(first["planVersion"], 1);
        assert_eq!(count(&pool, "specs").await, 1);
        assert_eq!(count(&pool, "plans").await, 1);
        let missing = create_hierarchy_from_json(&pool, serde_json::json!({})).await;
        assert_eq!(missing["ok"], false);
        assert!(
            missing["error"]
                .as_str()
                .unwrap_or("")
                .contains("projectId")
                || !missing["error"].as_str().unwrap_or("").is_empty()
        );
    }

    #[tokio::test]
    async fn hierarchy_follow_up_json_covers_plan_revise_and_link() {
        let (_dir, pool) = open().await;
        let created = create_hierarchy_from_json(&pool, create_body()).await;
        assert_eq!(created["ok"], true);
        let alternative = alternative_plan_from_json(
            &pool,
            serde_json::json!({
                "projectId": "ax",
                "specId": "spec-billing",
                "planId": "plan-cash",
                "planTitle": "Cash plan",
                "planBody": "Take cash.",
                "sessionId": "session-1",
                "turnId": "turn-2"
            }),
        )
        .await;
        assert_eq!(alternative["ok"], true);
        assert_eq!(alternative["planId"], "plan-cash");
        assert_eq!(alternative["planVersion"], 1);
        let revised = revise_plan_from_json(
            &pool,
            serde_json::json!({
                "planId": "plan-cash",
                "basedOn": 1,
                "body": "Take cash and receipt it.",
                "sessionId": "session-1",
                "turnId": "turn-3"
            }),
        )
        .await;
        assert_eq!(revised["ok"], true);
        assert_eq!(revised["planVersion"], 2);
        let conflict = revise_plan_from_json(
            &pool,
            serde_json::json!({
                "planId": "plan-cash",
                "basedOn": 1,
                "body": "A stale edit.",
                "sessionId": "session-1",
                "turnId": "turn-4"
            }),
        )
        .await;
        assert_eq!(conflict["ok"], false);
        assert!(conflict["error"]
            .as_str()
            .unwrap_or("")
            .contains("conflict"));
        let linked = link_session_from_json(
            &pool,
            serde_json::json!({ "sessionId": "session-9", "specId": "spec-billing" }),
        )
        .await;
        assert_eq!(linked["ok"], true);
        assert_eq!(linked["specIds"], serde_json::json!(["spec-billing"]));
        let again = link_session_from_json(
            &pool,
            serde_json::json!({ "sessionId": "session-9", "specId": "spec-billing" }),
        )
        .await;
        assert_eq!(again, linked);
    }

    #[tokio::test]
    async fn hierarchy_reads_cover_selection_compare_list_and_provenance() {
        let (_dir, pool) = open().await;
        assert_eq!(
            create_hierarchy_from_json(&pool, create_body()).await["ok"],
            true
        );
        create_workstream(
            &pool,
            NewWorkstream {
                id: "ws-billing".into(),
                project_id: "ax".into(),
                spec_id: "spec-billing".into(),
                name: "Billing".into(),
            },
        )
        .await
        .unwrap();
        let selected = select_plan_from_json(
            &pool,
            serde_json::json!({
                "workstreamId": "ws-billing",
                "planId": "plan-ledger",
                "planVersion": 1,
                "sessionId": "session-1",
                "turnId": "turn-4"
            }),
        )
        .await;
        assert_eq!(selected["ok"], true);
        assert_eq!(
            selected["selections"],
            serde_json::json!([{ "planId": "plan-ledger", "planVersion": 1 }])
        );
        let revised = revise_plan_from_json(
            &pool,
            serde_json::json!({
                "planId": "plan-ledger",
                "basedOn": 1,
                "body": "Record the ledger twice.",
                "sessionId": "session-1",
                "turnId": "turn-5"
            }),
        )
        .await;
        assert_eq!(revised["planVersion"], 2);
        let compared = compare_plan_from_json(
            &pool,
            serde_json::json!({ "planId": "plan-ledger", "left": 1, "right": 2 }),
        )
        .await;
        assert_eq!(compared["ok"], true);
        assert_ne!(compared["left"]["body"], compared["right"]["body"]);
        assert_ne!(compared["left"]["hash"], compared["right"]["hash"]);
        let listed =
            list_hierarchy_from_json(&pool, serde_json::json!({ "projectId": "ax" })).await;
        assert_eq!(listed["ok"], true);
        assert_eq!(listed["specs"][0]["specId"], "spec-billing");
        assert_eq!(listed["specs"][0]["plans"][0]["planId"], "plan-ledger");
        assert_eq!(listed["specs"][0]["plans"][0]["latestVersion"], 2);
        let provenance = provenance_from_json(
            &pool,
            serde_json::json!({ "planId": "plan-ledger", "version": 1 }),
        )
        .await;
        assert_eq!(provenance["ok"], true);
        assert_eq!(provenance["sessionId"], "session-1");
        assert_eq!(provenance["turnId"], "turn-1");
        assert_eq!(provenance["specId"], "spec-billing");
        assert_eq!(provenance["specVersion"], 1);
    }

    #[tokio::test]
    async fn approval_json_ignores_a_claimed_role() {
        let (_dir, pool) = open().await;
        assert_eq!(
            create_hierarchy_from_json(&pool, create_body()).await["ok"],
            true
        );
        let before: String =
            sqlx::query_scalar("SELECT body FROM plan_revisions WHERE plan_id = 'plan-ledger'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let members = membership().await;
        member(&members, "pat", "member").await;
        let denied = approve_from_json(
            &pool,
            &members,
            serde_json::json!({
                "projectId": "ax",
                "artifactKind": "plan",
                "artifactId": "plan-ledger",
                "version": 1,
                "actorMemberId": "pat",
                "actorKind": "human",
                "roles": ["owner"],
                "reason": "ready"
            }),
        )
        .await;
        assert_eq!(denied["ok"], false);
        assert!(denied["error"].as_str().unwrap_or("").contains("approver"));
        let status: String =
            sqlx::query_scalar("SELECT status FROM plan_revisions WHERE plan_id = 'plan-ledger'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(status, "draft");
        member(&members, "ada", "approver").await;
        let agent = approve_from_json(
            &pool,
            &members,
            serde_json::json!({
                "projectId": "ax",
                "artifactKind": "plan",
                "artifactId": "plan-ledger",
                "version": 1,
                "actorMemberId": "ada",
                "actorKind": "agent",
                "roles": ["owner"],
                "reason": "ready"
            }),
        )
        .await;
        assert_eq!(agent["ok"], false);
        assert!(agent["error"].as_str().unwrap_or("").contains("agent"));
        let approved = approve_from_json(
            &pool,
            &members,
            serde_json::json!({
                "projectId": "ax",
                "artifactKind": "plan",
                "artifactId": "plan-ledger",
                "version": 1,
                "actorMemberId": "ada",
                "actorKind": "human",
                "roles": ["member"],
                "reason": "ready"
            }),
        )
        .await;
        assert_eq!(approved["ok"], true);
        let after: (String, String) = sqlx::query_as(
            "SELECT status, body FROM plan_revisions WHERE plan_id = 'plan-ledger' AND version = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after.0, "approved");
        assert_eq!(after.1, before);
    }

    #[test]
    fn a_claimed_member_id_does_not_override_the_session() {
        let mismatch = bind_approval_actor(
            serde_json::json!({
                "actorMemberId": "pat",
                "actorKind": "agent",
                "reason": "ready"
            }),
            "ada",
        )
        .unwrap_err();
        assert!(mismatch.contains("member"), "{mismatch}");
        let bound = bind_approval_actor(serde_json::json!({ "reason": "ready" }), "ada").unwrap();
        assert_eq!(bound["actorMemberId"], "ada");
        assert_eq!(bound["actorKind"], "human");
        let same = bind_approval_actor(
            serde_json::json!({ "actorMemberId": "ada", "actorKind": "agent" }),
            "ada",
        )
        .unwrap();
        assert_eq!(same["actorMemberId"], "ada");
        assert_eq!(same["actorKind"], "human");
    }

    #[tokio::test]
    async fn the_hierarchy_flag_gates_list_preflight_and_capture() {
        let _lock = super::super::spec_registry::tests::HOME_LOCK.lock().await;
        let _home = super::super::spec_registry::tests::IsolatedHome::new();
        let (dir, pool) = open().await;
        assert_eq!(
            create_hierarchy_from_json(&pool, create_body()).await["ok"],
            true
        );
        link_session(&pool, "session-1", "spec-billing")
            .await
            .unwrap();
        let off = gated_preflight_block(&pool, dir.path(), "session-1").await;
        let legacy =
            super::super::spec_registry::spec_context_block(&pool, dir.path(), "session-1").await;
        assert_eq!(off, legacy);
        assert_eq!(
            off, "none",
            "a session without a legacy binding sees no spec"
        );
        assert_eq!(gated_list(&pool, "ax").await["mode"], "legacy");
        let blocked = capture_plan_draft(
            &pool,
            serde_json::json!({
                "planId": "plan-ledger",
                "basedOn": 1,
                "body": "A captured turn.",
                "sessionId": "session-1",
                "turnId": "turn-9"
            }),
        )
        .await;
        assert_eq!(blocked["ok"], false);
        assert!(blocked["error"]
            .as_str()
            .unwrap_or("")
            .contains("hierarchy"));
        set_hierarchy_flag(&pool, true).await.unwrap();
        let on = gated_preflight_block(&pool, dir.path(), "session-1").await;
        assert!(on.contains("hierarchy: on"), "{on}");
        assert!(on.contains("plan-ledger"), "{on}");
        assert!(!on.contains("Record the ledger."), "{on}");
        let listed = gated_list(&pool, "ax").await;
        assert_eq!(listed["mode"], "hierarchy");
        let billing = listed["specs"]
            .as_array()
            .and_then(|specs| specs.iter().find(|spec| spec["specId"] == "spec-billing"))
            .expect("spec-billing");
        assert_eq!(billing["plans"][0]["planId"], "plan-ledger");
        let specs_before = count(&pool, "specs").await;
        let captured = capture_plan_draft(
            &pool,
            serde_json::json!({
                "planId": "plan-ledger",
                "basedOn": 1,
                "body": "A captured turn.",
                "sessionId": "session-1",
                "turnId": "turn-9"
            }),
        )
        .await;
        assert_eq!(captured["ok"], true);
        assert_eq!(captured["planVersion"], 2);
        assert_eq!(count(&pool, "specs").await, specs_before);
    }

    #[tokio::test]
    async fn finished_plan_import_stays_a_spec_until_the_flag_is_on() {
        let (_dir, pool) = open().await;
        let legacy = super::super::spec_registry::capture_finished_plan(
            &pool,
            "ax",
            "plan-off",
            "Off",
            "make a plan",
            "Write the steps.\nDo the second step.",
            "session-1",
            "turn-off",
            "plan_mode",
            true,
        )
        .await
        .unwrap();
        assert!(legacy.is_some());
        assert_eq!(count(&pool, "specs").await, 1);
        set_hierarchy_flag(&pool, true).await.unwrap();
        let queued = super::super::spec_registry::capture_finished_plan(
            &pool,
            "ax",
            "plan-on",
            "On",
            "make a plan",
            "Write the new steps.\nDo the second step.",
            "session-1",
            "turn-on",
            "plan_mode",
            true,
        )
        .await
        .unwrap();
        assert!(queued.is_none());
        assert_eq!(count(&pool, "specs").await, 1);
        assert_eq!(count(&pool, "plans").await, 0);
        assert_eq!(
            import_review_ids(&pool).await.unwrap(),
            vec!["plan-on".to_string()]
        );
    }

    #[tokio::test]
    async fn the_import_review_queue_lists_flagged_plans() {
        let (_dir, pool) = open().await;
        hierarchy_flag_from_json(&pool, serde_json::json!({ "enabled": true })).await;
        let stored = super::super::spec_registry::capture_finished_plan(
            &pool,
            "ax",
            "plan-queued",
            "Queued",
            "make a plan",
            "Write the steps.\nDo the second step.",
            "session-1",
            "turn-queue",
            "plan_mode",
            true,
        )
        .await
        .unwrap();
        assert!(stored.is_none());
        let review = import_review_from_json(&pool).await;
        assert_eq!(review["ok"], true);
        assert_eq!(review["ids"], serde_json::json!(["plan-queued"]));
        assert_eq!(count(&pool, "specs").await, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_edits_of_one_version_get_one_win_and_conflict_payloads() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        let plans = (0..8).map(|index| {
            let pool = pool.clone();
            tokio::spawn(async move {
                revise_plan_from_json(
                    &pool,
                    serde_json::json!({
                        "planId": "plan-ledger",
                        "basedOn": 1,
                        "body": format!("Plan edit {index}."),
                        "sessionId": "session-1",
                        "turnId": format!("turn-plan-{index}"),
                    }),
                )
                .await
            })
        });
        let specs = (0..8).map(|index| {
            let pool = pool.clone();
            tokio::spawn(async move {
                revise_spec_from_json(
                    &pool,
                    serde_json::json!({
                        "specId": "spec-billing",
                        "basedOn": 1,
                        "body": format!("Spec edit {index}."),
                        "sessionId": "session-1",
                        "turnId": format!("turn-spec-{index}"),
                    }),
                )
                .await
            })
        });
        let tasks: Vec<_> = plans.chain(specs).collect();
        let mut results = Vec::new();
        for task in tasks {
            results.push(task.await.unwrap());
        }
        for (label, group) in [("plan", &results[..8]), ("spec", &results[8..])] {
            let wins = group.iter().filter(|value| value["ok"] == true).count();
            assert_eq!(wins, 1, "{label}: {group:?}");
            for value in group.iter().filter(|value| value["ok"] != true) {
                assert_eq!(value["error"], "conflict", "{label}: {value}");
                assert!(value["head"]["body"].is_string(), "{label}: {value}");
                assert!(value["base"]["body"].is_string(), "{label}: {value}");
            }
        }
    }

    #[tokio::test]
    async fn an_unchanged_plan_is_queued_once_across_turns() {
        let (_dir, pool) = open().await;
        set_hierarchy_flag(&pool, true).await.unwrap();
        for turn in ["turn-1", "turn-2", "turn-3"] {
            queue_finished_plan(
                &pool,
                "ax",
                "plan-x",
                "X",
                "Same body.",
                "session-1",
                turn,
                "file",
            )
            .await
            .unwrap();
        }
        assert_eq!(count(&pool, "import_review").await, 1);
        queue_finished_plan(
            &pool,
            "ax",
            "plan-x",
            "X",
            "Changed body.",
            "session-1",
            "turn-4",
            "file",
        )
        .await
        .unwrap();
        assert_eq!(count(&pool, "import_review").await, 2);
        assert_eq!(
            import_review_ids(&pool).await.unwrap(),
            vec!["plan-x".to_string()]
        );
    }

    #[tokio::test]
    async fn turning_the_hierarchy_flag_off_restores_spec_capture() {
        let (_dir, pool) = open().await;
        let initial = hierarchy_flag_from_json(&pool, serde_json::json!({})).await;
        assert_eq!(initial["ok"], true);
        assert_eq!(initial["enabled"], false);
        assert_eq!(initial["key"], "hierarchy_v2");
        let enabled = hierarchy_flag_from_json(&pool, serde_json::json!({ "enabled": true })).await;
        assert_eq!(enabled["enabled"], true);
        let queued = super::super::spec_registry::capture_finished_plan(
            &pool,
            "ax",
            "plan-flagged",
            "Flagged",
            "make a plan",
            "Write the steps.\nDo the second step.",
            "session-1",
            "turn-flag",
            "plan_mode",
            true,
        )
        .await
        .unwrap();
        assert!(queued.is_none());
        let disabled =
            hierarchy_flag_from_json(&pool, serde_json::json!({ "enabled": false })).await;
        assert_eq!(disabled["enabled"], false);
        let restored = super::super::spec_registry::capture_finished_plan(
            &pool,
            "ax",
            "plan-restored",
            "Restored",
            "make a plan",
            "Write the steps.\nDo the second step.",
            "session-1",
            "turn-restored",
            "plan_mode",
            true,
        )
        .await
        .unwrap();
        assert!(restored.is_some());
        assert_eq!(count(&pool, "specs").await, 1);
    }

    #[tokio::test]
    async fn a_dry_run_keeps_the_legacy_payload_hash_and_a_fresh_project_stays_separate() {
        let (_dir, legacy) = open().await;
        ensure_hierarchy_schema(&legacy).await.unwrap();
        for (id, body) in [("old-plan", "keep this body"), ("other-plan", "other body")] {
            sqlx::query(
                "INSERT INTO specs (id, project_id, title, markdown_path, created_at)
                 VALUES (?1, 'ax', ?1, ?2, 1)",
            )
            .bind(id)
            .bind(format!("docs/specs/{id}/SPEC.md"))
            .execute(&legacy)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO spec_revisions
                 (spec_id, version, status, body, content_hash, source, session_id, turn_id, based_on, created_at)
                 VALUES (?1, 1, 'draft', ?2, 'hash', 'plan_mode', '', '', NULL, 1)",
            )
            .bind(id)
            .bind(body)
            .execute(&legacy)
            .await
            .unwrap();
        }
        let before = legacy_table_fingerprints(&legacy).await.unwrap();
        let report = dry_run_legacy(&legacy).await.unwrap();
        let after = legacy_table_fingerprints(&legacy).await.unwrap();
        assert_eq!(after, before);
        assert_eq!(
            report.queued,
            vec!["old-plan".to_string(), "other-plan".to_string()]
        );
        assert_eq!(report.spec_count, 2);
        assert_eq!(count(&legacy, "plans").await, 0);
        let specs = before.iter().find(|item| item.table == "specs").unwrap();
        assert_eq!(specs.rows, 2);
        assert_ne!(specs.sha256, super::empty_sha256());
        sqlx::query("UPDATE spec_revisions SET body = 'changed' WHERE spec_id = 'old-plan'")
            .execute(&legacy)
            .await
            .unwrap();
        let changed = legacy_table_fingerprints(&legacy).await.unwrap();
        assert_ne!(changed, before);

        let (_fresh_dir, fresh) = open().await;
        let policy_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM policy_revisions")
            .fetch_one(&fresh)
            .await
            .unwrap();
        set_hierarchy_flag(&fresh, true).await.unwrap();
        let created = create_hierarchy_from_json(&fresh, create_body()).await;
        assert_eq!(created["ok"], true);
        let listed = gated_list(&fresh, "ax").await;
        assert_eq!(listed["mode"], "hierarchy");
        assert_eq!(listed["specs"][0]["specId"], "spec-billing");
        assert_eq!(listed["specs"][0]["plans"][0]["planId"], "plan-ledger");
        let stored: String =
            sqlx::query_scalar("SELECT body FROM plan_revisions WHERE plan_id = 'plan-ledger'")
                .fetch_one(&fresh)
                .await
                .unwrap();
        assert_eq!(stored, "Record the ledger.");
        let policy_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM policy_revisions")
            .fetch_one(&fresh)
            .await
            .unwrap();
        assert_eq!(policy_after, policy_before);
        set_hierarchy_flag(&fresh, false).await.unwrap();
        assert_eq!(gated_list(&fresh, "ax").await["mode"], "legacy");
    }

    #[tokio::test]
    #[ignore = "requires AX_HIERARCHY_FIXTURE, a copy that is not the live .ax database"]
    async fn copied_project_database_dry_run_keeps_hashes() {
        let path = std::env::var("AX_HIERARCHY_FIXTURE").expect("AX_HIERARCHY_FIXTURE");
        assert!(
            !path.replace('\\', "/").ends_with(".ax/ax.db"),
            "refusing to dry-run the live database"
        );
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&path)
            .read_only(false)
            .create_if_missing(false);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        let before = legacy_table_fingerprints(&pool).await.unwrap();
        let report = dry_run_legacy(&pool).await.unwrap();
        let after = legacy_table_fingerprints(&pool).await.unwrap();
        for item in &before {
            println!(
                "fingerprint {} rows={} bytes={} sha256={}",
                item.table, item.rows, item.bytes, item.sha256
            );
        }
        println!(
            "dry-run queued={} spec_count={} plans_after_checked_separately",
            report.queued.len(),
            report.spec_count
        );
        assert_eq!(after, before);
        assert_eq!(
            report.spec_count,
            before
                .iter()
                .find(|item| item.table == "specs")
                .unwrap()
                .rows
        );
        assert_eq!(count(&pool, "plans").await, 0);
        assert!(!report.queued.is_empty());
    }

    #[tokio::test]
    async fn a_policy_link_stores_the_revision_hash_and_not_the_body() {
        let (_dir, pool) = open().await;
        set_hierarchy_flag(&pool, true).await.unwrap();
        assert_eq!(
            create_hierarchy_from_json(&pool, create_body()).await["ok"],
            true
        );
        let rule = "RULE-BODY-MUST-NOT-BE-COPIED";
        sqlx::query(
            "INSERT INTO policy_revisions (kind, item_id, content_hash, body, source, created_at)
             VALUES ('rule', 'english-only', 'hash-1', ?1, 'save', 1)",
        )
        .bind(rule)
        .execute(&pool)
        .await
        .unwrap();
        let revision_id: i64 =
            sqlx::query_scalar("SELECT id FROM policy_revisions WHERE item_id = 'english-only'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let rejected = link_policy_from_json(
            &pool,
            serde_json::json!({
                "artifactKind": "spec",
                "artifactId": "spec-billing",
                "policyRevisionId": revision_id,
                "body": rule,
            }),
        )
        .await;
        assert_eq!(rejected["ok"], false);
        assert!(rejected["error"].as_str().unwrap_or("").contains("body"));
        assert_eq!(count(&pool, "artifact_policy_links").await, 0);

        let wrong_hash = link_policy_from_json(
            &pool,
            serde_json::json!({
                "artifactKind": "spec",
                "artifactId": "spec-billing",
                "policyRevisionId": revision_id,
                "contentHash": "wrong",
            }),
        )
        .await;
        assert_eq!(wrong_hash["ok"], false);
        assert_eq!(count(&pool, "artifact_policy_links").await, 0);

        let linked = link_policy_from_json(
            &pool,
            serde_json::json!({
                "artifactKind": "spec",
                "artifactId": "spec-billing",
                "policyRevisionId": revision_id,
            }),
        )
        .await;
        assert_eq!(linked["ok"], true);
        assert_eq!(linked["contentHash"], "hash-1");
        assert_eq!(linked["policyKind"], "rule");
        assert_eq!(linked["policyItemId"], "english-only");
        assert!(linked.get("body").is_none());
        let again = link_policy_from_json(
            &pool,
            serde_json::json!({
                "artifactKind": "spec",
                "artifactId": "spec-billing",
                "policyRevisionId": revision_id,
            }),
        )
        .await;
        assert_eq!(again, linked);
        assert_eq!(count(&pool, "artifact_policy_links").await, 1);

        let spec_body: String =
            sqlx::query_scalar("SELECT body FROM spec_revisions WHERE spec_id = 'spec-billing'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let plan_body: String =
            sqlx::query_scalar("SELECT body FROM plan_revisions WHERE plan_id = 'plan-ledger'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(!spec_body.contains(rule));
        assert!(!plan_body.contains(rule));
        let policy_body: String =
            sqlx::query_scalar("SELECT body FROM policy_revisions WHERE id = ?1")
                .bind(revision_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(policy_body, rule);

        let missing = link_policy_from_json(
            &pool,
            serde_json::json!({
                "artifactKind": "spec",
                "artifactId": "spec-billing",
                "policyRevisionId": 99999,
            }),
        )
        .await;
        assert_eq!(missing["ok"], false);
        let schema: String = sqlx::query_scalar(
            "SELECT sql FROM sqlite_master WHERE name = 'artifact_policy_links'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(!schema.to_lowercase().contains("body"));
    }

    #[tokio::test]
    async fn an_approved_spec_change_marks_only_that_specs_selections() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        create_spec_with_plan(
            &pool,
            NewSpecPlan {
                project_id: "ax".into(),
                spec_id: "spec-other".into(),
                spec_title: "Other".into(),
                spec_body: "Leave this spec alone.".into(),
                plan_id: "plan-other".into(),
                plan_title: "Other plan".into(),
                plan_body: "Other steps.".into(),
                session_id: "session-2".into(),
                turn_id: "turn-1".into(),
                agent_id: None,
            },
        )
        .await
        .unwrap();
        for (id, spec_id) in [("ws-billing", "spec-billing"), ("ws-other", "spec-other")] {
            create_workstream(
                &pool,
                NewWorkstream {
                    id: id.into(),
                    project_id: "ax".into(),
                    spec_id: spec_id.into(),
                    name: spec_id.into(),
                },
            )
            .await
            .unwrap();
        }
        select_plan(
            &pool,
            PlanSelection {
                workstream_id: "ws-billing".into(),
                plan_id: "plan-ledger".into(),
                plan_version: 1,
                session_id: "session-1".into(),
                turn_id: "turn-4".into(),
            },
        )
        .await
        .unwrap();
        select_plan(
            &pool,
            PlanSelection {
                workstream_id: "ws-other".into(),
                plan_id: "plan-other".into(),
                plan_version: 1,
                session_id: "session-2".into(),
                turn_id: "turn-1".into(),
            },
        )
        .await
        .unwrap();
        let approve = |kind: &str, id: &str, version: i64| ApprovalRequest {
            artifact_kind: kind.into(),
            artifact_id: id.into(),
            version,
            actor_member_id: "member-1".into(),
            actor_kind: "human".into(),
            roles: vec!["owner".into()],
            reason: "checked".into(),
        };
        approve_artifact(&pool, approve("spec", "spec-billing", 1))
            .await
            .unwrap();
        let ledger = compatibility(&pool, "plan-ledger").await;
        assert_eq!(ledger, "current");
        let body = "Charge the recorded amount, then reconcile.";
        sqlx::query(
            "INSERT INTO spec_revisions
             (spec_id, version, status, body, content_hash, source, session_id, turn_id, based_on, created_at)
             VALUES ('spec-billing', 2, 'draft', ?1, ?2, 'edit', 'session-1', 'turn-9', 1, 2)",
        )
        .bind(body)
        .bind(hash_text(body))
        .execute(&pool)
        .await
        .unwrap();
        let plan_hash: String = sqlx::query_scalar(
            "SELECT spec_content_hash FROM plan_revisions WHERE plan_id = 'plan-ledger' AND version = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let plan_body: String = sqlx::query_scalar(
            "SELECT body FROM plan_revisions WHERE plan_id = 'plan-ledger' AND version = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        approve_artifact(&pool, approve("spec", "spec-billing", 2))
            .await
            .unwrap();
        assert_eq!(
            compatibility(&pool, "plan-ledger").await,
            "compatibility_review_required"
        );
        assert_eq!(compatibility(&pool, "plan-other").await, "current");
        let plan_hash_after: String = sqlx::query_scalar(
            "SELECT spec_content_hash FROM plan_revisions WHERE plan_id = 'plan-ledger' AND version = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let plan_body_after: String = sqlx::query_scalar(
            "SELECT body FROM plan_revisions WHERE plan_id = 'plan-ledger' AND version = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(plan_hash_after, plan_hash);
        assert_eq!(plan_body_after, plan_body);
        confirm_plan_compatibility(&pool, "ws-billing:plan-ledger:1:session-1:turn-4")
            .await
            .unwrap();
        assert_eq!(compatibility(&pool, "plan-ledger").await, "current");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT spec_content_hash FROM plan_revisions WHERE plan_id = 'plan-ledger' AND version = 1",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            plan_hash
        );
    }

    #[tokio::test]
    async fn a_stale_plan_json_returns_the_three_bodies() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Write the ledger first."))
            .await
            .unwrap();
        let saved = revise_plan_from_json(
            &pool,
            serde_json::json!({
                "planId": "plan-ledger",
                "basedOn": 1,
                "body": "Write the ledger, then the receipt.",
                "sessionId": "session-1",
                "turnId": "turn-2",
            }),
        )
        .await;
        assert_eq!(saved["ok"], true);
        assert_eq!(saved["planVersion"], 2);
        let conflict = revise_plan_from_json(
            &pool,
            serde_json::json!({
                "planId": "plan-ledger",
                "basedOn": 1,
                "body": "Rejected write.",
                "sessionId": "session-1",
                "turnId": "turn-3",
            }),
        )
        .await;
        assert_eq!(conflict["ok"], false);
        assert!(conflict["error"]
            .as_str()
            .unwrap_or("")
            .contains("conflict"));
        assert_eq!(conflict["base"]["version"], 1);
        assert_eq!(conflict["base"]["body"], "Write the ledger first.");
        assert_eq!(conflict["head"]["version"], 2);
        assert_eq!(
            conflict["head"]["body"],
            "Write the ledger, then the receipt."
        );
        assert_eq!(conflict["incoming"]["body"], "Rejected write.");
        let latest: i64 = sqlx::query_scalar(
            "SELECT MAX(version) FROM plan_revisions WHERE plan_id = 'plan-ledger'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(latest, 2);
    }

    #[tokio::test]
    async fn a_spec_revision_keeps_history_and_a_stale_parent_returns_three_bodies() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        let saved = revise_spec_from_json(
            &pool,
            serde_json::json!({
                "specId": "spec-billing",
                "basedOn": 1,
                "body": "Charge the recorded amount, then reconcile.",
                "sessionId": "session-1",
                "turnId": "turn-2",
            }),
        )
        .await;
        assert_eq!(saved["ok"], true);
        assert_eq!(saved["specVersion"], 2);
        let original: String = sqlx::query_scalar(
            "SELECT body FROM spec_revisions WHERE spec_id = 'spec-billing' AND version = 1",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(original, "Charge the recorded amount.");
        let conflict = revise_spec_from_json(
            &pool,
            serde_json::json!({
                "specId": "spec-billing",
                "basedOn": 1,
                "body": "Rejected spec.",
                "sessionId": "session-1",
                "turnId": "turn-3",
            }),
        )
        .await;
        assert_eq!(conflict["ok"], false);
        assert!(conflict["error"]
            .as_str()
            .unwrap_or("")
            .contains("conflict"));
        assert_eq!(conflict["base"]["version"], 1);
        assert_eq!(conflict["base"]["body"], "Charge the recorded amount.");
        assert_eq!(conflict["head"]["version"], 2);
        assert_eq!(
            conflict["head"]["body"],
            "Charge the recorded amount, then reconcile."
        );
        assert_eq!(conflict["incoming"]["body"], "Rejected spec.");
        let latest: i64 = sqlx::query_scalar(
            "SELECT MAX(version) FROM spec_revisions WHERE spec_id = 'spec-billing'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(latest, 2);
        let plan_body: String =
            sqlx::query_scalar("SELECT body FROM plan_revisions WHERE plan_id = 'plan-ledger'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(plan_body, "Record the ledger.");
        let empty = revise_spec_from_json(
            &pool,
            serde_json::json!({
                "specId": "spec-billing",
                "basedOn": 2,
                "body": "   ",
                "sessionId": "session-1",
                "turnId": "turn-4",
            }),
        )
        .await;
        assert_eq!(empty["ok"], false);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT MAX(version) FROM spec_revisions WHERE spec_id = 'spec-billing'",
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            2
        );
    }

    #[tokio::test]
    async fn revising_an_unknown_spec_is_a_missing_spec() {
        let (_dir, pool) = open().await;
        let edit = |based_on: i64| SpecEdit {
            spec_id: "ghost".into(),
            based_on,
            body: "A body for nobody.".into(),
            session_id: "session-1".into(),
            turn_id: "turn-1".into(),
        };
        assert_eq!(
            revise_spec(&pool, edit(0)).await,
            Err("missing spec".into())
        );
        assert_eq!(
            revise_spec(&pool, edit(3)).await,
            Err("missing spec".into())
        );
        let orphans: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM spec_revisions WHERE spec_id = 'ghost'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(orphans, 0);
        let json = revise_spec_from_json(
            &pool,
            serde_json::json!({
                "specId": "ghost", "basedOn": 2, "body": "x",
                "sessionId": "s", "turnId": "t",
            }),
        )
        .await;
        assert_eq!(json["error"], "missing spec");
        assert!(json.get("head").is_none());
    }

    async fn approve_spec_v2(pool: &SqlitePool) {
        let approve = |version: i64| ApprovalRequest {
            artifact_kind: "spec".into(),
            artifact_id: "spec-billing".into(),
            version,
            actor_member_id: "member-1".into(),
            actor_kind: "human".into(),
            roles: vec!["owner".into()],
            reason: "checked".into(),
        };
        approve_artifact(pool, approve(1)).await.unwrap();
        let v2 = revise_spec(
            pool,
            SpecEdit {
                spec_id: "spec-billing".into(),
                based_on: 1,
                body: "Charge the recorded amount, then reconcile.".into(),
                session_id: "session-1".into(),
                turn_id: "turn-9".into(),
            },
        )
        .await
        .unwrap();
        approve_artifact(pool, approve(v2)).await.unwrap();
    }

    async fn billing_workstream(pool: &SqlitePool) {
        create_workstream(
            pool,
            NewWorkstream {
                id: "ws-billing".into(),
                project_id: "ax".into(),
                spec_id: "spec-billing".into(),
                name: "billing".into(),
            },
        )
        .await
        .unwrap();
    }

    fn pick(version: i64, turn: &str) -> PlanSelection {
        PlanSelection {
            workstream_id: "ws-billing".into(),
            plan_id: "plan-ledger".into(),
            plan_version: version,
            session_id: "session-1".into(),
            turn_id: turn.into(),
        }
    }

    #[tokio::test]
    async fn selecting_a_plan_written_against_a_superseded_spec_needs_review() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        billing_workstream(&pool).await;
        // Spec v2 is approved while no plan is selected yet.
        approve_spec_v2(&pool).await;
        select_plan(&pool, pick(1, "turn-10")).await.unwrap();
        assert_eq!(
            compatibility(&pool, "plan-ledger").await,
            "compatibility_review_required"
        );
    }

    #[tokio::test]
    async fn a_plan_edit_after_a_spec_change_still_needs_review() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        billing_workstream(&pool).await;
        approve_spec_v2(&pool).await;
        let v2 = revise_plan(
            &pool,
            PlanEdit {
                plan_id: "plan-ledger".into(),
                based_on: 1,
                body: "Record the ledger, then the receipt.".into(),
                session_id: "session-1".into(),
                turn_id: "turn-11".into(),
                agent_id: None,
            },
        )
        .await
        .unwrap();
        select_plan(&pool, pick(v2, "turn-12")).await.unwrap();
        assert_eq!(
            compatibility(&pool, "plan-ledger").await,
            "compatibility_review_required"
        );
    }

    #[tokio::test]
    async fn selecting_a_plan_for_the_approved_spec_stays_current() {
        let (_dir, pool) = open().await;
        create_spec_with_plan(&pool, sample("Record the ledger."))
            .await
            .unwrap();
        billing_workstream(&pool).await;
        select_plan(&pool, pick(1, "turn-2")).await.unwrap();
        assert_eq!(compatibility(&pool, "plan-ledger").await, "current");
    }

    /// A pool on a WAL file with a short busy timeout, so a read that needs
    /// the write lock fails fast instead of waiting.
    async fn short_timeout_pool(path: &std::path::Path) -> SqlitePool {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_millis(50));
        sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(2)
            .connect_with(options)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn reads_do_not_need_the_write_lock_after_the_schema_exists() {
        use sqlx::Connection;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reads.db");
        let pool = short_timeout_pool(&path).await;
        ensure_hierarchy_schema(&pool).await.unwrap();
        // Warm every connection so none has to open while the lock is held.
        let warm: Vec<_> = vec![pool.acquire().await.unwrap(), pool.acquire().await.unwrap()];
        drop(warm);

        let mut writer = sqlx::SqliteConnection::connect_with(
            &sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&path)
                .busy_timeout(std::time::Duration::from_millis(50)),
        )
        .await
        .unwrap();
        sqlx::query("BEGIN IMMEDIATE")
            .execute(&mut writer)
            .await
            .unwrap();

        assert_eq!(hierarchy_enabled(&pool).await, Ok(false));
        assert_eq!(specs_for_session(&pool, "s").await, Ok(Vec::new()));
        assert_eq!(selected_plans(&pool, "w").await, Ok(Vec::new()));
        assert_eq!(import_review_ids(&pool).await, Ok(Vec::new()));

        sqlx::query("ROLLBACK").execute(&mut writer).await.unwrap();
    }

    async fn compatibility(pool: &SqlitePool, plan_id: &str) -> String {
        sqlx::query_scalar("SELECT compatibility FROM plan_selections WHERE plan_id = ?1")
            .bind(plan_id)
            .fetch_one(pool)
            .await
            .unwrap()
    }
}
