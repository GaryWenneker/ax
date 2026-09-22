//! Copy a project's `.ax/ax.db` into the global database.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::open_pool;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub project_name: String,
    pub synced_count: usize,
    pub document_count: usize,
    pub edge_count: usize,
    pub duration_ms: u64,
}

pub fn project_db_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("ax.db")
}

pub async fn sync_project(global: &SqlitePool, project_root: &Path) -> Result<SyncResult> {
    let started = Instant::now();
    let db_path = project_db_path(project_root);
    if !db_path.is_file() {
        bail!("no project index at {} (expected .ax/ax.db)", db_path.display());
    }

    let name = project_root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("project")
        .to_string();
    let path_str = project_root
        .canonicalize()
        .unwrap_or_else(|_| project_root.to_path_buf())
        .to_string_lossy()
        .to_string();

    let project = open_pool(&db_path, false).await?;

    sqlx::query(
        "INSERT INTO projects (name, path, last_sync) VALUES (?, ?, CURRENT_TIMESTAMP)
         ON CONFLICT(path) DO UPDATE SET name = excluded.name, last_sync = CURRENT_TIMESTAMP",
    )
    .bind(&name)
    .bind(&path_str)
    .execute(global)
    .await?;

    let project_id: (i64,) = sqlx::query_as("SELECT id FROM projects WHERE path = ?")
        .bind(&path_str)
        .fetch_one(global)
        .await?;
    let pid = project_id.0;

    sqlx::query("DELETE FROM global_nodes WHERE project_id = ?")
        .bind(pid)
        .execute(global)
        .await?;
    sqlx::query("DELETE FROM global_documents WHERE project_id = ?")
        .bind(pid)
        .execute(global)
        .await?;
    sqlx::query("DELETE FROM global_edges WHERE project_id = ?")
        .bind(pid)
        .execute(global)
        .await?;
    sync_policy_table(
        global,
        &project,
        pid,
        "policy_rules",
        "id",
        "global_policy_rules",
    )
    .await?;
    sync_policy_table(
        global,
        &project,
        pid,
        "policy_skills",
        "name",
        "global_policy_skills",
    )
    .await?;

    let nodes: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
        "SELECT n.id, n.kind, n.name, n.file_path, f.content_hash
         FROM nodes n
         LEFT JOIN files f ON f.path = n.file_path",
    )
    .fetch_all(&project)
    .await
    .context("read nodes from project ax.db")?;

    for (source_id, kind, node_name, file_path, file_hash) in &nodes {
        let hash = file_hash
            .clone()
            .unwrap_or_else(|| format!("{kind}:{node_name}:{file_path}"));
        sqlx::query(
            "INSERT INTO global_nodes (project_id, source_id, node_type, name, file_path, content_hash)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(pid)
        .bind(source_id)
        .bind(kind)
        .bind(node_name)
        .bind(file_path)
        .bind(&hash)
        .execute(global)
        .await?;
    }

    let files: Vec<(String, String, String)> =
        sqlx::query_as("SELECT path, content_hash, language FROM files")
            .fetch_all(&project)
            .await
            .unwrap_or_default();

    for (file_path, content_hash, language) in &files {
        let doc_type = doc_type_from(file_path, language);
        sqlx::query(
            "INSERT INTO global_documents (project_id, file_path, doc_type, content_hash)
             VALUES (?, ?, ?, ?)",
        )
        .bind(pid)
        .bind(file_path)
        .bind(&doc_type)
        .bind(content_hash)
        .execute(global)
        .await?;
    }

    let edges: Vec<(String, String, String)> =
        sqlx::query_as("SELECT source, target, kind FROM edges")
            .fetch_all(&project)
            .await
            .unwrap_or_default();

    for (source, target, kind) in &edges {
        sqlx::query(
            "INSERT INTO global_edges (project_id, source_node, target_node, edge_type)
             VALUES (?, ?, ?, ?)",
        )
        .bind(pid)
        .bind(source)
        .bind(target)
        .bind(map_edge_kind(kind))
        .execute(global)
        .await?;
    }

    sqlx::query("UPDATE projects SET node_count = ?, doc_count = ? WHERE id = ?")
        .bind(nodes.len() as i64)
        .bind(files.len() as i64)
        .bind(pid)
        .execute(global)
        .await?;

    rebuild_shared_knowledge(global).await?;

    let duration_ms = started.elapsed().as_millis() as u64;
    sqlx::query(
        "INSERT INTO sync_log (source_project, target_type, record_count, duration_ms)
         VALUES (?, 'global', ?, ?)",
    )
    .bind(&name)
    .bind(nodes.len() as i64)
    .bind(duration_ms as i64)
    .execute(global)
    .await?;

    project.close().await;

    Ok(SyncResult {
        project_name: name,
        synced_count: nodes.len(),
        document_count: files.len(),
        edge_count: edges.len(),
        duration_ms,
    })
}

async fn table_exists(pool: &SqlitePool, name: &str) -> bool {
    let found: Option<(String,)> = sqlx::query_as(
        "SELECT name FROM sqlite_master WHERE type='table' AND name = ?",
    )
    .bind(name)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    found.is_some()
}

