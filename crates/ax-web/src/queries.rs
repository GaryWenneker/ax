//! Raw SQLite queries for the ax-web HTTP server.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

// ---- Graph (force-directed viz) -------------------------------------------

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub file_path: String,
    pub community_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub community_label: Option<String>,
    pub degree: i64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shared: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub selected: bool,
}

#[derive(Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
}

#[derive(Serialize, Clone)]
pub struct GraphSystem {
    pub origin: String,
    pub node_count: i64,
}

#[derive(Serialize)]
pub struct GraphPayload {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub total_nodes: i64,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<GraphSystem>,
}

/// Build a force-directed graph payload: the top-`limit` nodes by degree
/// (excluding structural `contains` edges) plus the edges between them.
/// Community ids come from the persisted `node_communities` table.
/// The nodes in `ids` (in that order) and every semantic edge that touches one of them.
pub async fn get_graph_for_ids(pool: &SqlitePool, ids: &[String]) -> anyhow::Result<GraphPayload> {
    if ids.is_empty() {
        return Ok(GraphPayload {
            nodes: Vec::new(),
            edges: Vec::new(),
            total_nodes: 0,
            truncated: false,
            palette: None,
            systems: Vec::new(),
        });
    }
    // One JSON parameter instead of one per id: no SQLite variable limit.
    let ids_json = serde_json::to_string(ids)?;
    let query = sqlx::query_as::<_, (String, String, String, String, Option<i64>, Option<String>)>(
        "SELECT n.id, n.name, n.kind, n.file_path, nc.community_id, nc.community_label \
         FROM nodes n LEFT JOIN node_communities nc ON nc.node_id = n.id \
         WHERE n.id IN (SELECT value FROM json_each(?1))",
    )
    .bind(&ids_json);
    let mut node_rows = query.fetch_all(pool).await?;
    let order: std::collections::HashMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    node_rows.sort_by_key(|r| order.get(r.0.as_str()).copied().unwrap_or(usize::MAX));

    let nodes: Vec<GraphNode> = node_rows
        .into_iter()
        .map(
            |(id, name, kind, file_path, community_id, community_label)| GraphNode {
                id,
                name,
                kind,
                file_path,
                community_id: community_id.unwrap_or(-1),
                community_label,
                degree: 0,
                shared: false,
                selected: false,
            },
        )
        .collect();
    if nodes.is_empty() {
        return Ok(GraphPayload {
            nodes,
            edges: Vec::new(),
            total_nodes: 0,
            truncated: false,
            palette: None,
            systems: Vec::new(),
        });
    }

    let found = node_ids_json(&nodes)?;
    // Edges that touch a selected node: those leaving it, then those entering
    // it from outside the set (so an edge inside the set is listed once).
    let edge_rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
        "SELECT source, target, kind, confidence FROM edges \
         WHERE kind != 'contains' AND source IN (SELECT value FROM json_each(?1)) \
         UNION ALL \
         SELECT source, target, kind, confidence FROM edges \
         WHERE kind != 'contains' AND target IN (SELECT value FROM json_each(?1)) \
         AND source NOT IN (SELECT value FROM json_each(?1))",
    )
    .bind(&found)
    .fetch_all(pool)
    .await?;
    let edges: Vec<GraphEdge> = edge_rows
        .into_iter()
        .map(|(source, target, kind, confidence)| GraphEdge {
            source,
            target,
            kind,
            confidence,
        })
        .collect();
    let total_nodes = nodes.len() as i64;
    Ok(GraphPayload {
        nodes,
        edges,
        total_nodes,
        truncated: false,
        palette: None,
        systems: Vec::new(),
    })
}

/// The ids of `nodes` as one JSON array, for `json_each(?)`.
fn node_ids_json(nodes: &[GraphNode]) -> anyhow::Result<String> {
    let ids: Vec<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    Ok(serde_json::to_string(&ids)?)
}

/// The GLOB operator and its bound pattern for `origin`. Only the `'static`
/// operator is interpolated into SQL; dynamic filters elsewhere in this module add
/// literal clauses with `?` placeholders, so their `AssertSqlSafe` wraps are safe.
fn graph_scope(origin: &str) -> anyhow::Result<(&'static str, String)> {
    if origin.is_empty() {
        return Ok(("NOT GLOB", "ax-replica/*".to_string()));
    }
    if ax_db::replication::valid_id(origin) {
        return Ok(("GLOB", format!("ax-replica/{origin}/*")));
    }
    anyhow::bail!("invalid graph origin");
}

fn shown_path(path: &str, origin: &str) -> String {
    if origin.is_empty() {
        return path.to_string();
    }
    path.strip_prefix(&format!("ax-replica/{origin}/"))
        .unwrap_or(path)
        .to_string()
}

