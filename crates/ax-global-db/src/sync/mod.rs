//! Copy a project's `.ax/ax.db` into the global database.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqliteConnection, SqlitePool};

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
        bail!(
            "no project index at {} (expected .ax/ax.db)",
            db_path.display()
        );
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
    let read = read_project(&project).await;
    project.close().await;
    let ProjectRows {
        nodes,
        files,
        edges,
        rules,
        skills,
    } = read?;

    // One transaction for the whole replace: a failure halfway (busy
    // timeout, disk full, interrupt) keeps the previous rows, readers never
    // see a half-built project, and the sync commits once instead of per row.
    // The first statement writes, so the write lock is taken up front.
    let mut tx = global.begin().await?;
    sqlx::query(
        "INSERT INTO projects (name, path, last_sync) VALUES (?, ?, CURRENT_TIMESTAMP)
         ON CONFLICT(path) DO UPDATE SET name = excluded.name, last_sync = CURRENT_TIMESTAMP",
    )
    .bind(&name)
    .bind(&path_str)
    .execute(&mut *tx)
    .await?;

    let project_id: (i64,) = sqlx::query_as("SELECT id FROM projects WHERE path = ?")
        .bind(&path_str)
        .fetch_one(&mut *tx)
        .await?;
    let pid = project_id.0;

    sqlx::query("DELETE FROM global_nodes WHERE project_id = ?")
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM global_documents WHERE project_id = ?")
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM global_edges WHERE project_id = ?")
        .bind(pid)
        .execute(&mut *tx)
        .await?;
    if let Some(rows) = &rules {
        sync_policy_table(&mut tx, pid, rows, "id", "global_policy_rules").await?;
    }
    if let Some(rows) = &skills {
        sync_policy_table(&mut tx, pid, rows, "name", "global_policy_skills").await?;
    }

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
        .execute(&mut *tx)
        .await?;
    }

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
        .execute(&mut *tx)
        .await?;
    }

    for (source, target, kind) in &edges {
        sqlx::query(
            "INSERT INTO global_edges (project_id, source_node, target_node, edge_type)
             VALUES (?, ?, ?, ?)",
        )
        .bind(pid)
        .bind(source)
        .bind(target)
        .bind(map_edge_kind(kind))
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("UPDATE projects SET node_count = ?, doc_count = ? WHERE id = ?")
        .bind(nodes.len() as i64)
        .bind(files.len() as i64)
        .bind(pid)
        .execute(&mut *tx)
        .await?;

    rebuild_shared_knowledge(&mut tx).await?;

    let duration_ms = started.elapsed().as_millis() as u64;
    sqlx::query(
        "INSERT INTO sync_log (source_project, target_type, record_count, duration_ms)
         VALUES (?, 'global', ?, ?)",
    )
    .bind(&name)
    .bind(nodes.len() as i64)
    .bind(duration_ms as i64)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(SyncResult {
        project_name: name,
        synced_count: nodes.len(),
        document_count: files.len(),
        edge_count: edges.len(),
        duration_ms,
    })
}

type NodeRow = (String, String, String, String, Option<String>);

/// Everything a sync copies out of the project's `ax.db`, read before the
/// global write transaction starts so that transaction stays short.
struct ProjectRows {
    nodes: Vec<NodeRow>,
    files: Vec<(String, String, String)>,
    edges: Vec<(String, String, String)>,
    rules: Option<Vec<sqlx::sqlite::SqliteRow>>,
    skills: Option<Vec<sqlx::sqlite::SqliteRow>>,
}

async fn read_project(project: &SqlitePool) -> Result<ProjectRows> {
    let nodes: Vec<NodeRow> = sqlx::query_as(
        "SELECT n.id, n.kind, n.name, n.file_path, f.content_hash
         FROM nodes n
         LEFT JOIN files f ON f.path = n.file_path",
    )
    .fetch_all(project)
    .await
    .context("read nodes from project ax.db")?;
    let files = sqlx::query_as("SELECT path, content_hash, language FROM files")
        .fetch_all(project)
        .await
        .unwrap_or_default();
    let edges = sqlx::query_as("SELECT source, target, kind FROM edges")
        .fetch_all(project)
        .await
        .unwrap_or_default();
    Ok(ProjectRows {
        nodes,
        files,
        edges,
        rules: policy_rows(project, "policy_rules").await,
        skills: policy_rows(project, "policy_skills").await,
    })
}

async fn policy_rows(
    project: &SqlitePool,
    table: &'static str,
) -> Option<Vec<sqlx::sqlite::SqliteRow>> {
    if !table_exists(project, table).await {
        return None;
    }
    // Safe: `table` is a `'static` literal from `project_rows`.
    Some(
        sqlx::query(sqlx::AssertSqlSafe(format!("SELECT * FROM {table}")))
            .fetch_all(project)
            .await
            .unwrap_or_default(),
    )
}

