//! List policy copies stored in global.db (other projects).

use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::{json, Value};
use sqlx::SqlitePool;

fn canon(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_path_buf())
}

/// Append other-project policy JSON objects onto `local` (already annotated).
pub async fn extend_with_foreign_policy(
    global: &SqlitePool,
    selected_root: &Path,
    kind: PolicyKind,
    local: &mut Vec<Value>,
) -> Result<()> {
    let selected = canon(selected_root);
    let table = match kind {
        PolicyKind::Rules => "global_policy_rules",
        PolicyKind::Skills => "global_policy_skills",
    };
    let rows: Vec<(i64, String, String, String, String)> = sqlx::query_as(&format!(
        "SELECT p.id, p.name, p.path, g.item_id, g.payload
         FROM {table} g
         JOIN projects p ON p.id = g.project_id"
    ))
    .fetch_all(global)
    .await
    .unwrap_or_default();

    let local_ids: std::collections::HashSet<String> = local
        .iter()
        .filter_map(|v| match kind {
            PolicyKind::Rules => v.get("id").and_then(|x| x.as_str()).map(|s| s.to_string()),
            PolicyKind::Skills => v.get("name").and_then(|x| x.as_str()).map(|s| s.to_string()),
        })
        .collect();

    for (pid, pname, ppath, item_id, payload) in rows {
        let home = canon(&PathBuf::from(&ppath)) == selected;
        if home && local_ids.contains(&item_id) {
            continue;
        }
        let mut v: Value = serde_json::from_str(&payload).unwrap_or_else(|_| json!({}));
        v = flatten_list_item(v, kind, &item_id);
        if !v.is_object() {
            continue;
        }
        let obj = v.as_object_mut().unwrap();
        obj.insert("origin".into(), json!("global"));
        obj.insert("projectName".into(), json!(pname));
        obj.insert("projectId".into(), json!(pid));
        obj.insert("selected".into(), json!(false));
        obj.insert("home".into(), json!(home));
        let prefix = match kind {
            PolicyKind::Rules => "gr",
            PolicyKind::Skills => "gs",
        };
        obj.insert("rowKey".into(), json!(format!("{prefix}:{pid}:{item_id}")));
        local.push(v);
    }
    Ok(())
}

pub async fn ensure_project(global: &SqlitePool, project_root: &Path) -> Result<i64> {
    let name = project_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string();
    let path_str = canon(project_root).to_string_lossy().to_string();
    sqlx::query(
        "INSERT INTO projects (name, path, last_sync) VALUES (?, ?, CURRENT_TIMESTAMP)
         ON CONFLICT(path) DO UPDATE SET name = excluded.name",
    )
    .bind(&name)
    .bind(&path_str)
    .execute(global)
    .await?;
    let row: (i64,) = sqlx::query_as("SELECT id FROM projects WHERE path = ?")
        .bind(&path_str)
        .fetch_one(global)
        .await?;
    Ok(row.0)
}

fn table_for(kind: PolicyKind) -> &'static str {
    match kind {
        PolicyKind::Rules => "global_policy_rules",
        PolicyKind::Skills => "global_policy_skills",
    }
}

pub async fn list_skill_payloads(global: &SqlitePool) -> Result<Vec<(String, Value)>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT item_id, payload FROM global_policy_skills WHERE level = 'global' ORDER BY item_id, project_id",
    )
    .fetch_all(global)
    .await
    .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (item_id, payload) in rows {
        if !seen.insert(item_id.clone()) {
            continue;
        }
        let value: Value = serde_json::from_str(&payload).unwrap_or_else(|_| json!({}));
        out.push((item_id.clone(), flatten_list_item(value, PolicyKind::Skills, &item_id)));
    }
    Ok(out)
}

pub async fn upsert_policy_item(
    global: &SqlitePool,
    project_id: i64,
    kind: PolicyKind,
    item_id: &str,
    payload: &Value,
) -> Result<()> {
    upsert_policy_item_from(global, project_id, kind, item_id, payload, "save")
        .await
        .map(|_| ())
}