async fn graph_systems(pool: &SqlitePool) -> anyhow::Result<Vec<GraphSystem>> {
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT CASE
            WHEN id GLOB 'ax-replica/*' AND instr(substr(id, 12), '/') > 1
            THEN substr(id, 12, instr(substr(id, 12), '/') - 1)
            ELSE ''
         END, COUNT(*)
         FROM nodes GROUP BY 1 ORDER BY 1",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(origin, node_count)| GraphSystem { origin, node_count })
        .collect())
}

pub async fn get_graph(
    pool: &SqlitePool,
    limit: i64,
    origin: &str,
) -> anyhow::Result<GraphPayload> {
    let (op, pattern) = graph_scope(origin)?;
    let systems = graph_systems(pool).await?;
    let total_nodes: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM nodes WHERE id {op} ?"
    )))
    .bind(&pattern)
    .fetch_one(pool)
    .await?;

    let node_rows = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            Option<i64>,
            Option<String>,
            i64,
        ),
    >(sqlx::AssertSqlSafe(format!(
        r#"
        WITH deg AS (
            SELECT id, SUM(cnt) AS degree FROM (
                SELECT source AS id, COUNT(*) AS cnt FROM edges
                WHERE kind != 'contains' AND source {op} ? AND target {op} ?
                GROUP BY source
                UNION ALL
                SELECT target AS id, COUNT(*) AS cnt FROM edges
                WHERE kind != 'contains' AND source {op} ? AND target {op} ?
                GROUP BY target
            ) GROUP BY id
        )
        SELECT n.id, n.name, n.kind, n.file_path,
               nc.community_id, nc.community_label,
               COALESCE(deg.degree, 0) AS degree
        FROM nodes n
        LEFT JOIN node_communities nc ON nc.node_id = n.id
        LEFT JOIN deg ON deg.id = n.id
        WHERE n.id {op} ?
        ORDER BY degree DESC, n.name ASC
        LIMIT ?
        "#
    )))
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let nodes: Vec<GraphNode> = node_rows
        .into_iter()
        .map(
            |(id, name, kind, file_path, community_id, community_label, degree)| GraphNode {
                id,
                name,
                kind,
                file_path: shown_path(&file_path, origin),
                community_id: community_id.unwrap_or(-1),
                community_label,
                degree,
                shared: false,
                selected: false,
            },
        )
        .collect();

    let truncated = total_nodes > nodes.len() as i64;

    if nodes.is_empty() {
        return Ok(GraphPayload {
            nodes,
            edges: Vec::new(),
            total_nodes,
            truncated,
            palette: None,
            systems,
        });
    }

    // Only edges with both ends in the selected node set, filtered in SQL.
    // The edges of each selected source come from the source index; the unary
    // `+` keeps SQLite from probing every source × target pair instead.
    let selected = node_ids_json(&nodes)?;
    let edge_rows = sqlx::query_as::<_, (String, String, String, Option<String>)>(
        "SELECT source, target, kind, confidence FROM edges \
         WHERE kind != 'contains' \
         AND source IN (SELECT value FROM json_each(?1)) \
         AND +target IN (SELECT value FROM json_each(?1))",
    )
    .bind(&selected)
    .fetch_all(pool)
    .await?;

    let edges: Vec<GraphEdge> = edge_rows
        .into_iter()
        .map(|(source, target, kind, confidence)| GraphEdge {
            source,
            target,
            kind,
            confidence,
        })
        .collect();

    Ok(GraphPayload {
        nodes,
        edges,
        total_nodes,
        truncated,
        palette: None,
        systems,
    })
}

// ---- Stats ----------------------------------------------------------------

#[derive(Serialize)]
pub struct Stats {
    pub node_count: i64,
    pub edge_count: i64,
    pub file_count: i64,
    pub languages: Vec<LangStat>,
    pub last_indexed_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unresolved_ref_count: Option<i64>,
    pub systems: Vec<StatsSystem>,
}

#[derive(Serialize)]
pub struct StatsSystem {
    pub origin: String,
    pub node_count: i64,
    pub edge_count: i64,
    pub file_count: i64,
}

#[derive(Serialize)]
pub struct LangStat {
    pub language: String,
    pub count: i64,
}

