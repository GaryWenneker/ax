//! ax-web: local HTTP server exposing the ax code graph + policy editor.

mod remote_host;
pub mod replication_setup;
pub mod server_config;
pub mod server_discovery;
mod server_network;

mod access_api;
mod actions;
mod agent;
mod agent_pty;
mod browser_origin;
mod changes;
mod context_authority_api;
mod context_budget_api;
mod dav;
mod docs_catalog;
mod domain_graph;
mod folder_picker;
mod global_policy;
mod graph_export;
mod links_api;
mod local_mcp_export;
mod local_owner;
mod lsp_api;
mod mcp_ops;
mod mcp_quality;
mod mcp_trace;
mod memory;
mod neural;
mod node_tracker;
mod okf_api;
mod plugins_api;
mod policy;
mod policy_share;
mod pricing_api;
mod project_purge;
mod queries;
mod remote_mcp;
mod remote_mcp_publications;
mod request_guard;
mod savings;
mod share_api;
mod share_auth;
mod ship;
mod skill_catalog_api;
mod sonar_proxy;
mod specs;
mod vault_folders;
mod web_auth;
mod workspace;
mod workspace_state;

pub use actions::{publish as publish_action, ActionEvent};
pub use workspace_state::WebHub;

use std::path::PathBuf;

use ax_db::queries::QueryBuilder;
use ax_resolution::{prune_stale_unresolved_refs, ReferenceResolver};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{Response, StatusCode, Uri},
    response::{IntoResponse, Json},
};
use include_dir::Dir;
use serde::{Deserialize, Serialize};

include!(concat!(env!("OUT_DIR"), "/web_dist.rs"));
const _: &str = env!("AX_WEB_DIST_STAMP");
const WEB_DIST_DISK_HINT: &str = "disk-first-20260916b";

#[derive(Serialize)]
struct WebStats {
    #[serde(flatten)]
    graph: queries::Stats,
    db_size_bytes: i64,
    policy_rules_count: i64,
    policy_skills_count: i64,
    readonly: bool,
    project_name: String,
}

#[derive(Serialize)]
pub(crate) struct ApiError {
    pub error: String,
}

fn api_err(msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError { error: msg.into() }),
    )
}