/// Write one row and record a revision when its payload changed.
/// Returns the new version, or `None` when the payload was already stored.
pub async fn upsert_policy_item_from(
    global: &SqlitePool,
    project_id: i64,
    kind: PolicyKind,
    item_id: &str,
    payload: &Value,
    source: &str,
) -> Result<Option<i64>> {
    let table = table_for(kind);
    let text = payload.to_string();
    let mut tx = global.begin().await?;
    let current: Option<(String,)> =
        sqlx::query_as(&format!("SELECT payload FROM {table} WHERE project_id = ? AND item_id = ?"))
            .bind(project_id)
            .bind(item_id)
            .fetch_optional(&mut *tx)
            .await?;
    sqlx::query(&format!(
        "INSERT INTO {table} (project_id, item_id, payload) VALUES (?, ?, ?)
         ON CONFLICT(project_id, item_id) DO UPDATE SET payload = excluded.payload, synced_at = CURRENT_TIMESTAMP"
    ))
    .bind(project_id)
    .bind(item_id)
    .bind(&text)
    .execute(&mut *tx)
    .await?;
    let current = current.map(|c| c.0);
    let version = if current.as_deref() == Some(text.as_str()) {
        None
    } else {
        if let Some(old) = current.as_deref() {
            if max_version(&mut tx, kind, item_id).await?.is_none() {
                record_revision(&mut tx, kind, item_id, project_id, old, "baseline").await?;
            }
        }
        Some(record_revision(&mut tx, kind, item_id, project_id, &text, source).await?)
    };
    tx.commit().await?;
    Ok(version)
}

/// Revisions kept per item; versions keep counting past the pruned ones.
pub const GLOBAL_REVISION_CAP: i64 = 20;

async fn record_revision(
    conn: &mut sqlx::SqliteConnection,
    kind: PolicyKind,
    item_id: &str,
    project_id: i64,
    payload: &str,
    source: &str,
) -> Result<i64> {
    let (next,): (i64,) = sqlx::query_as(
        "SELECT COALESCE(MAX(version), 0) + 1 FROM global_policy_revisions WHERE kind = ? AND item_id = ?",
    )
    .bind(kind.revision_kind())
    .bind(item_id)
    .fetch_one(&mut *conn)
    .await?;
    sqlx::query(
        "INSERT INTO global_policy_revisions (kind, item_id, project_id, version, payload, source, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(kind.revision_kind())
    .bind(item_id)
    .bind(project_id)
    .bind(next)
    .bind(payload)
    .bind(source)
    .bind(now_ms())
    .execute(&mut *conn)
    .await?;
    sqlx::query("DELETE FROM global_policy_revisions WHERE kind = ? AND item_id = ? AND version <= ?")
        .bind(kind.revision_kind())
        .bind(item_id)
        .bind(next - GLOBAL_REVISION_CAP)
        .execute(&mut *conn)
        .await?;
    Ok(next)
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Newest first.
pub async fn list_revisions(global: &SqlitePool, kind: PolicyKind, item_id: &str) -> Result<Vec<GlobalRevision>> {
    let rows: Vec<(i64, i64, String, String, i64)> = sqlx::query_as(
        "SELECT version, project_id, payload, source, created_at FROM global_policy_revisions
         WHERE kind = ? AND item_id = ? ORDER BY version DESC",
    )
    .bind(kind.revision_kind())
    .bind(item_id)
    .fetch_all(global)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(version, project_id, payload, source, created_at)| GlobalRevision {
            version,
            project_id,
            payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
            source,
            created_at,
        })
        .collect())
}

/// The version the next revision of this item will get.
async fn max_version(conn: &mut sqlx::SqliteConnection, kind: PolicyKind, item_id: &str) -> Result<Option<i64>> {
    let (max,): (Option<i64>,) =
        sqlx::query_as("SELECT MAX(version) FROM global_policy_revisions WHERE kind = ? AND item_id = ?")
            .bind(kind.revision_kind())
            .bind(item_id)
            .fetch_one(&mut *conn)
            .await?;
    Ok(max)
}