pub async fn get_stats(pool: &SqlitePool) -> anyhow::Result<Stats> {
    let node_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM nodes WHERE id NOT GLOB 'ax-replica/*'")
            .fetch_one(pool)
            .await?;
    let edge_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM edges WHERE source NOT GLOB 'ax-replica/*' AND target NOT GLOB 'ax-replica/*'",
    )
    .fetch_one(pool)
    .await?;
    let file_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM files WHERE path NOT GLOB 'ax-replica/*'")
            .fetch_one(pool)
            .await?;

    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT language, COUNT(*) AS count FROM nodes WHERE id NOT GLOB 'ax-replica/*' GROUP BY language ORDER BY count DESC",
    )
    .fetch_all(pool)
    .await?;
    let languages = rows
        .into_iter()
        .map(|(language, count)| LangStat { language, count })
        .collect();

    let last_indexed_at: Option<i64> =
        sqlx::query_scalar("SELECT MAX(indexed_at) FROM files WHERE path NOT GLOB 'ax-replica/*'")
            .fetch_one(pool)
            .await?;
    let last_indexed_at = last_indexed_at.unwrap_or(0);

    let unresolved_ref_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM unresolved_refs WHERE from_node_id NOT GLOB 'ax-replica/*'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    let unresolved_ref_count = if unresolved_ref_count > 0 {
        Some(unresolved_ref_count)
    } else {
        None
    };

    let node_rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT CASE
            WHEN id GLOB 'ax-replica/*' AND instr(substr(id, 12), '/') > 1
            THEN substr(id, 12, instr(substr(id, 12), '/') - 1)
            ELSE ''
         END, COUNT(*) FROM nodes GROUP BY 1",
    )
    .fetch_all(pool)
    .await?;
    let edge_rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT CASE
            WHEN source GLOB 'ax-replica/*' AND instr(substr(source, 12), '/') > 1
            THEN substr(source, 12, instr(substr(source, 12), '/') - 1)
            ELSE ''
         END, COUNT(*) FROM edges GROUP BY 1",
    )
    .fetch_all(pool)
    .await?;
    let file_rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT CASE
            WHEN path GLOB 'ax-replica/*' AND instr(substr(path, 12), '/') > 1
            THEN substr(path, 12, instr(substr(path, 12), '/') - 1)
            ELSE ''
         END, COUNT(*) FROM files GROUP BY 1",
    )
    .fetch_all(pool)
    .await?;
    let mut systems: Vec<StatsSystem> = Vec::new();
    for (origin, node_count) in node_rows {
        if origin.is_empty() {
            continue;
        }
        systems.push(StatsSystem {
            origin,
            node_count,
            edge_count: 0,
            file_count: 0,
        });
    }
    for (origin, count) in edge_rows {
        if let Some(system) = systems.iter_mut().find(|s| s.origin == origin) {
            system.edge_count = count;
        }
    }
    for (origin, count) in file_rows {
        if let Some(system) = systems.iter_mut().find(|s| s.origin == origin) {
            system.file_count = count;
        }
    }

    Ok(Stats {
        node_count,
        edge_count,
        file_count,
        languages,
        last_indexed_at,
        unresolved_ref_count,
        systems,
    })
}

// ---- Nodes ----------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
pub struct NodeRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub qualified_name: String,
    pub file_path: String,
    pub language: String,
    pub start_line: i64,
    pub end_line: i64,
    pub signature: Option<String>,
    pub is_exported: i64,
}

pub struct NodeFilter<'a> {
    pub kind: Option<&'a str>,
    pub lang: Option<&'a str>,
    pub file: Option<&'a str>,
    pub q: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

pub struct NodePage {
    pub nodes: Vec<NodeRow>,
    pub total: i64,
}