async fn handle_stats(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match queries::get_stats(&ws.graph_pool).await {
        Ok(graph) => {
            let db_size_bytes = std::fs::metadata(&ws.db_path)
                .map(|m| m.len() as i64)
                .unwrap_or(0);
            let policy_rules_count = ws
                .policy
                .store
                .list_rules()
                .await
                .map(|r| r.len() as i64)
                .unwrap_or(0);
            let policy_skills_count = ws
                .policy
                .store
                .list_skills()
                .await
                .map(|sk| sk.len() as i64)
                .unwrap_or(0);
            let project_name = ws
                .project_root
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project")
                .to_string();
            let body = WebStats {
                graph,
                db_size_bytes,
                policy_rules_count,
                policy_skills_count,
                readonly: hub.readonly,
                project_name,
            };
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct NodesQuery {
    kind: Option<String>,
    lang: Option<String>,
    file: Option<String>,
    q: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

fn files_max_limit() -> i64 {
    10_000
}

fn nodes_max_limit() -> i64 {
    2_000
}

/// Page size from a query string, kept in `1..=max`. SQLite reads a negative
/// `LIMIT` as "no limit", so the lower bound matters as much as the upper.
fn page_limit(limit: i64, max: i64) -> i64 {
    limit.clamp(1, max)
}

/// Row offset from a query string; negative values start at the first row.
fn page_offset(offset: i64) -> i64 {
    offset.max(0)
}

async fn handle_nodes(State(hub): State<WebHub>, Query(p): Query<NodesQuery>) -> impl IntoResponse {
    let ws = hub.read().await;
    let filter = queries::NodeFilter {
        kind: p.kind.as_deref(),
        lang: p.lang.as_deref(),
        file: p.file.as_deref(),
        q: p.q.as_deref(),
        limit: page_limit(p.limit, nodes_max_limit()),
        offset: page_offset(p.offset),
    };
    match queries::get_nodes(&ws.graph_pool, filter).await {
        Ok(page) => (
            StatusCode::OK,
            Json(serde_json::json!({ "nodes": page.nodes, "total": page.total })),
        )
            .into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

async fn handle_node(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    let ws = hub.read().await;
    match queries::get_node_detail(&ws.graph_pool, &id).await {
        Ok(Some(detail)) => (StatusCode::OK, Json(detail)).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Not found".into(),
            }),
        )
            .into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct FilesQuery {
    lang: Option<String>,
    q: Option<String>,
    prefix: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

async fn handle_files(State(hub): State<WebHub>, Query(p): Query<FilesQuery>) -> impl IntoResponse {
    let ws = hub.read().await;
    let filter = queries::FileFilter {
        lang: p.lang.as_deref(),
        q: p.q.as_deref(),
        prefix: p.prefix.as_deref(),
        limit: page_limit(p.limit, files_max_limit()),
        offset: page_offset(p.offset),
    };
    match queries::get_files(&ws.graph_pool, filter).await {
        Ok(page) => (
            StatusCode::OK,
            Json(serde_json::json!({ "files": page.files, "total": page.total })),
        )
            .into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

async fn handle_file_roots(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match queries::get_file_roots(&ws.graph_pool).await {
        Ok(page) => (StatusCode::OK, Json(serde_json::json!(page))).into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct SearchQuery {
    q: Option<String>,
    #[serde(default = "default_search_limit")]
    limit: i64,
}

fn default_search_limit() -> i64 {
    20
}

async fn handle_search(
    State(hub): State<WebHub>,
    Query(p): Query<SearchQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let q = p.q.as_deref().unwrap_or("");
    match queries::search(&ws.graph_pool, q, page_limit(p.limit, 100)).await {
        Ok(results) => (
            StatusCode::OK,
            Json(serde_json::json!({ "results": results })),
        )
            .into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct UnresolvedQuery {
    q: Option<String>,
    kind: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

async fn handle_unresolved_summary(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match queries::get_unresolved_summary(&ws.graph_pool).await {
        Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

async fn handle_unresolved(
    State(hub): State<WebHub>,
    Query(p): Query<UnresolvedQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let filter = queries::UnresolvedFilter {
        q: p.q.as_deref(),
        kind: p.kind.as_deref(),
        limit: page_limit(p.limit, 500),
        offset: page_offset(p.offset),
    };
    match queries::get_unresolved_refs(&ws.graph_pool, filter).await {
        Ok(page) => (
            StatusCode::OK,
            Json(serde_json::json!({ "refs": page.refs, "total": page.total })),
        )
            .into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

async fn handle_unresolved_reconcile(State(hub): State<WebHub>) -> impl IntoResponse {
    if hub.readonly {
        ax_usage::log_share(None, "readonly write denied");
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({ "error": "read-only mode (AX_WEB_READONLY=1)" })),
        )
            .into_response();
    }
    let ws = hub.read().await;
    let queries = QueryBuilder::new(ws.graph_pool.clone());
    let pruned = match prune_stale_unresolved_refs(&queries).await {
        Ok(p) => p,
        Err(e) => return api_err(e.to_string()).into_response(),
    };
    let mut resolver = ReferenceResolver::new(&ws.project_root);
    let resolution = match resolver.resolve_all(&queries, None, None).await {
        Ok(r) => r,
        Err(e) => return api_err(e.to_string()).into_response(),
    };
    let remaining = queries.count_unresolved_refs().await.unwrap_or(0);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "ok": true,
            "pruned": pruned,
            "resolved": resolution.stats.resolved,
            "remaining": remaining,
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct SourceQuery {
    path: String,
    /// 1-based inclusive start line. Defaults to 1.
    start: Option<i64>,
    /// 1-based inclusive end line. Defaults to start + 200.
    end: Option<i64>,
    /// Extra lines shown above/below the requested range.
    #[serde(default = "default_source_context")]
    context: i64,
}

fn default_source_context() -> i64 {
    3
}

const SOURCE_MAX_LINES: i64 = 500;

/// Largest file `/api/source` reads; bigger files get 413.
const SOURCE_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Paths `/api/source` never serves, even when an index lists them.
fn source_path_denied(rel: &str) -> bool {
    rel.split('/')
        .any(|part| part == ".git" || part == ".ax" || part == ".env" || part.starts_with(".env."))
}

/// Whether `/api/source` may serve `resolved`: the file must be in the
/// index's `files` table (by the requested path or by its path relative to
/// the canonical root) and not on the deny list.
async fn source_is_indexed(
    pool: &sqlx::SqlitePool,
    root: &std::path::Path,
    resolved: &std::path::Path,
    requested: &str,
) -> Result<bool, sqlx::Error> {
    let Ok(rel) = resolved.strip_prefix(root) else {
        return Ok(false);
    };
    let rel = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    let requested = requested.replace('\\', "/");
    let requested = requested.trim_start_matches("./");
    if rel.is_empty() || source_path_denied(&rel) || source_path_denied(requested) {
        return Ok(false);
    }
    let hit =
        sqlx::query_scalar::<_, i64>("SELECT 1 FROM files WHERE path = ? OR path = ? LIMIT 1")
            .bind(&rel)
            .bind(requested)
            .fetch_optional(pool)
            .await?;
    Ok(hit.is_some())
}

async fn handle_source(
    State(hub): State<WebHub>,
    Query(p): Query<SourceQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let root = match ws.project_root.canonicalize() {
        Ok(r) => r,
        Err(e) => return api_err(format!("project root unavailable: {e}")).into_response(),
    };
    let replicated = ax_db::replication::is_replica(&p.path);
    let text = if replicated {
        match sqlx::query_scalar::<_, String>("SELECT content FROM file_contents WHERE path=?")
            .bind(&p.path)
            .fetch_optional(&ws.graph_pool)
            .await
        {
            Ok(Some(text)) => text,
            Ok(None) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: "Replicated source is not stored in the received index".into(),
                    }),
                )
                    .into_response()
            }
            Err(e) => return api_err(e.to_string()).into_response(),
        }
    } else {
        // Reject traversal: resolve the candidate and require it stays inside the root.
        let candidate = root.join(p.path.replace('\\', "/"));
        let resolved = match candidate.canonicalize() {
            Ok(r) => r,
            Err(_) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: format!("file not found: {}", p.path),
                    }),
                )
                    .into_response()
            }
        };
        if !resolved.starts_with(&root) {
            return (
                StatusCode::FORBIDDEN,
                Json(ApiError {
                    error: "path outside project root".into(),
                }),
            )
                .into_response();
        }
        // Serve only files the index holds, so files left out of the index on
        // purpose (secrets, VCS metadata, ax state) stay unreadable here.
        match source_is_indexed(&ws.graph_pool, &root, &resolved, &p.path).await {
            Ok(true) => {}
            Ok(false) => {
                return (
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: format!("file is not in the index: {}", p.path),
                    }),
                )
                    .into_response()
            }
            Err(e) => return api_err(e.to_string()).into_response(),
        }

        let path = resolved.clone();
        let read =
            tokio::task::spawn_blocking(move || ax_utils::read_bounded(&path, SOURCE_MAX_BYTES))
                .await;
        match read {
            Ok(Ok(bytes)) => match String::from_utf8(bytes) {
                Ok(t) => t,
                Err(_) => {
                    return api_err(format!("cannot read {}: not UTF-8 text", p.path))
                        .into_response()
                }
            },
            Ok(Err(e)) if e.kind() == std::io::ErrorKind::InvalidData => {
                return (
                    StatusCode::PAYLOAD_TOO_LARGE,
                    Json(ApiError {
                        error: format!(
                            "{} is larger than {} MiB",
                            p.path,
                            SOURCE_MAX_BYTES / (1024 * 1024)
                        ),
                    }),
                )
                    .into_response()
            }
            Ok(Err(e)) => return api_err(format!("cannot read {}: {e}", p.path)).into_response(),
            Err(e) => return api_err(format!("cannot read {}: {e}", p.path)).into_response(),
        }
    };
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len() as i64;

    // Query values are untrusted i64s: saturate so extreme values clamp
    // instead of overflowing.
    let context = p.context.max(0);
    let start = p.start.unwrap_or(1).max(1);
    let end = p.end.unwrap_or(start.saturating_add(200)).max(start);
    let from = start.saturating_sub(context).max(1);
    let to = end
        .saturating_add(context)
        .min(total)
        .min(from.saturating_add(SOURCE_MAX_LINES - 1));

    let slice: Vec<serde_json::Value> = lines
        .iter()
        .enumerate()
        .skip((from - 1) as usize)
        .take((to - from + 1).max(0) as usize)
        .map(|(i, l)| serde_json::json!({ "no": (i + 1) as i64, "text": l }))
        .collect();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "path": p.path,
            "from": from,
            "to": to,
            "total_lines": total,
            "replicated":replicated,
            "lines": slice,
        })),
    )
        .into_response()
}

#[derive(Deserialize)]
struct GraphQuery {
    #[serde(default = "default_graph_limit")]
    limit: i64,
    #[serde(default)]
    recompute: bool,
    #[serde(default)]
    origin: String,
}

fn default_graph_limit() -> i64 {
    600
}

fn graph_max_limit() -> i64 {
    3_000
}

async fn load_graph_payload(hub: &WebHub, p: GraphQuery) -> Result<queries::GraphPayload, String> {
    let ws = hub.read().await;
    let limit = p.limit.clamp(1, graph_max_limit());
    // The canvas is the open workspace index. ~/.ax/global.db stays a cross-project
    // store for policy sync; it must not paint other workspaces onto this graph.
    let qb = QueryBuilder::new(ws.graph_pool.clone());
    let needs_compute =
        p.recompute || matches!(qb.communities_computed_at().await, Ok(None) | Err(_));
    if needs_compute && !hub.readonly {
        let gm = ax_graph::GraphQueryManager::new(QueryBuilder::new(ws.graph_pool.clone()));
        if let Err(e) = gm.compute_insights(1.0, 30, 30).await {
            return Err(format!("community detection failed: {e}"));
        }
    }
    if let Err(e) = node_tracker::observe(&ws.graph_pool, &ws.db_path).await {
        tracing::debug!("node tracker snapshot failed: {e}");
    }
    queries::get_graph(&ws.graph_pool, limit, &p.origin)
        .await
        .map_err(|e| e.to_string())
}

/// Ensure community assignments exist (compute + persist on first use or when
/// `recompute` is requested), then return the force-directed graph payload.
async fn handle_graph(State(hub): State<WebHub>, Query(p): Query<GraphQuery>) -> impl IntoResponse {
    match load_graph_payload(&hub, p).await {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(e) => api_err(e).into_response(),
    }
}

#[derive(Deserialize)]
struct RecentGraphQuery {
    since: Option<String>,
    limit: Option<i64>,
}

const RECENT_GRAPH_MAX: i64 = 200;

async fn handle_graph_recent(
    State(hub): State<WebHub>,
    Query(p): Query<RecentGraphQuery>,
) -> impl IntoResponse {
    let Some(since) = p.since.as_deref().and_then(|s| s.parse::<i64>().ok()) else {
        return (
            StatusCode::BAD_REQUEST,
            api_err("`since` must be a unix time in milliseconds").1,
        )
            .into_response();
    };
    let limit = p
        .limit
        .unwrap_or(RECENT_GRAPH_MAX)
        .clamp(1, RECENT_GRAPH_MAX);
    let ws = hub.read().await;
    if let Err(e) = node_tracker::observe(&ws.graph_pool, &ws.db_path).await {
        return api_err(e.to_string()).into_response();
    }
    let ids = node_tracker::created_since(&ws.db_path, since, limit as usize);
    match queries::get_graph_for_ids(&ws.graph_pool, &ids).await {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct GraphAsOfQuery {
    at: i64,
    #[serde(default = "default_graph_limit")]
    limit: i64,
}

async fn handle_graph_as_of(
    State(hub): State<WebHub>,
    Query(p): Query<GraphAsOfQuery>,
) -> impl IntoResponse {
    let limit = p.limit.clamp(1, graph_max_limit());
    let ws = hub.read().await;
    let ids: Result<Vec<String>, _> = sqlx::query_scalar(
        "SELECT DISTINCT node_key FROM graph_node_versions
         WHERE valid_from <= ?
           AND (valid_to IS NULL OR valid_to > ?)
         ORDER BY node_key
         LIMIT ?",
    )
    .bind(p.at)
    .bind(p.at)
    .bind(limit)
    .fetch_all(&ws.graph_pool)
    .await;
    let ids = match ids {
        Ok(ids) => ids,
        Err(e) => return api_err(e.to_string()).into_response(),
    };
    match queries::get_graph_for_ids(&ws.graph_pool, &ids).await {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

/// Streaming variant of [`handle_graph`]: emits the graph as SSE events
/// (`meta`, then batches of `nodes`, then batches of `edges`, then `done`) so
/// the client can render the graph gradually instead of blocking on one large
/// JSON payload.
async fn handle_graph_stream(
    State(hub): State<WebHub>,
    Query(p): Query<GraphQuery>,
) -> impl IntoResponse {
    use axum::response::sse::{Event, KeepAlive, Sse};

    let payload = match load_graph_payload(&hub, p).await {
        Ok(payload) => payload,
        Err(e) => return api_err(e).into_response(),
    };

    const NODE_BATCH: usize = 200;
    const EDGE_BATCH: usize = 800;

    let stream = async_stream::stream! {
        let palette = payload.palette.clone().unwrap_or_default();
        let meta = serde_json::json!({
            "type": "meta",
            "total_nodes": payload.total_nodes,
            "truncated": payload.truncated,
            "node_count": payload.nodes.len(),
            "edge_count": payload.edges.len(),
            "palette": if palette.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(palette) },
            "systems": payload.systems,
        });
        yield Ok::<Event, std::convert::Infallible>(Event::default().data(meta.to_string()));

        for chunk in payload.nodes.chunks(NODE_BATCH) {
            let arr = serde_json::to_string(chunk).unwrap_or_else(|_| "[]".to_string());
            yield Ok(Event::default().data(format!("{{\"type\":\"nodes\",\"nodes\":{arr}}}")));
            tokio::task::yield_now().await;
        }

        for chunk in payload.edges.chunks(EDGE_BATCH) {
            let arr = serde_json::to_string(chunk).unwrap_or_else(|_| "[]".to_string());
            yield Ok(Event::default().data(format!("{{\"type\":\"edges\",\"edges\":{arr}}}")));
            tokio::task::yield_now().await;
        }

        yield Ok(Event::default().data("{\"type\":\"done\"}"));
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[derive(Deserialize)]
struct InsightsQuery {
    #[serde(default = "default_resolution")]
    resolution: f64,
}

fn default_resolution() -> f64 {
    1.0
}

/// Recompute and return whole-graph insights (god nodes, communities,
/// surprising connections). Persists community assignments as a side effect.
async fn handle_insights(
    State(hub): State<WebHub>,
    Query(p): Query<InsightsQuery>,
) -> impl IntoResponse {
    if hub.readonly {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "read-only mode (AX_WEB_READONLY=1)".into(),
            }),
        )
            .into_response();
    }
    let ws = hub.read().await;
    let gm = ax_graph::GraphQueryManager::new(QueryBuilder::new(ws.graph_pool.clone()));
    let resolution = if p.resolution > 0.0 {
        p.resolution
    } else {
        1.0
    };
    match gm.compute_insights(resolution, 25, 25).await {
        Ok(insights) => {
            let suggested_questions = ax_core::report::suggested_questions(&insights);
            (
                StatusCode::OK,
                Json(InsightsApiResponse {
                    insights,
                    suggested_questions,
                }),
            )
                .into_response()
        }
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InsightsApiResponse {
    #[serde(flatten)]
    insights: ax_graph::GraphInsights,
    suggested_questions: Vec<String>,
}

async fn handle_version() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "version": env!("CARGO_PKG_VERSION") })),
    )
}

/// One-shot recovery for a poisoned browser origin (stale immutable `sw.js` / Cache Storage).
/// Open `http://localhost:PORT/api/reset-client-cache` once, then reload — browsers treat
/// `localhost` and `127.0.0.1` as separate origins, so only the origin you hit is cleared.
pub async fn handle_reset_client_cache() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html; charset=utf-8")
        .header("Cache-Control", "no-store, no-cache, must-revalidate")
        .header("Clear-Site-Data", "\"cache\", \"storage\"")
        .body(Body::from(
            r#"<!doctype html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="refresh" content="1;url=/">
<title>ax — client cache cleared</title>
</head>
<body>
<p>Cleared Command Center cache and service workers for this origin. Redirecting…</p>
<script>
(async function () {
  try {
    if ('serviceWorker' in navigator) {
      const regs = await navigator.serviceWorker.getRegistrations();
      await Promise.all(regs.map((r) => r.unregister()));
    }
    if ('caches' in window) {
      const keys = await caches.keys();
      await Promise.all(keys.map((k) => caches.delete(k)));
    }
  } catch (_) {}
  location.replace('/');
})();
</script>
</body>
</html>"#,
        ))
        .unwrap()
}

