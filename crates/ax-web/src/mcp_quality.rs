//! Live MCP quality snapshots for Command Center (status-bar slide-out).

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::Stream;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::RwLock;

use ax_usage::{
    audit_project, format_markdown_report, load_latest_snapshot, AuditOptions, QualitySnapshot,
    DEFAULT_WINDOW_MINUTES,
};

use crate::workspace_state::WebHub;

#[derive(Clone, Default)]
struct QualityCache {
    root: PathBuf,
    log_mtime_ms: i64,
    snap: Option<QualitySnapshot>,
}

#[derive(Clone)]
pub struct QualityState {
    cache: Arc<RwLock<QualityCache>>,
}

impl QualityState {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(RwLock::new(QualityCache::default())),
        }
    }
}

pub fn router(hub: WebHub) -> Router {
    let state = QualityState::new();
    spawn_watcher(hub.clone(), state.clone());
    Router::new()
        .route("/mcp-quality", get(handle_quality))
        .route("/mcp-quality/events", get(handle_quality_events))
        .route("/mcp-audit", post(handle_audit))
        .with_state((hub, state))
}

fn file_mtime_ms(path: &std::path::Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn latest_verbose_log_mtime_ms(root: &std::path::Path) -> i64 {
    let mut max = file_mtime_ms(&ax_usage::current_log_path(Some(root)));
    for (_day, path) in ax_usage::list_dated_log_files(root) {
        max = max.max(file_mtime_ms(&path));
    }
    let legacy = root.join(".ax").join(ax_usage::LEGACY_LOG_NAME);
    if legacy.is_file() {
        max = max.max(file_mtime_ms(&legacy));
    }
    max
}

async fn refresh_snapshot(hub: &WebHub, state: &QualityState, force: bool) -> QualitySnapshot {
    let root = {
        let ws = hub.read().await;
        ws.project_root.clone()
    };
    let mtime = latest_verbose_log_mtime_ms(&root);

    {
        let cache = state.cache.read().await;
        if !force
            && cache.root == root
            && cache.log_mtime_ms == mtime
            && cache.snap.is_some()
            && mtime > 0
        {
            if let Some(ref s) = cache.snap {
                return s.clone();
            }
        }
    }

    let opts = AuditOptions {
        window_minutes: Some(DEFAULT_WINDOW_MINUTES),
        persist: true,
        ..Default::default()
    };
    let snap = match tokio::task::spawn_blocking({
        let root = root.clone();
        move || audit_project(&root, &opts)
    })
    .await
    {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => empty_error_snap(&root, &e),
        Err(e) => empty_error_snap(&root, &e.to_string()),
    };

    let mut cache = state.cache.write().await;
    cache.root = root;
    cache.log_mtime_ms = mtime;
    cache.snap = Some(snap.clone());
    snap
}

fn empty_error_snap(root: &std::path::Path, err: &str) -> QualitySnapshot {
    let mut snap = load_latest_snapshot(root).unwrap_or_else(|| QualitySnapshot {
        project_root: root.display().to_string(),
        project_label: root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".into()),
        log_path: ax_usage::current_log_path(Some(root)).display().to_string(),
        mode: "verbose_only".into(),
        window_minutes: DEFAULT_WINDOW_MINUTES,
        updated_at_ms: 0,
        score: 0,
        grade: "F".into(),
        correlation_pct: 0.0,
        matched_calls: 0,
        unmatched_ax_calls: 0,
        verbose_clusters: 0,
        enrichment: Default::default(),
        tool_mix: Default::default(),
        findings: vec![],
        tokens_at_risk: 0,
        critical_count: 0,
        verbose_enabled: false,
        verbose_present: false,
        session_id: None,
        session_path: None,
    });
    snap.findings.insert(
        0,
        ax_usage::Finding {
            id: "audit-error".into(),
            check: "VerboseGap".into(),
            severity: "medium".into(),
            title: "Quality audit failed".into(),
            detail: err.to_string(),
            waste_hint: "Fix verbose log / transcript access and retry.".into(),
            tokens_est: 0,
            tool: None,
            ts_ms: None,
            log_line_hint: None,
        },
    );
    snap
}

fn spawn_watcher(hub: WebHub, state: QualityState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            let _ = refresh_snapshot(&hub, &state, false).await;
        }
    });
}

async fn handle_quality(State((hub, state)): State<(WebHub, QualityState)>) -> impl IntoResponse {
    let snap = refresh_snapshot(&hub, &state, false).await;
    Json(snap)
}

#[derive(Deserialize, Default)]
struct AuditBody {
    #[serde(default)]
    session: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    window_minutes: Option<u64>,
    #[serde(default)]
    markdown: bool,
}

async fn handle_audit(
    State((hub, state)): State<(WebHub, QualityState)>,
    Json(body): Json<AuditBody>,
) -> impl IntoResponse {
    let root = {
        let ws = hub.read().await;
        ws.project_root.clone()
    };
    let mut opts = AuditOptions {
        window_minutes: body.window_minutes.or(Some(DEFAULT_WINDOW_MINUTES)),
        persist: true,
        ..Default::default()
    };
    if let Some(s) = body.session.or(body.session_id) {
        let p = PathBuf::from(&s);
        if p.is_file() {
            opts.session_path = Some(p);
        } else {
            opts.session_id = Some(s);
            opts.window_minutes = None; // full session
        }
    }
    let snap = match tokio::task::spawn_blocking(move || audit_project(&root, &opts)).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e })),
            )
                .into_response();
        }
        Err(e) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };
    {
        let mut cache = state.cache.write().await;
        cache.snap = Some(snap.clone());
        cache.root = PathBuf::from(&snap.project_root);
    }
    if body.markdown {
        let md = format_markdown_report(&snap);
        return Json(json!({ "snapshot": snap, "markdown": md })).into_response();
    }
    Json(snap).into_response()
}

async fn handle_quality_events(
    State((hub, state)): State<(WebHub, QualityState)>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut last_score: Option<u8> = None;
        let mut last_updated: i64 = 0;
        loop {
            let snap = refresh_snapshot(&hub, &state, false).await;
            if last_score != Some(snap.score) || snap.updated_at_ms != last_updated {
                last_score = Some(snap.score);
                last_updated = snap.updated_at_ms;
                if let Ok(data) = serde_json::to_string(&snap) {
                    yield Ok(Event::default().event("quality").data(data));
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}