pub async fn get_nodes(pool: &SqlitePool, f: NodeFilter<'_>) -> anyhow::Result<NodePage> {
    // Build dynamic WHERE clause components.
    let mut wheres: Vec<String> = Vec::new();
    if f.kind.is_some() {
        wheres.push("kind = ?".into());
    }
    if f.lang.is_some() {
        wheres.push("language = ?".into());
    }
    if f.file.is_some() {
        wheres.push("file_path = ?".into());
    }

    // Full-text query goes via FTS sub-select.
    let use_fts = f.q.filter(|s| !s.trim().is_empty()).is_some();

    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", wheres.join(" AND "))
    };

    if use_fts {
        let q_term = fts_prefix_term(f.q.unwrap_or_default());
        let count_sql = format!(
            r#"SELECT COUNT(*) FROM nodes_fts fts JOIN nodes n ON n.id = fts.id
               WHERE nodes_fts MATCH ? {}"#,
            if wheres.is_empty() {
                String::new()
            } else {
                format!("AND {}", wheres.join(" AND "))
            }
        );

        // We build queries without sqlx macros since the SQL is dynamic.
        let mut count_q =
            sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql)).bind(&q_term);
        if let Some(k) = f.kind {
            count_q = count_q.bind(k);
        }
        if let Some(l) = f.lang {
            count_q = count_q.bind(l);
        }
        if let Some(fp) = f.file {
            count_q = count_q.bind(fp);
        }
        let total = count_q.fetch_one(pool).await?;

        let fts_with_match = format!(
            r#"SELECT n.id, n.kind, n.name, n.qualified_name, n.file_path, n.language,
                      n.start_line, n.end_line, n.signature, n.is_exported
               FROM nodes_fts fts
               JOIN nodes n ON n.id = fts.id
               WHERE nodes_fts MATCH ? {}
               ORDER BY rank
               LIMIT ? OFFSET ?"#,
            if wheres.is_empty() {
                String::new()
            } else {
                format!("AND {}", wheres.join(" AND "))
            }
        );
        let mut rows_q = sqlx::query_as::<
            _,
            (
                String,
                String,
                String,
                String,
                String,
                String,
                i64,
                i64,
                Option<String>,
                i64,
            ),
        >(sqlx::AssertSqlSafe(fts_with_match))
        .bind(&q_term);
        if let Some(k) = f.kind {
            rows_q = rows_q.bind(k);
        }
        if let Some(l) = f.lang {
            rows_q = rows_q.bind(l);
        }
        if let Some(fp) = f.file {
            rows_q = rows_q.bind(fp);
        }
        rows_q = rows_q.bind(f.limit.max(1)).bind(f.offset.max(0));
        let rows = rows_q.fetch_all(pool).await?;
        let nodes = rows.into_iter().map(row_to_node).collect();
        return Ok(NodePage { nodes, total });
    }

    let list_sql = format!(
        r#"SELECT id, kind, name, qualified_name, file_path, language,
                  start_line, end_line, signature, is_exported
           FROM nodes {where_sql}
           ORDER BY start_line, lower(name)
           LIMIT ? OFFSET ?"#
    );
    let count_sql = format!("SELECT COUNT(*) FROM nodes {where_sql}");

    let mut count_q = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
    if let Some(k) = f.kind {
        count_q = count_q.bind(k);
    }
    if let Some(l) = f.lang {
        count_q = count_q.bind(l);
    }
    if let Some(fp) = f.file {
        count_q = count_q.bind(fp);
    }
    let total = count_q.fetch_one(pool).await?;

    let mut rows_q = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            i64,
            Option<String>,
            i64,
        ),
    >(sqlx::AssertSqlSafe(list_sql));
    if let Some(k) = f.kind {
        rows_q = rows_q.bind(k);
    }
    if let Some(l) = f.lang {
        rows_q = rows_q.bind(l);
    }
    if let Some(fp) = f.file {
        rows_q = rows_q.bind(fp);
    }
    rows_q = rows_q.bind(f.limit.max(1)).bind(f.offset.max(0));

    let rows = rows_q.fetch_all(pool).await?;
    let nodes = rows.into_iter().map(row_to_node).collect();
    Ok(NodePage { nodes, total })
}

fn row_to_node(
    r: (
        String,
        String,
        String,
        String,
        String,
        String,
        i64,
        i64,
        Option<String>,
        i64,
    ),
) -> NodeRow {
    NodeRow {
        id: r.0,
        kind: r.1,
        name: r.2,
        qualified_name: r.3,
        file_path: r.4,
        language: r.5,
        start_line: r.6,
        end_line: r.7,
        signature: r.8,
        is_exported: r.9,
    }
}

// ---- Node detail ----------------------------------------------------------

#[derive(Serialize)]
pub struct NodeDetail {
    pub node: NodeDetailRow,
    pub callers: Vec<EdgeNode>,
    pub callees: Vec<EdgeNode>,
}

#[derive(Serialize)]
pub struct NodeDetailRow {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub qualified_name: String,
    pub file_path: String,
    pub language: String,
    pub start_line: i64,
    pub end_line: i64,
    pub signature: Option<String>,
    pub docstring: Option<String>,
    pub visibility: Option<String>,
    pub is_exported: i64,
    pub is_async: i64,
}

#[derive(Serialize)]
pub struct EdgeNode {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub file_path: String,
    pub start_line: i64,
    pub edge_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_confidence: Option<String>,
}