/// Reads one web UI file. `path` comes from the request URL, so it must stay
/// inside a dist folder: `..`, absolute paths, backslashes and links that
/// point out of the folder are refused.
fn dist_file(path: &str) -> Option<(Vec<u8>, &'static str)> {
    let _ = WEB_DIST_DISK_HINT;
    if path.contains('\\') {
        return None;
    }
    let rel = ax_utils::safe_relative(path).ok()?;
    for root in dist_roots() {
        let Ok(disk) = ax_utils::contained_join(&root, path) else {
            continue;
        };
        if disk.is_file() {
            return std::fs::read(disk).ok().map(|b| (b, "disk"));
        }
    }
    WEB_DIST
        .get_file(&rel)
        .map(|f| (f.contents().to_vec(), "embed"))
}

fn dist_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(p) = std::env::var("AX_WEB_DIST") {
        roots.push(PathBuf::from(p));
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web-ui/dist"));
    if let Ok(exe) = std::env::current_exe() {
        // target-dev/release/ax → <repo>/crates/ax-web/web-ui/dist
        if let Some(repo) = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
        {
            roots.push(repo.join("crates/ax-web/web-ui/dist"));
        }
    }
    roots
}

async fn handle_spa(uri: Uri) -> impl IntoResponse {
    let raw_path = uri.path();
    let path = raw_path.trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    // Axum nests like `/api/memory` do not match `/api/memory/`. Redirect so clients
    // (and `fetch`) land on the real JSON route instead of a blank SPA/HTML body.
    if (path == "api" || path.starts_with("api/")) && raw_path.ends_with('/') && raw_path.len() > 1
    {
        let trimmed = raw_path.trim_end_matches('/');
        let loc = match uri.query() {
            Some(q) => format!("{trimmed}?{q}"),
            None => trimmed.to_string(),
        };
        return Response::builder()
            .status(StatusCode::PERMANENT_REDIRECT)
            .header("Location", loc)
            .header("Cache-Control", "no-cache")
            .body(Body::empty())
            .unwrap();
    }

    // Never serve the SPA shell for missed API routes.
    if path == "api" || path.starts_with("api/") {
        return Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "application/json; charset=utf-8")
            .header("Cache-Control", "no-cache")
            .body(Body::from(r#"{"error":"not found"}"#))
            .unwrap();
    }

    if let Some((bytes, src)) = dist_file(path) {
        let mime = mime_guess::from_path(path).first_or_text_plain();
        let cache = if path == "index.html" {
            "no-cache, no-store, must-revalidate"
        } else if path.starts_with("assets/") {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache, no-store, must-revalidate"
        };
        let mut builder = Response::builder()
            .status(200)
            .header("Content-Type", mime.as_ref())
            .header("Cache-Control", cache)
            .header("X-Ax-Web-Dist", src);
        if path == "sw.js" {
            builder = builder.header("Service-Worker-Allowed", "/");
        }
        builder.body(Body::from(bytes)).unwrap()
    } else if path.starts_with("assets/") || path.contains('.') {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "text/plain; charset=utf-8")
            .header("Cache-Control", "no-cache")
            .body(Body::from("Not found"))
            .unwrap()
    } else {
        let index = dist_file("index.html").map(|(b, _)| b).unwrap_or_default();
        Response::builder()
            .status(200)
            .header("Content-Type", "text/html; charset=utf-8")
            .header("Cache-Control", "no-cache, no-store, must-revalidate")
            .body(Body::from(index))
            .unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub root: PathBuf,
    pub port: u16,
    pub open: bool,
    /// Bind address (default `127.0.0.1`). Use `0.0.0.0` for LAN/`ax share`.
    pub bind: String,
    /// When set, requires `?token=` / Bearer / cookie (also sets `AX_SHARE_TOKEN`).
    pub share_token: Option<String>,
    /// Serve beyond loopback without a login or share token. Off by default;
    /// `AX_WEB_ALLOW_ANONYMOUS_LAN=1` has the same effect.
    pub allow_anonymous_lan: bool,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            port: 7070,
            open: false,
            bind: "127.0.0.1".into(),
            share_token: None,
            allow_anonymous_lan: false,
        }
    }
}

