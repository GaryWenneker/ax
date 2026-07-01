//! ax-web: local HTTP server exposing the ax code graph + policy editor.

mod policy;
mod queries;

use std::path::PathBuf;
use std::sync::Arc;

use ax_policy::PolicyStore;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{Response, StatusCode, Uri},
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use include_dir::{include_dir, Dir};
use policy::PolicyApiState;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqliteConnectOptions, SqlitePool};
use tower_http::cors::{Any, CorsLayer};

static WEB_DIST: Dir = include_dir!("$CARGO_MANIFEST_DIR/web-ui/dist");

#[derive(Clone)]
struct AppState {
    graph_pool: SqlitePool,
    policy: PolicyApiState,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

fn api_err(msg: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: msg.into() }))
}

async fn handle_stats(State(s): State<AppState>) -> impl IntoResponse {
    match queries::get_stats(&s.graph_pool).await {
        Ok(stats) => (StatusCode::OK, Json(serde_json::to_value(stats).unwrap())).into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

#[derive(Deserialize)]
struct NodesQuery {
    kind: Option<String>,
    lang: Option<String>,
    q: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

fn default_limit() -> i64 {
    50
}

async fn handle_nodes(State(s): State<AppState>, Query(p): Query<NodesQuery>) -> impl IntoResponse {
    let filter = queries::NodeFilter {
        kind: p.kind.as_deref(),
        lang: p.lang.as_deref(),
        q: p.q.as_deref(),
        limit: p.limit.min(200),
        offset: p.offset,
    };
    match queries::get_nodes(&s.graph_pool, filter).await {
        Ok(page) => (
            StatusCode::OK,
            Json(serde_json::json!({ "nodes": page.nodes, "total": page.total })),
        )
            .into_response(),
        Err(e) => api_err(e.to_string()).into_response(),
    }
}

async fn handle_node(State(s): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    match queries::get_node_detail(&s.graph_pool, &id).await {
        Ok(Some(detail)) => (StatusCode::OK, Json(serde_json::to_value(detail).unwrap())).into_response(),
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
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
}

async fn handle_files(State(s): State<AppState>, Query(p): Query<FilesQuery>) -> impl IntoResponse {
    let filter = queries::FileFilter {
        lang: p.lang.as_deref(),
        q: p.q.as_deref(),
        limit: p.limit.min(200),
        offset: p.offset,
    };
    match queries::get_files(&s.graph_pool, filter).await {
        Ok(page) => (
            StatusCode::OK,
            Json(serde_json::json!({ "files": page.files, "total": page.total })),
        )
            .into_response(),
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

async fn handle_search(State(s): State<AppState>, Query(p): Query<SearchQuery>) -> impl IntoResponse {
    let q = p.q.as_deref().unwrap_or("");
    match queries::search(&s.graph_pool, q, p.limit.min(100)).await {
        Ok(results) => (
            StatusCode::OK,
            Json(serde_json::json!({ "results": results })),
        )
            .into_response(),
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
        Response::builder()
            .status(200)
            .header("Content-Type", mime.as_ref())
            .header("Cache-Control", "public, max-age=3600")
            .body(Body::from(file.contents().to_vec()))
            .unwrap()
    } else {
        let index = WEB_DIST
            .get_file("index.html")
            .map(|f| f.contents().to_vec())
            .unwrap_or_default();
        Response::builder()
            .status(200)
            .header("Content-Type", "text/html; charset=utf-8")
            .body(Body::from(index))
            .unwrap()
    }
}

pub async fn serve(root: PathBuf, port: u16, open: bool) -> Result<(), String> {
    let db_path = root.join(".ax").join("ax.db");
    if !db_path.exists() {
        return Err(format!(
            "No ax index found at {}. Run `ax init` first.",
            db_path.display()
        ));
    }

    let graph_opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .read_only(true)
        .create_if_missing(false);

    let graph_pool = SqlitePool::connect_with(graph_opts)
        .await
        .map_err(|e| format!("Failed to open ax.db: {e}"))?;

    let policy_pool = ax_policy::open_rw_pool(&db_path)
        .await
        .map_err(|e| e.to_string())?;
    ax_policy::ensure_scaffold(&root.join(".ax")).map_err(|e| e.to_string())?;
    let store = PolicyStore::new(policy_pool, root.clone());
    let _ = store.reindex(false).await;

    let readonly = std::env::var("AX_WEB_READONLY").ok().as_deref() == Some("1");
    let policy_state = PolicyApiState {
        store: Arc::new(store),
        readonly,
    };

    let state = AppState {
        graph_pool,
        policy: policy_state.clone(),
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let graph_api = Router::new()
        .route("/stats", get(handle_stats))
        .route("/version", get(handle_version))
        .route("/nodes", get(handle_nodes))
        .route("/node/{id}", get(handle_node))
        .route("/files", get(handle_files))
        .route("/search", get(handle_search))
        .with_state(state);

    let policy_api = policy::router(policy_state);

    let app = Router::new()
        .nest("/api", graph_api)
        .nest("/api/policy", policy_api)
        .fallback(handle_spa)
        .layer(cors);

    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Cannot bind to {addr}: {e}"))?;

    let url = format!("http://localhost:{port}");
    eprintln!("ax web  {url}");
    eprintln!("  Graph + policy: {}", root.display());
    eprintln!("  Press Ctrl+C to stop.");

    if open {
        open_browser(&url);
    }

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| e.to_string())
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