/// The version the next changed write gets. A row stored before revisions existed first
/// becomes version 1 (`baseline`), so its next change is version 2.
pub async fn next_version(global: &SqlitePool, kind: PolicyKind, item_id: &str) -> Result<i64> {
    let mut conn = global.acquire().await?;
    if let Some(max) = max_version(&mut conn, kind, item_id).await? {
        return Ok(max + 1);
    }
    let table = table_for(kind);
    let stored: Option<(i64,)> = sqlx::query_as(&format!("SELECT 1 FROM {table} WHERE item_id = ? LIMIT 1"))
        .bind(item_id)
        .fetch_optional(&mut *conn)
        .await?;
    Ok(if stored.is_some() { 2 } else { 1 })
}

/// `mirror` rows whose name exists at the global level, as `(project_id, item_id)`.
/// They are deleted when `delete` is set.
pub async fn shadowed_mirrors(global: &SqlitePool, kind: PolicyKind, delete: bool) -> Result<Vec<(i64, String)>> {
    let table = table_for(kind);
    let shadowed: Vec<(i64, String)> = sqlx::query_as(&format!(
        "SELECT project_id, item_id FROM {table} WHERE level = 'mirror'
         AND item_id IN (SELECT item_id FROM {table} WHERE level = 'global') ORDER BY item_id, project_id"
    ))
    .fetch_all(global)
    .await?;
    if delete {
        for (pid, item_id) in &shadowed {
            sqlx::query(&format!("DELETE FROM {table} WHERE project_id = ? AND item_id = ? AND level = 'mirror'"))
                .bind(pid)
                .bind(item_id)
                .execute(global)
                .await?;
        }
    }
    Ok(shadowed)
}

/// Store one machine-wide skill in `~/.ax/global.db` (`global_policy_skills`).
/// The row is keyed to `~/.ax` so it is not tied to a single project.
pub async fn upsert_machine_skill(item_id: &str, payload: &Value) -> Result<()> {
    let db_path = crate::global_db_path()?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pool = crate::open_and_init(&db_path).await?;
    let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("HOME not set"))?;
    let project_id = ensure_project(&pool, &home.join(".ax")).await?;
    upsert_policy_item(&pool, project_id, PolicyKind::Skills, item_id, payload).await?;
    pool.close().await;
    Ok(())
}

pub async fn load_policy_item(
    global: &SqlitePool,
    project_id: i64,
    kind: PolicyKind,
    item_id: &str,
) -> Result<Option<Value>> {
    let table = table_for(kind);
    let row: Option<(String,)> = sqlx::query_as(&format!(
        "SELECT payload FROM {table} WHERE project_id = ? AND item_id = ?"
    ))
    .bind(project_id)
    .bind(item_id)
    .fetch_optional(global)
    .await?;
    Ok(row.and_then(|p| serde_json::from_str(&p.0).ok()))
}

pub async fn delete_policy_item(
    global: &SqlitePool,
    project_id: i64,
    kind: PolicyKind,
    item_id: &str,
) -> Result<bool> {
    delete_policy_item_from(global, project_id, kind, item_id, "delete").await
}

