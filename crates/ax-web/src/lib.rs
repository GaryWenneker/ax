//! ax-web: local HTTP server exposing the ax code graph + policy editor.

mod actions;
mod agent;
mod agent_pty;
mod mcp_quality;
mod mcp_trace;
mod memory;
mod policy;
mod queries;
mod share_auth;
mod ship;
mod sonar_proxy;
mod savings;
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
use include_dir::{include_dir, Dir};
use serde::{Deserialize, Serialize};
use tower_http::cors::{Any, CorsLayer};

static WEB_DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/web-ui/dist");

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
struct ApiError {
    error: String,
}

fn api_err(msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: msg.into(),
        }),
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

async fn handle_nodes(
    State(hub): State<WebHub>,
    Query(p): Query<NodesQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let filter = queries::NodeFilter {
        kind: p.kind.as_deref(),
        lang: p.lang.as_deref(),
        file: p.file.as_deref(),
        q: p.q.as_deref(),
        limit: p.limit.min(nodes_max_limit()),
        offset: p.offset,
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

async fn handle_files(
    State(hub): State<WebHub>,
    Query(p): Query<FilesQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let filter = queries::FileFilter {
        lang: p.lang.as_deref(),
        q: p.q.as_deref(),
        prefix: p.prefix.as_deref(),
        limit: p.limit.min(files_max_limit()),
        offset: p.offset,
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
        Ok(roots) => (StatusCode::OK, Json(serde_json::json!({ "roots": roots }))).into_response(),
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
    match queries::search(&ws.graph_pool, q, p.limit.min(100)).await {
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
        limit: p.limit.min(500),
        offset: p.offset,
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

async fn handle_source(
    State(hub): State<WebHub>,
    Query(p): Query<SourceQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let root = match ws.project_root.canonicalize() {
        Ok(r) => r,
        Err(e) => return api_err(format!("project root unavailable: {e}")).into_response(),
    };
    // Reject traversal: resolve the candidate and require it stays inside the root.
    let candidate = root.join(p.path.replace('\\', "/"));
    let resolved = match candidate.canonicalize() {
        Ok(r) => r,
        Err(_) => {
            return (
                StatusCode::NOT_FOUND,
                Json(ApiError { error: format!("file not found: {}", p.path) }),
            )
                .into_response()
        }
    };
    if !resolved.starts_with(&root) {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError { error: "path outside project root".into() }),
        )
            .into_response();
    }

    let text = match tokio::fs::read_to_string(&resolved).await {
        Ok(t) => t,
        Err(e) => return api_err(format!("cannot read {}: {e}", p.path)).into_response(),
    };
    let lines: Vec<&str> = text.lines().collect();
    let total = lines.len() as i64;

    let start = p.start.unwrap_or(1).max(1);
    let end = p.end.unwrap_or(start + 200).max(start);
    let from = (start - p.context.max(0)).max(1);
    let to = (end + p.context.max(0)).min(total).min(from + SOURCE_MAX_LINES - 1);

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
}

fn default_graph_limit() -> i64 {
    600
}

fn graph_max_limit() -> i64 {
    3_000
}

/// Ensure community assignments exist (compute + persist on first use or when
/// `recompute` is requested), then return the force-directed graph payload.
async fn handle_graph(
    State(hub): State<WebHub>,
    Query(p): Query<GraphQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let qb = QueryBuilder::new(ws.graph_pool.clone());
    let needs_compute = p.recompute
        || matches!(qb.communities_computed_at().await, Ok(None) | Err(_));
    if needs_compute && !hub.readonly {
        let gm = ax_graph::GraphQueryManager::new(QueryBuilder::new(ws.graph_pool.clone()));
        if let Err(e) = gm.compute_insights(1.0, 30, 30).await {
            return api_err(format!("community detection failed: {e}")).into_response();
        }
    }
    let limit = p.limit.clamp(1, graph_max_limit());
    match queries::get_graph(&ws.graph_pool, limit).await {
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

    let ws = hub.read().await;
    let qb = QueryBuilder::new(ws.graph_pool.clone());
    let needs_compute =
        p.recompute || matches!(qb.communities_computed_at().await, Ok(None) | Err(_));
    if needs_compute && !hub.readonly {
        let gm = ax_graph::GraphQueryManager::new(QueryBuilder::new(ws.graph_pool.clone()));
        if let Err(e) = gm.compute_insights(1.0, 30, 30).await {
            return api_err(format!("community detection failed: {e}")).into_response();
        }
    }
    let limit = p.limit.clamp(1, graph_max_limit());
    let payload = match queries::get_graph(&ws.graph_pool, limit).await {
        Ok(payload) => payload,
        Err(e) => return api_err(e.to_string()).into_response(),
    };
    drop(ws);

    const NODE_BATCH: usize = 200;
    const EDGE_BATCH: usize = 800;

    let stream = async_stream::stream! {
        let meta = format!(
            "{{\"type\":\"meta\",\"total_nodes\":{},\"truncated\":{},\"node_count\":{},\"edge_count\":{}}}",
            payload.total_nodes,
            payload.truncated,
            payload.nodes.len(),
            payload.edges.len()
        );
        yield Ok::<Event, std::convert::Infallible>(Event::default().data(meta));

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

        yield Ok(Event::default().data("{\"type\":\"done\"}".to_string()));
    };

    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
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
            Json(ApiError { error: "read-only mode (AX_WEB_READONLY=1)".into() }),
        )
            .into_response();
    }
    let ws = hub.read().await;
    let gm = ax_graph::GraphQueryManager::new(QueryBuilder::new(ws.graph_pool.clone()));
    let resolution = if p.resolution > 0.0 { p.resolution } else { 1.0 };
    match gm.compute_insights(resolution, 25, 25).await {
        Ok(insights) => (StatusCode::OK, Json(insights)).into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

async fn handle_version() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(serde_json::json!({ "version": env!("CARGO_PKG_VERSION") })),
    )
}

async fn handle_spa(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = WEB_DIST.get_file(path) {
        let mime = mime_guess::from_path(path).first_or_text_plain();
        let cache = if path == "index.html" {
            "no-cache"
        } else {
            "public, max-age=31536000, immutable"
        };
        Response::builder()
            .status(200)
            .header("Content-Type", mime.as_ref())
            .header("Cache-Control", cache)
            .body(Body::from(file.contents().to_vec()))
            .unwrap()
    } else if path.starts_with("assets/") || path.contains('.') {
        Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header("Content-Type", "text/plain; charset=utf-8")
            .header("Cache-Control", "no-cache")
            .body(Body::from("Not found"))
            .unwrap()
    } else {
        let index = WEB_DIST
            .get_file("index.html")
            .map(|f| f.contents().to_vec())
            .unwrap_or_default();
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
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            port: 7070,
            open: false,
            bind: "127.0.0.1".into(),
            share_token: None,
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
    let readonly = std::env::var("AX_WEB_READONLY").ok().as_deref() == Some("1")
        || opts.share_token.is_some();
    let hub = WebHub::open(opts.root.clone(), readonly, opts.port).await?;
    let _ = ax_agent::config::touch_recent_project(&opts.root, true);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = hub.nest_routers(cors);

    let addr = format!("{}:{}", opts.bind, opts.port);
    let listener = bind_web_listener(&addr, opts.port).await?;

    let local_url = format!("http://localhost:{}", opts.port);
    eprintln!("ax web  {local_url}");
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

    axum::serve(listener, app)
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
    free_web_port(port);

    match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => Ok(listener),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            free_web_port(port);
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            tokio::net::TcpListener::bind(addr)
                .await
                .map_err(|e| format!("Cannot bind to {addr}: {e}"))
        }
        Err(e) => Err(format!("Cannot bind to {addr}: {e}")),
    }
}

fn free_web_port(port: u16) {
    let self_pid = std::process::id();
    match ax_utils::kill_listening_on_port(port, self_pid) {
        Ok(0) => {}
        Ok(n) => eprintln!("ax web: freed port {port} (stopped {n} process(es))"),
        Err(e) => eprintln!("ax web: warning: could not free port {port}: {e}"),
    }
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