async fn table_exists(pool: &SqlitePool, name: &str) -> bool {
    let found: Option<(String,)> =
        sqlx::query_as("SELECT name FROM sqlite_master WHERE type='table' AND name = ?")
            .bind(name)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    found.is_some()
}

async fn sync_policy_table(
    global: &mut SqliteConnection,
    pid: i64,
    rows: &[sqlx::sqlite::SqliteRow],
    id_col: &str,
    dest_table: &'static str,
) -> Result<()> {
    // Safe (all queries below): `dest_table` is a `'static` literal from `sync_project`.
    for row in rows {
        let item_id: String = row.try_get::<String, _>(id_col).unwrap_or_default();
        if item_id.is_empty() {
            continue;
        }
        let at_global_level: Option<(i64,)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT 1 FROM {dest_table} WHERE item_id = ? AND level = 'global' LIMIT 1"
        )))
        .bind(&item_id)
        .fetch_optional(&mut *global)
        .await?;
        if at_global_level.is_some() {
            continue;
        }
        let payload = row_to_policy_json(row, id_col);
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "INSERT INTO {dest_table} (project_id, item_id, payload, level) VALUES (?, ?, ?, 'mirror')
             ON CONFLICT(project_id, item_id) DO UPDATE SET payload = excluded.payload, synced_at = CURRENT_TIMESTAMP
             WHERE level = 'mirror'"
        )))
        .bind(pid)
        .bind(&item_id)
        .bind(payload.to_string())
        .execute(&mut *global)
        .await?;
    }
    Ok(())
}

/// A project `policy_rules` / `policy_skills` row as the JSON payload stored in global.db.
pub fn row_to_policy_json(row: &sqlx::sqlite::SqliteRow, id_col: &str) -> serde_json::Value {
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
    if let Some(v) = col(row, "properties") {
        let parsed = v
            .as_str()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .unwrap_or_else(|| json!({}));
        if parsed.as_object().is_some_and(|o| !o.is_empty()) {
            m.insert("properties".into(), parsed);
        }
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

async fn rebuild_shared_knowledge(global: &mut SqliteConnection) -> Result<()> {
    sqlx::query("DELETE FROM shared_knowledge")
        .execute(&mut *global)
        .await?;
    sqlx::query("DELETE FROM cross_project_refs")
        .execute(&mut *global)
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
    .fetch_all(&mut *global)
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
        .execute(&mut *global)
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
                .execute(&mut *global)
                .await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_and_init;

    async fn project_index(root: &Path, nodes: &[&str]) {
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        let db = open_pool(&project_db_path(root), true).await.unwrap();
        for ddl in [
            "CREATE TABLE IF NOT EXISTS nodes (id TEXT, kind TEXT, name TEXT, file_path TEXT)",
            "CREATE TABLE IF NOT EXISTS files (path TEXT, content_hash TEXT, language TEXT)",
            "CREATE TABLE IF NOT EXISTS edges (source TEXT, target TEXT, kind TEXT)",
            "DELETE FROM nodes",
            "DELETE FROM files",
            "DELETE FROM edges",
        ] {
            sqlx::query(ddl).execute(&db).await.unwrap();
        }
        for id in nodes {
            sqlx::query("INSERT INTO nodes VALUES (?, 'function', ?, 'src/a.rs')")
                .bind(id)
                .bind(id)
                .execute(&db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO files VALUES ('src/a.rs', 'h1', 'rust')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO edges VALUES ('a', 'b', 'calls')")
            .execute(&db)
            .await
            .unwrap();
        db.close().await;
    }

    async fn count(pool: &SqlitePool, table: &str) -> i64 {
        let row: (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
            .fetch_one(pool)
            .await
            .unwrap();
        row.0
    }

    #[tokio::test]
    async fn a_failed_sync_keeps_the_previous_global_rows() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("proj");
        project_index(&root, &["a", "b"]).await;
        let global = open_and_init(&dir.path().join("global.db")).await.unwrap();
        let first = sync_project(&global, &root).await.unwrap();
        assert_eq!(first.synced_count, 2);
        assert_eq!(first.edge_count, 1);

        project_index(&root, &["a", "b", "c"]).await;
        // Fail the sync after the nodes are written, while writing edges.
        sqlx::query(
            "CREATE TRIGGER fail_edges BEFORE INSERT ON global_edges
             BEGIN SELECT RAISE(ABORT, 'disk full'); END",
        )
        .execute(&global)
        .await
        .unwrap();
        assert!(sync_project(&global, &root).await.is_err());

        assert_eq!(count(&global, "global_nodes").await, 2);
        assert_eq!(count(&global, "global_documents").await, 1);
        assert_eq!(count(&global, "global_edges").await, 1);

        sqlx::query("DROP TRIGGER fail_edges")
            .execute(&global)
            .await
            .unwrap();
        let second = sync_project(&global, &root).await.unwrap();
        assert_eq!(second.synced_count, 3);
        assert_eq!(count(&global, "global_nodes").await, 3);
        assert_eq!(count(&global, "global_edges").await, 1);
    }
}
