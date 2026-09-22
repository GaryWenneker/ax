//! Sample a force-layout slice from global.db (color = project).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::Result;
use serde::Serialize;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize)]
pub struct GlobalGraphNode {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub project_id: i64,
    pub project_name: String,
    pub shared: bool,
    pub selected: bool,
    pub degree: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalGraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GlobalGraphSlice {
    pub nodes: Vec<GlobalGraphNode>,
    pub edges: Vec<GlobalGraphEdge>,
    pub total_nodes: i64,
    pub truncated: bool,
}

pub async fn graph_slice(
    pool: &SqlitePool,
    limit: i64,
    selected_root: &Path,
) -> Result<GlobalGraphSlice> {
    let limit = limit.clamp(1, 3_000);
    let mut conn = pool.acquire().await?;

    let total_nodes: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM global_nodes")
        .fetch_one(&mut *conn)
        .await?;

    let projects: Vec<(i64, String, String)> =
        sqlx::query_as("SELECT id, name, path FROM projects ORDER BY id")
            .fetch_all(&mut *conn)
            .await?;
    if projects.is_empty() {
        return Ok(GlobalGraphSlice {
            nodes: vec![],
            edges: vec![],
            total_nodes,
            truncated: false,
        });
    }

    let selected = selected_root
        .canonicalize()
        .unwrap_or_else(|_| selected_root.to_path_buf());
    let per = (limit / projects.len() as i64).max(1);

    let shared: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT content_hash FROM shared_knowledge")
            .fetch_all(&mut *conn)
            .await?
            .into_iter()
            .collect();

    sqlx::query("DROP TABLE IF EXISTS temp.slice_nodes")
        .execute(&mut *conn)
        .await
        .ok();
    sqlx::query(
        "CREATE TEMP TABLE slice_nodes (
            gid TEXT PRIMARY KEY,
            project_id INTEGER NOT NULL,
            source_id TEXT NOT NULL
        )",
    )
    .execute(&mut *conn)
    .await?;

    let mut nodes = Vec::new();
    for (pid, pname, ppath) in &projects {
        let is_sel = Path::new(ppath)
            .canonicalize()
            .map(|p| p == selected)
            .unwrap_or(false);
        let rows: Vec<(i64, String, String, String, String, String)> = sqlx::query_as(
            "SELECT id, source_id, node_type, name, COALESCE(file_path, ''), content_hash
             FROM global_nodes WHERE project_id = ? ORDER BY id LIMIT ?",
        )
        .bind(pid)
        .bind(per)
        .fetch_all(&mut *conn)
        .await?;
        for (gid, source_id, kind, name, file_path, hash) in rows {
            let id = format!("g{gid}");
            sqlx::query("INSERT INTO slice_nodes (gid, project_id, source_id) VALUES (?, ?, ?)")
                .bind(&id)
                .bind(pid)
                .bind(&source_id)
                .execute(&mut *conn)
                .await?;
            nodes.push(GlobalGraphNode {
                id,
                name,
                kind,
                file_path,
                project_id: *pid,
                project_name: pname.clone(),
                shared: shared.contains(&hash),
                selected: is_sel,
                degree: 1,
            });
        }
    }

    let erows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT a.gid, b.gid, e.edge_type
         FROM global_edges e
         JOIN slice_nodes a ON a.project_id = e.project_id AND a.source_id = e.source_node
         JOIN slice_nodes b ON b.project_id = e.project_id AND b.source_id = e.target_node
         WHERE a.gid != b.gid",
    )
    .fetch_all(&mut *conn)
    .await?;

    let mut deg: HashMap<String, i64> = HashMap::new();
    let mut edges = Vec::with_capacity(erows.len());
    for (a, b, kind) in erows {
        *deg.entry(a.clone()).or_insert(0) += 1;
        *deg.entry(b.clone()).or_insert(0) += 1;
        edges.push(GlobalGraphEdge {
            source: a,
            target: b,
            kind,
        });
    }
    for n in &mut nodes {
        n.degree = *deg.get(&n.id).unwrap_or(&1);
    }

    let truncated = total_nodes > nodes.len() as i64;
    Ok(GlobalGraphSlice {
        nodes,
        edges,
        total_nodes,
        truncated,
    })
}
