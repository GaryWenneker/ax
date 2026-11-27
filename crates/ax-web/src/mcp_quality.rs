//! Live MCP quality snapshots for Command Center (status-bar slide-out).

use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
    /// When the cached snapshot was audited.
    audited_at: Option<std::time::Instant>,
}

/// A changed verbose log does not trigger a new audit sooner than this.
const MIN_AUDIT_INTERVAL: Duration = Duration::from_secs(5);

struct QualityInner {
    cache: RwLock<QualityCache>,
    /// Held while an audit runs, so concurrent callers share one audit.
    refresh: tokio::sync::Mutex<()>,
    /// Audits run so far (background and on request).
    audits: AtomicU64,
    watcher_started: AtomicBool,
}

#[derive(Clone)]
pub struct QualityState(Arc<QualityInner>);

impl QualityState {
    pub fn new() -> Self {
        Self(Arc::new(QualityInner {
            cache: RwLock::new(QualityCache::default()),
            refresh: tokio::sync::Mutex::new(()),
            audits: AtomicU64::new(0),
            watcher_started: AtomicBool::new(false),
        }))
    }

    #[cfg(test)]
    fn audit_runs(&self) -> u64 {
        self.0.audits.load(Ordering::SeqCst)
    }

    #[cfg(test)]
    fn watcher_running(&self) -> bool {
        self.0.watcher_started.load(Ordering::SeqCst)
    }
}

pub fn router(hub: WebHub) -> Router {
    router_with_state(hub, QualityState::new())
}

/// The background watcher starts with the first request to these routes,
/// not when the router is built.
fn router_with_state(hub: WebHub, state: QualityState) -> Router {
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

/// The cached snapshot when it still describes `root`: the verbose log has
/// not changed (also when there is none), or the last audit is recent.
fn cached_snapshot(
    cache: &QualityCache,
    root: &std::path::Path,
    mtime: i64,
) -> Option<QualitySnapshot> {
    if cache.root != root {
        return None;
    }
    let recent = cache
        .audited_at
        .is_some_and(|at| at.elapsed() < MIN_AUDIT_INTERVAL);
    if cache.log_mtime_ms == mtime || recent {
        return cache.snap.clone();
    }
    None
}

async fn refresh_snapshot(hub: &WebHub, state: &QualityState, force: bool) -> QualitySnapshot {
    let root = {
        let ws = hub.read().await;
        ws.project_root.clone()
    };
    let mtime = latest_verbose_log_mtime_ms(&root);
    if !force {
        if let Some(snap) = cached_snapshot(&*state.0.cache.read().await, &root, mtime) {
            return snap;
        }
    }

    // One audit at a time; callers that waited reuse its result.
    let _running = state.0.refresh.lock().await;
    if !force {
        if let Some(snap) = cached_snapshot(&*state.0.cache.read().await, &root, mtime) {
            return snap;
        }
    }

    let opts = AuditOptions {
        window_minutes: Some(DEFAULT_WINDOW_MINUTES),
        persist: true,
        ..Default::default()
    };
    state.0.audits.fetch_add(1, Ordering::SeqCst);
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

    let mut cache = state.0.cache.write().await;
    cache.root = root;
    cache.log_mtime_ms = mtime;
    cache.snap = Some(snap.clone());
    cache.audited_at = Some(std::time::Instant::now());
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

/// Starts the background refresh once. It holds only a weak reference to the
/// state, so it stops when the router that owns the state is dropped.
fn ensure_watcher(hub: &WebHub, state: &QualityState) {
    if state
        .0
        .watcher_started
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let hub = hub.clone();
    let weak = Arc::downgrade(&state.0);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        interval.tick().await; // the first tick fires at once; the request already refreshed
        loop {
            interval.tick().await;
            let Some(inner) = weak.upgrade() else {
                break;
            };
            let _ = refresh_snapshot(&hub, &QualityState(inner), false).await;
        }
    });
}