pub async fn get_node_detail(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<NodeDetail>> {
    let row = sqlx::query_as::<
        _,
        (
            String,
            String,
            String,
            String,
            String,
            String,
            i64,
            i64,
            Option<String>,
            Option<String>,
            Option<String>,
            i64,
            i64,
        ),
    >(
        r#"SELECT id, kind, name, qualified_name, file_path, language,
                  start_line, end_line, signature, docstring, visibility, is_exported, is_async
           FROM nodes WHERE id = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else { return Ok(None) };

    let node = NodeDetailRow {
        id: r.0,
        kind: r.1,
        name: r.2,
        qualified_name: r.3,
        file_path: r.4,
        language: r.5,
        start_line: r.6,
        end_line: r.7,
        signature: r.8,
        docstring: r.9,
        visibility: r.10,
        is_exported: r.11,
        is_async: r.12,
    };

    let callers =
        sqlx::query_as::<_, (String, String, String, String, i64, String, Option<String>)>(
            r#"SELECT n.id, n.kind, n.name, n.file_path, n.start_line, e.kind, e.confidence
           FROM edges e JOIN nodes n ON n.id = e.source
           WHERE e.target = ?
           ORDER BY n.name LIMIT 50"#,
        )
        .bind(id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| EdgeNode {
            id: r.0,
            kind: r.1,
            name: r.2,
            file_path: r.3,
            start_line: r.4,
            edge_kind: r.5,
            edge_confidence: r.6,
        })
        .collect();

    let callees =
        sqlx::query_as::<_, (String, String, String, String, i64, String, Option<String>)>(
            r#"SELECT n.id, n.kind, n.name, n.file_path, n.start_line, e.kind, e.confidence
           FROM edges e JOIN nodes n ON n.id = e.target
           WHERE e.source = ?
           ORDER BY n.name LIMIT 50"#,
        )
        .bind(id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| EdgeNode {
            id: r.0,
            kind: r.1,
            name: r.2,
            file_path: r.3,
            start_line: r.4,
            edge_kind: r.5,
            edge_confidence: r.6,
        })
        .collect();

    Ok(Some(NodeDetail {
        node,
        callers,
        callees,
    }))
}

// ---- Files ----------------------------------------------------------------

#[derive(Serialize)]
pub struct FileRow {
    pub path: String,
    pub language: String,
    pub size: i64,
    pub node_count: i64,
    pub indexed_at: i64,
}

pub struct FileFilter<'a> {
    pub lang: Option<&'a str>,
    pub q: Option<&'a str>,
    /// Only paths under this folder (e.g. `MyRepo` or `MyRepo/src`).
    pub prefix: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Serialize)]