pub async fn serve(root: PathBuf, port: u16, open: bool) -> Result<(), String> {
    serve_with(ServeOptions {
        root,
        port,
        open,
        ..ServeOptions::default()
    })
    .await
}

pub async fn serve_with(opts: ServeOptions) -> Result<(), String> {
    if let Some(token) = &opts.share_token {
        std::env::set_var("AX_SHARE_TOKEN", token);
    }
    let readonly =
        std::env::var("AX_WEB_READONLY").ok().as_deref() == Some("1") || opts.share_token.is_some();
    let mut hub = WebHub::open(opts.root.clone(), readonly, opts.port).await?;
    hub.local_only = matches!(
        opts.bind.as_str(),
        "127.0.0.1" | "localhost" | "::1" | "[::1]"
    );
    if !hub.local_only {
        let login = match ax_global_db::global_db_path() {
            Ok(path) => match ax_global_db::open_and_init(&path).await {
                Ok(pool) => web_auth::web_login_enabled(&pool).await,
                Err(_) => false,
            },
            Err(_) => false,
        };
        let allow_anonymous = opts.allow_anonymous_lan
            || std::env::var("AX_WEB_ALLOW_ANONYMOUS_LAN").ok().as_deref() == Some("1");
        if let Some(warning) = request_guard::lan_exposure(
            &opts.bind,
            opts.share_token.is_some(),
            login,
            allow_anonymous,
        )? {
            tracing::warn!("{warning}");
            eprintln!("ax web: WARNING: {warning}");
        }
    }
    if !readonly {
        if let Err(error) = replication_setup::auto_start(&opts.root).await {
            tracing::warn!("replication automatic startup failed: {error}");
        }
    }
    let _ = ax_agent::config::touch_recent_project(&opts.root, true);

    let cors = browser_origin::command_center_cors(&opts.bind, opts.port);

    if !readonly {
        let sync_hub = hub.clone();
        tokio::spawn(async move { vault_folders::sync_indexed(&sync_hub).await });
    }
    let app = hub.nest_routers(cors);

    let addr = format!("{}:{}", opts.bind, opts.port);
    let listener = bind_web_listener(&addr, opts.port).await?;

    // Prefer 127.0.0.1 over localhost: browsers treat them as separate origins, and
    // a poisoned localhost service-worker/HTTP cache can leave pages empty forever
    // while 127.0.0.1 stays healthy.
    let local_url = format!("http://127.0.0.1:{}", opts.port);
    eprintln!("ax web  {local_url}");
    eprintln!(
        "         (also http://localhost:{} — if that origin shows empty pages, open http://localhost:{}/api/reset-client-cache once)",
        opts.port, opts.port
    );
    if opts.bind != "127.0.0.1" && opts.bind != "localhost" {
        if let Some(ip) = guess_lan_ip() {
            let mut share = format!("http://{ip}:{}", opts.port);
            if let Some(token) = &opts.share_token {
                share.push_str(&format!("?token={token}"));
            }
            eprintln!("ax share {share}");
        }
    }
    if let Some(token) = &opts.share_token {
        ax_usage::log_share(Some(&opts.root), "session start readonly=true");
        eprintln!("  Share token: {token}");
        eprintln!("  Mode: read-only (share session)");
    }
    eprintln!("  Graph + policy: {}", opts.root.display());
    eprintln!("  Press Ctrl+C to stop.");

    if opts.open {
        let mut open_url = local_url.clone();
        if let Some(token) = &opts.share_token {
            open_url.push_str(&format!("?token={token}"));
        }
        open_browser(&open_url);
    }

    actions::publish("web", "server started", None);
    ax_usage::spawn_ensure_daily_pricing_sync();

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .map_err(|e| e.to_string())
}