async fn handle_quality(State((hub, state)): State<(WebHub, QualityState)>) -> impl IntoResponse {
    ensure_watcher(&hub, &state);
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

/// A transcript session id: letters, digits, `-`, `_` and `.`, never a path.
fn is_session_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value != "."
        && value != ".."
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The transcript `requested` names, when it is one of this project's agent
/// transcripts. Any other file is refused, so a caller cannot probe or parse
/// arbitrary files.
fn known_transcript(root: &std::path::Path, requested: &str) -> Option<PathBuf> {
    let wanted = std::path::Path::new(requested).canonicalize().ok()?;
    ax_usage::find_cursor_transcripts(root)
        .into_iter()
        .find(|p| p.canonicalize().ok().as_ref() == Some(&wanted))
}

async fn handle_audit(
    State((hub, state)): State<(WebHub, QualityState)>,
    Json(body): Json<AuditBody>,
) -> impl IntoResponse {
    if hub.readonly {
        ax_usage::log_share(None, "readonly write denied");
        return (
            axum::http::StatusCode::FORBIDDEN,
            Json(json!({ "error": "read-only mode (AX_WEB_READONLY=1)" })),
        )
            .into_response();
    }
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
        if is_session_id(&s) {
            opts.session_id = Some(s);
            opts.window_minutes = None; // full session
        } else if let Some(p) = known_transcript(&root, &s) {
            opts.session_path = Some(p);
        } else {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(json!({ "error": "session must be a session id or one of this project's agent transcripts" })),
            )
                .into_response();
        }
    }
    // Shares the lock with the background refresh: one audit at a time.
    let running = state.0.refresh.lock().await;
    state.0.audits.fetch_add(1, Ordering::SeqCst);
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
        let mut cache = state.0.cache.write().await;
        cache.snap = Some(snap.clone());
        cache.root = PathBuf::from(&snap.project_root);
    }
    drop(running);
    if body.markdown {
        let md = format_markdown_report(&snap);
        return Json(json!({ "snapshot": snap, "markdown": md })).into_response();
    }
    Json(snap).into_response()
}

async fn handle_quality_events(
    State((hub, state)): State<(WebHub, QualityState)>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    ensure_watcher(&hub, &state);
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    async fn hub(dir: &std::path::Path) -> WebHub {
        let db_path = dir.join(".ax").join("ax.db");
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        drop(ax_db::Database::open(&db_path).await.unwrap());
        WebHub::open(dir.to_path_buf(), true, 0).await.unwrap()
    }

    /// With no verbose log (the default), the audit result is reused instead
    /// of rerunning a full audit on every poll.
    #[tokio::test]
    async fn audit_is_cached_without_verbose_logs() {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub(dir.path()).await;
        let state = QualityState::new();
        assert_eq!(latest_verbose_log_mtime_ms(dir.path()), 0);
        refresh_snapshot(&hub, &state, false).await;
        refresh_snapshot(&hub, &state, false).await;
        refresh_snapshot(&hub, &state, false).await;
        assert_eq!(state.audit_runs(), 1);
    }

    /// Callers that arrive while an audit runs wait for it instead of
    /// starting their own.
    #[tokio::test]
    async fn concurrent_refreshes_run_one_audit() {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub(dir.path()).await;
        let state = QualityState::new();
        tokio::join!(
            refresh_snapshot(&hub, &state, false),
            refresh_snapshot(&hub, &state, false),
            refresh_snapshot(&hub, &state, false),
            refresh_snapshot(&hub, &state, false),
        );
        assert_eq!(state.audit_runs(), 1);
    }

    /// Building the router starts nothing; the background watcher starts with
    /// the first quality request.
    #[tokio::test]
    async fn watcher_starts_on_first_request() {
        let dir = tempfile::tempdir().unwrap();
        let hub = hub(dir.path()).await;
        let state = QualityState::new();
        let app = router_with_state(hub, state.clone());
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(!state.watcher_running());
        assert_eq!(state.audit_runs(), 0, "router construction ran an audit");
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/mcp-quality")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(res.status().is_success());
        assert!(state.watcher_running());
    }
}