pub struct FileRoot {
    pub name: String,
    pub path: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct FileRootsPage {
    pub roots: Vec<FileRoot>,
    /// Indexed files at the project root (no `/` in path) — not folders.
    pub files: Vec<FileRow>,
}

pub struct FilePage {
    pub files: Vec<FileRow>,
    pub total: i64,
}

pub async fn get_file_roots(pool: &SqlitePool) -> anyhow::Result<FileRootsPage> {
    // Only first path segments that are directories (have children under them).
    // Root-level files like README.md must not appear as expandable folders.
    let folders = sqlx::query_as::<_, (String, i64)>(
        r#"SELECT
            substr(path, 1, instr(path, '/') - 1) AS name,
            COUNT(*)
           FROM files
           WHERE instr(path, '/') > 0
           GROUP BY 1
           ORDER BY 1 COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await?;

    let root_files = sqlx::query_as::<_, (String, String, i64, i64, i64)>(
        r#"SELECT path, language, size, node_count, indexed_at
           FROM files
           WHERE instr(path, '/') = 0
           ORDER BY path COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(FileRootsPage {
        roots: folders
            .into_iter()
            .map(|(name, count)| FileRoot {
                path: name.clone(),
                name,
                count,
            })
            .collect(),
        files: root_files
            .into_iter()
            .map(|(path, language, size, node_count, indexed_at)| FileRow {
                path,
                language,
                size,
                node_count,
                indexed_at,
            })
            .collect(),
    })
}

pub async fn get_files(pool: &SqlitePool, f: FileFilter<'_>) -> anyhow::Result<FilePage> {
    let mut wheres: Vec<String> = Vec::new();
    if f.lang.is_some() {
        wheres.push("language = ?".into());
    }
    if f.q.filter(|s| !s.is_empty()).is_some() {
        wheres.push("path LIKE ?".into());
    }
    if let Some(prefix) = f.prefix.filter(|s| !s.is_empty()) {
        wheres.push("(path = ? OR path LIKE ?)".into());
        let _ = prefix;
    }

    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", wheres.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM files {where_sql}");
    let list_sql = format!(
        "SELECT path, language, size, node_count, indexed_at FROM files {where_sql} ORDER BY path LIMIT ? OFFSET ?"
    );

    let mut count_q = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
    if let Some(l) = f.lang {
        count_q = count_q.bind(l);
    }
    if let Some(q) = f.q.filter(|s| !s.is_empty()) {
        count_q = count_q.bind(format!("%{q}%"));
    }
    if let Some(prefix) = f.prefix.filter(|s| !s.is_empty()) {
        count_q = count_q.bind(prefix).bind(format!("{prefix}/%"));
    }
    let total = count_q.fetch_one(pool).await?;

    let mut rows_q =
        sqlx::query_as::<_, (String, String, i64, i64, i64)>(sqlx::AssertSqlSafe(list_sql));
    if let Some(l) = f.lang {
        rows_q = rows_q.bind(l);
    }
    if let Some(q) = f.q.filter(|s| !s.is_empty()) {
        rows_q = rows_q.bind(format!("%{q}%"));
    }
    if let Some(prefix) = f.prefix.filter(|s| !s.is_empty()) {
        rows_q = rows_q.bind(prefix).bind(format!("{prefix}/%"));
    }
    rows_q = rows_q.bind(f.limit.max(1)).bind(f.offset.max(0));

    let rows = rows_q.fetch_all(pool).await?;
    let files = rows
        .into_iter()
        .map(|r| FileRow {
            path: r.0,
            language: r.1,
            size: r.2,
            node_count: r.3,
            indexed_at: r.4,
        })
        .collect();

    Ok(FilePage { files, total })
}

// ---- Search ---------------------------------------------------------------

#[derive(Serialize)]
pub struct SearchResult {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub qualified_name: String,
    pub file_path: String,
    pub start_line: i64,
    pub language: String,
    pub snippet: Option<String>,
}

/// FTS5 prefix query for raw user text: the text is one quoted phrase (inner
/// quotes doubled), so `(`, `-`, `:`, `*` and AND/OR/NOT are matched as text
/// instead of parsed as query syntax.
fn fts_prefix_term(q: &str) -> String {
    format!("\"{}\"*", q.trim().replace('"', "\"\""))
}

pub async fn search(pool: &SqlitePool, q: &str, limit: i64) -> anyhow::Result<Vec<SearchResult>> {
    if q.trim().is_empty() {
        return Ok(vec![]);
    }
    let term = fts_prefix_term(q);

    let rows = sqlx::query_as::<_, (String, String, String, String, String, i64, String, Option<String>)>(
        r#"SELECT n.id, n.kind, n.name, n.qualified_name, n.file_path, n.start_line, n.language, n.docstring
           FROM nodes_fts fts
           JOIN nodes n ON n.id = fts.id
           WHERE nodes_fts MATCH ?
           ORDER BY rank
           LIMIT ?"#,
    )
    .bind(&term)
    .bind(limit.max(1))
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| SearchResult {
            id: r.0,
            kind: r.1,
            name: r.2,
            qualified_name: r.3,
            file_path: r.4,
            start_line: r.5,
            language: r.6,
            snippet: r.7.map(|d| d.chars().take(120).collect()),
        })
        .collect())
}

// ---- Unresolved references ------------------------------------------------

#[derive(Serialize)]
pub struct UnresolvedRow {
    pub id: i64,
    pub from_node_id: String,
    pub reference_name: String,
    pub reference_kind: String,
    pub line: i64,
    pub col: i64,
    pub file_path: String,
    pub language: String,
}

pub struct UnresolvedFilter<'a> {
    pub q: Option<&'a str>,
    pub kind: Option<&'a str>,
    pub limit: i64,
    pub offset: i64,
}

pub struct UnresolvedPage {
    pub refs: Vec<UnresolvedRow>,
    pub total: i64,
}

#[derive(Serialize)]
pub struct UnresolvedKindStat {
    pub kind: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct UnresolvedNameStat {
    pub name: String,
    pub count: i64,
}

#[derive(Serialize)]
pub struct UnresolvedSummary {
    pub total: i64,
    pub by_kind: Vec<UnresolvedKindStat>,
    pub top_names: Vec<UnresolvedNameStat>,
}

pub async fn get_unresolved_summary(pool: &SqlitePool) -> anyhow::Result<UnresolvedSummary> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM unresolved_refs")
        .fetch_one(pool)
        .await?;

    let by_kind = sqlx::query_as::<_, (String, i64)>(
        "SELECT reference_kind, COUNT(*) FROM unresolved_refs GROUP BY reference_kind ORDER BY COUNT(*) DESC",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(kind, count)| UnresolvedKindStat { kind, count })
    .collect();

    let top_names = sqlx::query_as::<_, (String, i64)>(
        "SELECT reference_name, COUNT(*) AS cnt FROM unresolved_refs GROUP BY reference_name ORDER BY cnt DESC LIMIT 12",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(name, count)| UnresolvedNameStat { name, count })
    .collect();

    Ok(UnresolvedSummary {
        total,
        by_kind,
        top_names,
    })
}