fn guess_lan_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

async fn bind_web_listener(addr: &str, port: u16) -> Result<tokio::net::TcpListener, String> {
    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => Ok(listener),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            // Only an earlier `ax web` is stopped; any other program keeps the port.
            let stopped = tokio::task::spawn_blocking(move || stop_previous_ax_web(port))
                .await
                .map_err(|e| format!("Cannot bind to {addr}: {e}"))??;
            if stopped > 0 {
                eprintln!("ax web: stopped the previous ax web on port {port}");
            }
            for _ in 0..10 {
                match tokio::net::TcpListener::bind(addr).await {
                    Ok(listener) => return Ok(listener),
                    Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    }
                    Err(e) => return Err(format!("Cannot bind to {addr}: {e}")),
                }
            }
            Err(format!(
                "Cannot bind to {addr}: port {port} is still in use. Choose another port with --port."
            ))
        }
        Err(e) => Err(format!("Cannot bind to {addr}: {e}")),
    }
}

/// Whether a process command (name or executable path) is the `ax` binary.
fn is_ax_command(command: &str) -> bool {
    let name = command
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    name == "ax" || name.eq_ignore_ascii_case("ax.exe")
}

/// The command name of `pid`, or `None` when it cannot be read.
fn process_command(pid: u32) -> Option<String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let out = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let first = text.lines().next()?.split(',').next()?.trim_matches('"');
        (!first.is_empty() && !first.contains(' ')).then(|| first.to_string())
    }
    #[cfg(unix)]
    {
        let out = std::process::Command::new("ps")
            .args(["-o", "comm=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (out.status.success() && !text.is_empty()).then_some(text)
    }
}

/// Asks `pid` to stop, waits, and force-stops it only if it is still running.
fn stop_pid_gracefully(pid: u32) {
    #[cfg(windows)]
    let (soft, hard): (Vec<String>, Vec<String>) = (
        vec!["/PID".into(), pid.to_string()],
        vec!["/PID".into(), pid.to_string(), "/F".into()],
    );
    #[cfg(windows)]
    let program = "taskkill";
    #[cfg(unix)]
    let (soft, hard): (Vec<String>, Vec<String>) = (
        vec!["-TERM".into(), pid.to_string()],
        vec!["-KILL".into(), pid.to_string()],
    );
    #[cfg(unix)]
    let program = "kill";
    let _ = std::process::Command::new(program).args(&soft).status();
    for _ in 0..30 {
        if !ax_utils::is_pid_alive(pid) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let _ = std::process::Command::new(program).args(&hard).status();
}

/// Stops an earlier `ax web` that holds `port`. Fails, and stops nothing,
/// when the port belongs to any other program. Returns how many processes
/// were stopped.
fn stop_previous_ax_web(port: u16) -> Result<usize, String> {
    let self_pid = std::process::id();
    let pids: Vec<u32> = ax_utils::pids_listening_on_port(port)
        .map_err(|e| format!("Port {port} is in use ({e}). Choose another port with --port."))?
        .into_iter()
        .filter(|pid| *pid != self_pid)
        .collect();
    for pid in &pids {
        let command = process_command(*pid);
        if !command.as_deref().is_some_and(is_ax_command) {
            let who = command.unwrap_or_else(|| "an unknown program".into());
            return Err(format!(
                "Port {port} is in use by {who} (pid {pid}). ax web does not stop other programs; choose another port with --port."
            ));
        }
    }
    for pid in &pids {
        stop_pid_gracefully(*pid);
    }
    Ok(pids.len())
}

fn open_browser(url: &str) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c().await.ok();
    eprintln!("\nax web: shutting down.");
}

#[cfg(test)]
mod port_tests {
    use super::*;

    #[test]
    fn only_ax_commands_count_as_ax() {
        assert!(is_ax_command("ax"));
        assert!(is_ax_command("/usr/local/bin/ax"));
        assert!(is_ax_command("C:\\Tools\\ax.exe"));
        assert!(is_ax_command("ax.exe"));
        assert!(!is_ax_command("node"));
        assert!(!is_ax_command("/usr/bin/python3"));
        assert!(!is_ax_command("max"));
        assert!(!is_ax_command("ax-something-else"));
        assert!(!is_ax_command(""));
    }

    /// Another program holding the port is left alone: binding fails with a
    /// message that names `--port`, and the program keeps running.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_foreign_listener_is_not_killed() {
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = probe.local_addr().unwrap().port();
        drop(probe);
        let script = format!(
            "import socket,time\ns=socket.socket()\ns.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1)\ns.bind(('127.0.0.1',{port}))\ns.listen()\ntime.sleep(60)"
        );
        let Ok(mut child) = std::process::Command::new("python3")
            .args(["-c", &script])
            .spawn()
        else {
            eprintln!("skip: python3 not available");
            return;
        };
        let addr = format!("127.0.0.1:{port}");
        let mut up = false;
        for _ in 0..100 {
            if std::net::TcpStream::connect(&addr).is_ok() {
                up = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(up, "helper listener did not start");

        let result = bind_web_listener(&addr, port).await;
        let alive = child.try_wait().unwrap().is_none();
        let _ = child.kill();
        let _ = child.wait();
        assert!(alive, "the foreign listener was killed");
        let err = result.expect_err("bind must fail while the port is taken");
        assert!(err.contains("--port"), "{err}");
    }
}