async fn sync_policy_table(
    global: &SqlitePool,
    project: &SqlitePool,
    pid: i64,
    source_table: &str,
    id_col: &str,
    dest_table: &str,
) -> Result<()> {
    if !table_exists(project, source_table).await {
        return Ok(());
    }
    let rows: Vec<sqlx::sqlite::SqliteRow> = sqlx::query(&format!("SELECT * FROM {source_table}"))
        .fetch_all(project)
        .await
        .unwrap_or_default();
    for row in rows {
        let item_id: String = row.try_get::<String, _>(id_col).unwrap_or_default();
        if item_id.is_empty() {
            continue;
        }
        let payload = row_to_policy_json(&row, id_col);
        sqlx::query(&format!(
            "INSERT INTO {dest_table} (project_id, item_id, payload) VALUES (?, ?, ?)
             ON CONFLICT(project_id, item_id) DO UPDATE SET payload = excluded.payload, synced_at = CURRENT_TIMESTAMP"
        ))
        .bind(pid)
        .bind(&item_id)
        .bind(payload.to_string())
        .execute(global)
        .await?;
    }
    Ok(())
}

fn row_to_policy_json(row: &sqlx::sqlite::SqliteRow, id_col: &str) -> serde_json::Value {
    use serde_json::{json, Map, Value};
    fn col(row: &sqlx::sqlite::SqliteRow, name: &str) -> Option<Value> {
        if let Ok(v) = row.try_get::<String, _>(name) {
            return Some(json!(v));
        }
        if let Ok(v) = row.try_get::<i64, _>(name) {
            return Some(json!(v));
        }
        if let Ok(v) = row.try_get::<Option<String>, _>(name) {
            return v.map(|s| json!(s));
        }
        None
    }
    fn json_list(raw: &Value) -> Value {
        let Some(s) = raw.as_str() else {
            return json!([]);
        };
        serde_json::from_str(s).unwrap_or_else(|_| json!([]))
    }
    let mut m = Map::new();
    let id = col(row, id_col).unwrap_or(json!(""));
    if id_col == "id" {
        m.insert("id".into(), id.clone());
    } else {
        m.insert("name".into(), id.clone());
    }
    if let Some(v) = col(row, "level") {
        m.insert("level".into(), v);
    }
    if let Some(v) = col(row, "description") {
        m.insert("description".into(), v);
    }
    if let Some(v) = col(row, "always_apply") {
        m.insert("alwaysApply".into(), json!(v.as_i64().unwrap_or(0) != 0));
    }
    if let Some(v) = col(row, "globs") {
        m.insert("globs".into(), json_list(&v));
    }
    if let Some(v) = col(row, "triggers") {
        m.insert("triggers".into(), json_list(&v));
    }
    if let Some(v) = col(row, "tags") {
        m.insert("tags".into(), json_list(&v));
    }
    if let Some(v) = col(row, "priority") {
        m.insert("priority".into(), v);
    }
    if let Some(v) = col(row, "body") {
        m.insert("body".into(), v);
    }
    if let Some(v) = col(row, "source_path") {
        m.insert("sourcePath".into(), v);
    }
    if let Some(v) = col(row, "enabled") {
        m.insert("enabled".into(), json!(v.as_i64().unwrap_or(1) != 0));
    }
    if let Some(v) = col(row, "status") {
        m.insert("status".into(), v);
    }
    if let Some(v) = col(row, "scope") {
        m.insert("scope".into(), v);
    }
    if let Some(v) = col(row, "context_task") {
        m.insert("contextTask".into(), v);
    }
    Value::Object(m)
}

fn doc_type_from(path: &str, language: &str) -> String {
    let lower = path.to_lowercase();
    if lower.ends_with(".md") || language.eq_ignore_ascii_case("markdown") {
        "markdown".into()
    } else if lower.ends_with(".json") {
        "json".into()
    } else if lower.ends_with(".xml") {
        "xml".into()
    } else if lower.ends_with(".pdf") {
        "pdf".into()
    } else {
        language.to_string()
    }
}

fn map_edge_kind(kind: &str) -> &'static str {
    let l = kind.to_lowercase();
    if l.contains("call") {
        "calls"
    } else if l.contains("contain") {
        "contains"
    } else {
        "references"
    }
}

async fn rebuild_shared_knowledge(global: &SqlitePool) -> Result<()> {
    sqlx::query("DELETE FROM shared_knowledge")
        .execute(global)
        .await?;
    sqlx::query("DELETE FROM cross_project_refs")
        .execute(global)
        .await?;

    let groups: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT d.content_hash,
                MIN(d.doc_type),
                MIN(d.file_path),
                json_group_array(DISTINCT p.name)
         FROM global_documents d
         JOIN projects p ON p.id = d.project_id
         GROUP BY d.content_hash
         HAVING COUNT(DISTINCT d.project_id) > 1",
    )
    .fetch_all(global)
    .await?;

    for (hash, doc_type, file_path, projects_json) in groups {
        let names: Vec<String> = serde_json::from_str(&projects_json).unwrap_or_default();
        let first = names.first().cloned().unwrap_or_default();
        sqlx::query(
            "INSERT INTO shared_knowledge (content_hash, node_type, name, projects, first_seen_project)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&hash)
        .bind(&doc_type)
        .bind(&file_path)
        .bind(&projects_json)
        .bind(&first)
        .execute(global)
        .await?;

        for i in 0..names.len() {
            for j in (i + 1)..names.len() {
                sqlx::query(
                    "INSERT OR IGNORE INTO cross_project_refs
                     (source_project, target_project, source_node_name, target_node_name, ref_type)
                     VALUES (?, ?, ?, ?, 'duplicate')",
                )
                .bind(&names[i])
                .bind(&names[j])
                .bind(&file_path)
                .bind(&file_path)
                .execute(global)
                .await?;
            }
        }
    }
    Ok(())
}