pub async fn get_unresolved_refs(
    pool: &SqlitePool,
    f: UnresolvedFilter<'_>,
) -> anyhow::Result<UnresolvedPage> {
    let mut wheres: Vec<String> = Vec::new();
    if f.kind.is_some() {
        wheres.push("reference_kind = ?".into());
    }
    if f.q.filter(|s| !s.is_empty()).is_some() {
        wheres.push("(reference_name LIKE ? OR file_path LIKE ?)".into());
    }

    let where_sql = if wheres.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", wheres.join(" AND "))
    };

    let count_sql = format!("SELECT COUNT(*) FROM unresolved_refs {where_sql}");
    let list_sql = format!(
        "SELECT id, from_node_id, reference_name, reference_kind, line, col, file_path, language
         FROM unresolved_refs {where_sql}
         ORDER BY file_path, line, reference_name
         LIMIT ? OFFSET ?"
    );

    let mut count_q = sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(count_sql));
    if let Some(k) = f.kind {
        count_q = count_q.bind(k);
    }
    if let Some(q) = f.q.filter(|s| !s.is_empty()) {
        let pattern = format!("%{q}%");
        count_q = count_q.bind(pattern.clone()).bind(pattern);
    }
    let total = count_q.fetch_one(pool).await?;

    let mut rows_q = sqlx::query_as::<_, (i64, String, String, String, i64, i64, String, String)>(
        sqlx::AssertSqlSafe(list_sql),
    );
    if let Some(k) = f.kind {
        rows_q = rows_q.bind(k);
    }
    if let Some(q) = f.q.filter(|s| !s.is_empty()) {
        let pattern = format!("%{q}%");
        rows_q = rows_q.bind(pattern.clone()).bind(pattern);
    }
    rows_q = rows_q.bind(f.limit.max(1)).bind(f.offset.max(0));

    let rows = rows_q.fetch_all(pool).await?;
    let refs = rows
        .into_iter()
        .map(|r| UnresolvedRow {
            id: r.0,
            from_node_id: r.1,
            reference_name: r.2,
            reference_kind: r.3,
            line: r.4,
            col: r.5,
            file_path: r.6,
            language: r.7,
        })
        .collect();

    Ok(UnresolvedPage { refs, total })
}

#[cfg(test)]
mod fts_tests {
    use super::*;

    async fn pool_with_nodes(dir: &std::path::Path) -> ax_db::Database {
        let db = ax_db::Database::open(&dir.join("ax.db")).await.unwrap();
        for (id, name) in [("n1", "foo"), ("n2", "foobar"), ("n3", "other")] {
            sqlx::query(
                "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language,
                 start_line, end_line, start_column, end_column, updated_at)
                 VALUES (?, 'function', ?, ?, 'a.rs', 'rust', 1, 2, 0, 0, 0)",
            )
            .bind(id)
            .bind(name)
            .bind(name)
            .execute(db.pool())
            .await
            .unwrap();
        }
        db
    }

    fn filter(q: &str) -> NodeFilter<'_> {
        NodeFilter {
            kind: None,
            lang: None,
            file: None,
            q: Some(q),
            limit: 50,
            offset: 0,
        }
    }

    #[tokio::test]
    async fn search_treats_fts_syntax_as_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool_with_nodes(dir.path()).await;
        for q in [
            "foo(", "a-b", "C++", "\"x", "foo:", "NOT", "foo AND", "(", "*", "\"",
        ] {
            let res = search(db.pool(), q, 10).await;
            assert!(res.is_ok(), "search({q:?}) failed: {:?}", res.err());
            let page = get_nodes(db.pool(), filter(q)).await;
            assert!(page.is_ok(), "get_nodes({q:?}) failed: {:?}", page.err());
        }
        let hits = search(db.pool(), "foo(", 10).await.unwrap();
        let names: Vec<_> = hits.iter().map(|h| h.name.as_str()).collect();
        assert!(
            names.contains(&"foo") && names.contains(&"foobar"),
            "{names:?}"
        );
        let page = get_nodes(db.pool(), filter("foo")).await.unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(page.nodes.len(), 2);
    }

    #[tokio::test]
    async fn negative_limits_do_not_mean_unlimited() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool_with_nodes(dir.path()).await;
        assert_eq!(search(db.pool(), "foo", -1).await.unwrap().len(), 1);
        let mut f = filter("foo");
        f.limit = -1;
        f.offset = -3;
        assert_eq!(get_nodes(db.pool(), f).await.unwrap().nodes.len(), 1);
        let mut f = filter("");
        f.q = None;
        f.limit = -1;
        assert_eq!(get_nodes(db.pool(), f).await.unwrap().nodes.len(), 1);
    }

    #[tokio::test]
    async fn count_errors_are_returned() {
        let dir = tempfile::tempdir().unwrap();
        let db = pool_with_nodes(dir.path()).await;
        sqlx::query("DROP TABLE nodes_fts")
            .execute(db.pool())
            .await
            .unwrap();
        assert!(get_nodes(db.pool(), filter("foo")).await.is_err());
    }
}