/// Delete one row, recording its last payload as a revision first.
pub async fn delete_policy_item_from(
    global: &SqlitePool,
    project_id: i64,
    kind: PolicyKind,
    item_id: &str,
    source: &str,
) -> Result<bool> {
    let table = table_for(kind);
    let mut tx = global.begin().await?;
    let current: Option<(String,)> =
        sqlx::query_as(&format!("SELECT payload FROM {table} WHERE project_id = ? AND item_id = ?"))
            .bind(project_id)
            .bind(item_id)
            .fetch_optional(&mut *tx)
            .await?;
    let Some((payload,)) = current else {
        return Ok(false);
    };
    record_revision(&mut tx, kind, item_id, project_id, &payload, source).await?;
    sqlx::query(&format!("DELETE FROM {table} WHERE project_id = ? AND item_id = ?"))
        .bind(project_id)
        .bind(item_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn resolve_project_id(
    global: &SqlitePool,
    project_root: &Path,
    explicit: Option<i64>,
) -> Result<i64> {
    if let Some(id) = explicit {
        return Ok(id);
    }
    ensure_project(global, project_root).await
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct GlobalRevision {
    pub version: i64,
    pub project_id: i64,
    pub payload: Value,
    pub source: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyKind {
    Rules,
    Skills,
}

impl PolicyKind {
    /// The `kind` value in project `policy_revisions` and `global_policy_revisions`.
    pub fn revision_kind(self) -> &'static str {
        match self {
            PolicyKind::Rules => "rule",
            PolicyKind::Skills => "skill",
        }
    }
}

/// Nested `PolicyRuleDoc` / `PolicySkillDoc` JSON (from relocate) must look like a list row.
pub fn flatten_list_item(mut v: Value, kind: PolicyKind, item_id: &str) -> Value {
    if let Some(fm) = v.get("frontmatter").cloned() {
        if let (Some(obj), Some(fm_obj)) = (v.as_object_mut(), fm.as_object()) {
            for (k, val) in fm_obj {
                let empty = match obj.get(k) {
                    None => true,
                    Some(Value::Null) => true,
                    Some(Value::String(s)) if s.is_empty() => true,
                    _ => false,
                };
                if empty {
                    obj.insert(k.clone(), val.clone());
                }
            }
        }
    }
    let Some(obj) = v.as_object_mut() else {
        return v;
    };
    match kind {
        PolicyKind::Rules => {
            let missing = obj.get("id").and_then(|x| x.as_str()).unwrap_or("").is_empty();
            if missing {
                obj.insert("id".into(), json!(item_id));
            }
        }
        PolicyKind::Skills => {
            let missing = obj.get("name").and_then(|x| x.as_str()).unwrap_or("").is_empty();
            if missing {
                obj.insert("name".into(), json!(item_id));
            }
        }
    }
    // Always prefer a non-empty item_id over a still-missing list key.
    match kind {
        PolicyKind::Rules => {
            if obj.get("id").and_then(|x| x.as_str()).unwrap_or("").is_empty() && !item_id.is_empty()
            {
                obj.insert("id".into(), json!(item_id));
            }
        }
        PolicyKind::Skills => {
            if obj.get("name").and_then(|x| x.as_str()).unwrap_or("").is_empty() && !item_id.is_empty()
            {
                obj.insert("name".into(), json!(item_id));
            }
        }
    }
    for key in ["tags", "globs", "triggers"] {
        if !obj.get(key).map(|x| x.is_array()).unwrap_or(false) {
            obj.insert(key.into(), json!([]));
        }
    }
    obj.entry("level").or_insert(json!("INFO"));
    obj.entry("description").or_insert(json!(""));
    obj.entry("body").or_insert(json!(""));
    obj.entry("priority").or_insert(json!(50));
    obj.entry("enabled").or_insert(json!(true));
    obj.entry("scope").or_insert(json!("project"));
    v
}

pub fn annotate_local(mut item: Value, project_name: &str, kind: PolicyKind) -> Value {
    let key = match kind {
        PolicyKind::Rules => item
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        PolicyKind::Skills => item
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
    };
    if let Some(obj) = item.as_object_mut() {
        obj.entry("origin".to_string())
            .or_insert_with(|| json!("project"));
        obj.insert("projectName".into(), json!(project_name));
        obj.insert("selected".into(), json!(true));
        let prefix = match kind {
            PolicyKind::Rules => "pr",
            PolicyKind::Skills => "ps",
        };
        obj.insert("rowKey".into(), json!(format!("{prefix}:{key}")));
    }
    item
}
