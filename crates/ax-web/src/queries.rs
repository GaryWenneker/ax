//! Raw SQLite queries for the ax-web HTTP server.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

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
}

#[derive(Serialize)]
pub struct LangStat {
    pub language: String,
    pub count: i64,
}

pub async fn get_stats(pool: &SqlitePool) -> anyhow::Result<Stats> {
    let node_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nodes").fetch_one(pool).await?;
    let edge_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM edges").fetch_one(pool).await?;
    let file_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM files").fetch_one(pool).await?;

    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT language, COUNT(*) AS count FROM nodes GROUP BY language ORDER BY count DESC",
    )
    .fetch_all(pool)
    .await?;
    let languages = rows.into_iter().map(|(language, count)| LangStat { language, count }).collect();

    let last_indexed_at: Option<i64> =
        sqlx::query_scalar("SELECT MAX(indexed_at) FROM files").fetch_one(pool).await?;
    let last_indexed_at = last_indexed_at.unwrap_or(0);

    let unresolved_ref_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM unresolved_refs").fetch_one(pool).await.unwrap_or(0);
    let unresolved_ref_count = if unresolved_ref_count > 0 {
        Some(unresolved_ref_count)
    } else {
        None
    };

    Ok(Stats {
        node_count,
        edge_count,
        file_count,
        languages,
        last_indexed_at,
        unresolved_ref_count,
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
    if f.kind.is_some() { wheres.push("kind = ?".into()); }
    if f.lang.is_some() { wheres.push("language = ?".into()); }
    if f.file.is_some() { wheres.push("file_path = ?".into()); }

    // Full-text query goes via FTS sub-select.
    let use_fts = f.q.filter(|s| !s.trim().is_empty()).is_some();

    let where_sql = if wheres.is_empty() { String::new() } else { format!("WHERE {}", wheres.join(" AND ")) };

    if use_fts {
        let q_term = format!("{}*", f.q.unwrap().trim());
        let count_sql = format!(
            r#"SELECT COUNT(*) FROM nodes_fts fts JOIN nodes n ON n.id = fts.id
               WHERE nodes_fts MATCH ? {}"#,
            if wheres.is_empty() { String::new() } else { format!("AND {}", wheres.join(" AND ")) }
        );

        // We build queries without sqlx macros since the SQL is dynamic.
        let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql).bind(&q_term);
        if let Some(k) = f.kind { count_q = count_q.bind(k); }
        if let Some(l) = f.lang { count_q = count_q.bind(l); }
        if let Some(fp) = f.file { count_q = count_q.bind(fp); }
        let total = count_q.fetch_one(pool).await.unwrap_or(0);

        let fts_with_match = format!(
            r#"SELECT n.id, n.kind, n.name, n.qualified_name, n.file_path, n.language,
                      n.start_line, n.end_line, n.signature, n.is_exported
               FROM nodes_fts fts
               JOIN nodes n ON n.id = fts.id
               WHERE nodes_fts MATCH ? {}
               ORDER BY rank
               LIMIT ? OFFSET ?"#,
            if wheres.is_empty() { String::new() } else { format!("AND {}", wheres.join(" AND ")) }
        );
        let mut rows_q = sqlx::query_as::<_, (String, String, String, String, String, String, i64, i64, Option<String>, i64)>(&fts_with_match)
            .bind(&q_term);
        if let Some(k) = f.kind { rows_q = rows_q.bind(k); }
        if let Some(l) = f.lang { rows_q = rows_q.bind(l); }
        if let Some(fp) = f.file { rows_q = rows_q.bind(fp); }
        rows_q = rows_q.bind(f.limit).bind(f.offset);
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

    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    if let Some(k) = f.kind { count_q = count_q.bind(k); }
    if let Some(l) = f.lang { count_q = count_q.bind(l); }
    if let Some(fp) = f.file { count_q = count_q.bind(fp); }
    let total = count_q.fetch_one(pool).await.unwrap_or(0);

    let mut rows_q = sqlx::query_as::<_, (String, String, String, String, String, String, i64, i64, Option<String>, i64)>(&list_sql);
    if let Some(k) = f.kind { rows_q = rows_q.bind(k); }
    if let Some(l) = f.lang { rows_q = rows_q.bind(l); }
    if let Some(fp) = f.file { rows_q = rows_q.bind(fp); }
    rows_q = rows_q.bind(f.limit).bind(f.offset);

    let rows = rows_q.fetch_all(pool).await?;
    let nodes = rows.into_iter().map(row_to_node).collect();
    Ok(NodePage { nodes, total })
}

fn row_to_node(r: (String, String, String, String, String, String, i64, i64, Option<String>, i64)) -> NodeRow {
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
}

pub async fn get_node_detail(pool: &SqlitePool, id: &str) -> anyhow::Result<Option<NodeDetail>> {
    let row = sqlx::query_as::<_, (String, String, String, String, String, String, i64, i64, Option<String>, Option<String>, Option<String>, i64, i64)>(
        r#"SELECT id, kind, name, qualified_name, file_path, language,
                  start_line, end_line, signature, docstring, visibility, is_exported, is_async
           FROM nodes WHERE id = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    let Some(r) = row else { return Ok(None) };

    let node = NodeDetailRow {
        id: r.0, kind: r.1, name: r.2, qualified_name: r.3,
        file_path: r.4, language: r.5, start_line: r.6, end_line: r.7,
        signature: r.8, docstring: r.9, visibility: r.10,
        is_exported: r.11, is_async: r.12,
    };

    let callers = sqlx::query_as::<_, (String, String, String, String, i64, String)>(
        r#"SELECT n.id, n.kind, n.name, n.file_path, n.start_line, e.kind
           FROM edges e JOIN nodes n ON n.id = e.source
           WHERE e.target = ?
           ORDER BY n.name LIMIT 50"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| EdgeNode { id: r.0, kind: r.1, name: r.2, file_path: r.3, start_line: r.4, edge_kind: r.5 })
    .collect();

    let callees = sqlx::query_as::<_, (String, String, String, String, i64, String)>(
        r#"SELECT n.id, n.kind, n.name, n.file_path, n.start_line, e.kind
           FROM edges e JOIN nodes n ON n.id = e.target
           WHERE e.source = ?
           ORDER BY n.name LIMIT 50"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| EdgeNode { id: r.0, kind: r.1, name: r.2, file_path: r.3, start_line: r.4, edge_kind: r.5 })
    .collect();

    Ok(Some(NodeDetail { node, callers, callees }))
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

pub struct FilePage {
    pub files: Vec<FileRow>,
    pub total: i64,
}

pub async fn get_file_roots(pool: &SqlitePool) -> anyhow::Result<Vec<FileRoot>> {
    let rows = sqlx::query_as::<_, (String, i64)>(
        r#"SELECT
            CASE WHEN instr(path, '/') > 0 THEN substr(path, 1, instr(path, '/') - 1) ELSE path END,
            COUNT(*)
           FROM files
           GROUP BY 1
           ORDER BY 1 COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(name, count)| FileRoot {
            path: name.clone(),
            name,
            count,
        })
        .collect())
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

    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    if let Some(l) = f.lang {
        count_q = count_q.bind(l);
    }
    if let Some(q) = f.q.filter(|s| !s.is_empty()) {
        count_q = count_q.bind(format!("%{q}%"));
    }
    if let Some(prefix) = f.prefix.filter(|s| !s.is_empty()) {
        count_q = count_q.bind(prefix).bind(format!("{prefix}/%"));
    }
    let total = count_q.fetch_one(pool).await.unwrap_or(0);

    let mut rows_q = sqlx::query_as::<_, (String, String, i64, i64, i64)>(&list_sql);
    if let Some(l) = f.lang {
        rows_q = rows_q.bind(l);
    }
    if let Some(q) = f.q.filter(|s| !s.is_empty()) {
        rows_q = rows_q.bind(format!("%{q}%"));
    }
    if let Some(prefix) = f.prefix.filter(|s| !s.is_empty()) {
        rows_q = rows_q.bind(prefix).bind(format!("{prefix}/%"));
    }
    rows_q = rows_q.bind(f.limit).bind(f.offset);

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

pub async fn search(pool: &SqlitePool, q: &str, limit: i64) -> anyhow::Result<Vec<SearchResult>> {
    if q.trim().is_empty() { return Ok(vec![]); }
    let term = format!("{}*", q.trim());

    let rows = sqlx::query_as::<_, (String, String, String, String, String, i64, String, Option<String>)>(
        r#"SELECT n.id, n.kind, n.name, n.qualified_name, n.file_path, n.start_line, n.language, n.docstring
           FROM nodes_fts fts
           JOIN nodes n ON n.id = fts.id
           WHERE nodes_fts MATCH ?
           ORDER BY rank
           LIMIT ?"#,
    )
    .bind(&term)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| SearchResult {
        id: r.0, kind: r.1, name: r.2, qualified_name: r.3,
        file_path: r.4, start_line: r.5, language: r.6,
        snippet: r.7.map(|d| d.chars().take(120).collect()),
    }).collect())
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

pub async fn get_unresolved_refs(pool: &SqlitePool, f: UnresolvedFilter<'_>) -> anyhow::Result<UnresolvedPage> {
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

    let mut count_q = sqlx::query_scalar::<_, i64>(&count_sql);
    if let Some(k) = f.kind {
        count_q = count_q.bind(k);
    }
    if let Some(q) = f.q.filter(|s| !s.is_empty()) {
        let pattern = format!("%{q}%");
        count_q = count_q.bind(pattern.clone()).bind(pattern);
    }
    let total = count_q.fetch_one(pool).await.unwrap_or(0);

    let mut rows_q = sqlx::query_as::<_, (i64, String, String, String, i64, i64, String, String)>(&list_sql);
    if let Some(k) = f.kind {
        rows_q = rows_q.bind(k);
    }
    if let Some(q) = f.q.filter(|s| !s.is_empty()) {
        let pattern = format!("%{q}%");
        rows_q = rows_q.bind(pattern.clone()).bind(pattern);
    }
    rows_q = rows_q.bind(f.limit).bind(f.offset);

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