#[cfg(test)]
mod graph_edge_tests {
    use super::*;

    /// Nodes a..e; edges a→b (calls), b→c (calls), a→c (contains), d→e (calls).
    async fn small_graph(dir: &std::path::Path) -> ax_db::Database {
        let db = ax_db::Database::open(&dir.join("ax.db")).await.unwrap();
        for id in ["a", "b", "c", "d", "e"] {
            insert_node(db.pool(), id).await;
        }
        for (s, t, k) in [
            ("a", "b", "calls"),
            ("b", "c", "calls"),
            ("a", "c", "contains"),
            ("d", "e", "calls"),
        ] {
            sqlx::query("INSERT INTO edges (source, target, kind) VALUES (?, ?, ?)")
                .bind(s)
                .bind(t)
                .bind(k)
                .execute(db.pool())
                .await
                .unwrap();
        }
        db
    }

    async fn insert_node(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO nodes (id, kind, name, qualified_name, file_path, language,
             start_line, end_line, start_column, end_column, updated_at)
             VALUES (?, 'function', ?, ?, 'a.rs', 'rust', 1, 2, 0, 0, 0)",
        )
        .bind(id)
        .bind(id)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    }

    fn pairs(payload: &GraphPayload) -> Vec<(String, String)> {
        let mut out: Vec<_> = payload
            .edges
            .iter()
            .map(|e| (e.source.clone(), e.target.clone()))
            .collect();
        out.sort();
        out
    }

    #[tokio::test]
    async fn the_graph_keeps_edges_between_its_nodes_only() {
        let dir = tempfile::tempdir().unwrap();
        let db = small_graph(dir.path()).await;
        let all = get_graph(db.pool(), 100, "").await.unwrap();
        assert_eq!(
            pairs(&all),
            vec![
                ("a".into(), "b".into()),
                ("b".into(), "c".into()),
                ("d".into(), "e".into())
            ]
        );
    }

    #[tokio::test]
    async fn recent_nodes_bring_every_non_contains_edge_that_touches_them() {
        let dir = tempfile::tempdir().unwrap();
        let db = small_graph(dir.path()).await;
        let ids = vec!["b".to_string()];
        let payload = get_graph_for_ids(db.pool(), &ids).await.unwrap();
        assert_eq!(payload.nodes.len(), 1);
        assert_eq!(
            pairs(&payload),
            vec![("a".into(), "b".into()), ("b".into(), "c".into())]
        );
    }

    #[tokio::test]
    async fn replicated_nodes_stay_in_their_own_graph() {
        let dir = tempfile::tempdir().unwrap();
        let db = small_graph(dir.path()).await;
        insert_node(db.pool(), "ax-replica/node1/r").await;
        let local = get_graph(db.pool(), 100, "").await.unwrap();
        assert!(local.nodes.iter().all(|n| !n.id.starts_with("ax-replica/")));
        let remote = get_graph(db.pool(), 100, "node1").await.unwrap();
        let ids: Vec<_> = remote.nodes.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(ids, vec!["ax-replica/node1/r"]);
        assert!(get_graph(db.pool(), 100, "../x").await.is_err());
    }

    #[tokio::test]
    async fn node_filters_and_limits_apply() {
        let dir = tempfile::tempdir().unwrap();
        let db = small_graph(dir.path()).await;
        let filter = NodeFilter {
            kind: Some("function"),
            lang: Some("rust"),
            file: Some("a.rs"),
            q: None,
            limit: 2,
            offset: 0,
        };
        let page = get_nodes(db.pool(), filter).await.unwrap();
        assert_eq!(page.total, 5);
        assert_eq!(page.nodes.len(), 2);
        let none = NodeFilter {
            kind: Some("class"),
            lang: None,
            file: None,
            q: None,
            limit: 10,
            offset: 0,
        };
        assert_eq!(get_nodes(db.pool(), none).await.unwrap().total, 0);
    }

    #[tokio::test]
    async fn any_number_of_ids_can_be_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let db = small_graph(dir.path()).await;
        let mut ids: Vec<String> = (0..40_000).map(|i| format!("missing-{i}")).collect();
        ids.push("d".into());
        let payload = get_graph_for_ids(db.pool(), &ids).await.unwrap();
        assert_eq!(payload.nodes.len(), 1);
        assert_eq!(pairs(&payload), vec![("d".into(), "e".into())]);
    }
}
