//! Workspace / project directory API — switch, browse, create, init.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use ax_agent::config::{
    default_browse_roots, forget_recent_project, load_workspace_config, projects_for_switcher,
    touch_recent_project, RecentProject,
};
use axum::extract::{Query, State};
use axum::response::{
    sse::{Event, KeepAlive, Sse},
    IntoResponse, Json,
};
use axum::routing::{get, post};
use axum::Router;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::workspace_state::WebHub;

#[derive(Serialize)]
struct CurrentWorkspace {
    path: String,
    label: String,
    initialized: bool,
}

#[derive(Serialize)]
struct BrowseEntry {
    name: String,
    path: String,
    is_dir: bool,
    initialized: bool,
}

fn path_label(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string()
}

fn is_initialized(path: &Path) -> bool {
    path.join(".ax").join("ax.db").exists()
}

fn canonical_or(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

/// Strip Windows `\\?\` extended-length prefix for UI / clients.
pub(crate) fn display_path(path: &Path) -> String {
    let s = path.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_owned()
}

/// Any folder on this computer may be browsed, created, initialised or
/// switched to: the project picker walks the whole disk. Who may do so is
/// decided by [`local_or_signed_in`]. Paths must be absolute, so nothing
/// resolves against the server's working directory.
fn is_path_allowed(path: &Path) -> bool {
    path.is_absolute()
}

fn safe_join(parent: &Path, name: &str) -> Result<PathBuf, String> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
        || name == "."
    {
        return Err("Invalid directory name".into());
    }
    Ok(parent.join(name))
}

async fn handle_current(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    let path = canonical_or(&ws.project_root);
    drop(ws);
    // A read leaves the recent list alone; opening and switching update it.
    let recent = switcher_projects().await;
    Json(serde_json::json!({
        "ok": true,
        "workspace": CurrentWorkspace {
            path: display_path(&path),
            label: path_label(&path),
            initialized: is_initialized(&path),
        },
        "recent": recent,
    }))
}

async fn switcher_projects() -> Vec<RecentProject> {
    // The home scan is blocking directory IO. A panicked scan still leaves the saved recent list.
    match tokio::task::spawn_blocking(projects_for_switcher).await {
        Ok(list) => list,
        Err(_) => load_workspace_config().recent,
    }
}

async fn handle_recent(State(_hub): State<WebHub>) -> impl IntoResponse {
    Json(serde_json::json!({
        "ok": true,
        "recent": switcher_projects().await,
        "browse_roots": default_browse_roots().iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>(),
    }))
}

#[derive(Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

async fn handle_browse(
    State(hub): State<WebHub>,
    Query(q): Query<BrowseQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let project_root = ws.project_root.clone();
    drop(ws);

    let base = q
        .path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| canonical_or(&project_root));

    if !is_path_allowed(&base) {
        return Json(serde_json::json!({
            "ok": false,
            "error": "Path must be absolute"
        }))
        .into_response();
    }

    let base = canonical_or(&base);
    if !base.is_dir() {
        return Json(serde_json::json!({
            "ok": false,
            "error": "Not a directory"
        }))
        .into_response();
    }

    // Listing a folder and probing each entry is blocking IO.
    let scan = base.clone();
    let entries = tokio::task::spawn_blocking(move || browse_entries(&scan))
        .await
        .unwrap_or_default();

    Json(serde_json::json!({
        "ok": true,
        "path": display_path(&base),
        "parent": base.parent().map(display_path),
        "initialized": is_initialized(&base),
        "entries": entries,
    }))
    .into_response()
}

/// The non-hidden subfolders of `base`, ax projects first.
fn browse_entries(base: &Path) -> Vec<BrowseEntry> {
    let mut entries = Vec::new();
    if let Ok(read) = std::fs::read_dir(base) {
        for ent in read.flatten() {
            // Prefer DirEntry::file_type — includes empty dirs and avoids
            // extra metadata probes that can fail on locked Windows folders.
            let is_dir = ent
                .file_type()
                .map(|t| t.is_dir())
                .unwrap_or_else(|_| ent.path().is_dir());
            if !is_dir {
                continue;
            }
            let path = ent.path();
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.'))
                .unwrap_or(false)
            {
                continue;
            }
            entries.push(BrowseEntry {
                name: path_label(&path),
                path: display_path(&path),
                is_dir: true,
                initialized: is_initialized(&path),
            });
        }
    }
    entries.sort_by(|a, b| {
        // Ax projects first, then alphabetical — easier to spot in the picker.
        b.initialized
            .cmp(&a.initialized)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    entries
}

#[derive(Deserialize)]
struct SwitchBody {
    path: String,
}

async fn handle_switch(
    State(hub): State<WebHub>,
    Json(body): Json<SwitchBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    let target = PathBuf::from(&body.path);
    if !is_path_allowed(&target) {
        return Json(serde_json::json!({ "ok": false, "error": "Path must be absolute" }))
            .into_response();
    }
    let target = canonical_or(&target);
    if !target.is_dir() {
        return Json(serde_json::json!({ "ok": false, "error": "Directory does not exist" }))
            .into_response();
    }
    if !is_initialized(&target) {
        return Json(serde_json::json!({
            "ok": false,
            "error": "Project not initialized — run init first",
            "needs_init": true,
            "path": target.to_string_lossy(),
        }))
        .into_response();
    }

    match hub.switch(target.clone()).await {
        Ok(info) => Json(serde_json::json!({
            "ok": true,
            "path": info.path,
            "label": info.label,
            "switched": true,
            "url": format!("http://127.0.0.1:{}", hub.port),
        }))
        .into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

#[derive(Deserialize)]
struct MkdirBody {
    parent: String,
    name: String,
}

async fn handle_mkdir(State(hub): State<WebHub>, Json(body): Json<MkdirBody>) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    let parent = PathBuf::from(&body.parent);
    if !is_path_allowed(&parent) {
        return Json(serde_json::json!({ "ok": false, "error": "Parent path must be absolute" }))
            .into_response();
    }
    let parent = canonical_or(&parent);
    let new_path = match safe_join(&parent, &body.name) {
        Ok(p) => p,
        Err(e) => return Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    };
    if new_path.exists() {
        return Json(serde_json::json!({
            "ok": true,
            "path": new_path.to_string_lossy(),
            "created": false,
            "initialized": is_initialized(&new_path),
        }))
        .into_response();
    }
    match std::fs::create_dir_all(&new_path) {
        Ok(()) => Json(serde_json::json!({
            "ok": true,
            "path": new_path.to_string_lossy(),
            "created": true,
            "initialized": false,
        }))
        .into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e.to_string() })).into_response(),
    }
}

#[derive(Deserialize)]
struct InitBody {
    path: String,
}

async fn handle_init_stream(
    State(hub): State<WebHub>,
    Json(body): Json<InitBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    let target = PathBuf::from(&body.path);
    if !is_path_allowed(&target) {
        return Json(serde_json::json!({ "ok": false, "error": "Path must be absolute" }))
            .into_response();
    }
    let target = canonical_or(&target);
    if !target.is_dir() {
        return Json(serde_json::json!({ "ok": false, "error": "Directory does not exist" }))
            .into_response();
    }
    if is_initialized(&target) {
        return Json(serde_json::json!({
            "ok": true,
            "already_initialized": true,
            "path": target.to_string_lossy(),
        }))
        .into_response();
    }

    let Some(run) = InitRun::start(&target) else {
        return Json(serde_json::json!({
            "ok": false,
            "error": "ax init is already running for this folder",
        }))
        .into_response();
    };
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let path = target.clone();
    tokio::spawn(async move {
        let _run = run;
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ax"));
        let path_str = path.to_string_lossy().into_owned();
        let mut child = match tokio::process::Command::new(exe)
            .args(["init", &path_str])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(
                    serde_json::json!({"type":"done","ok":false,"error":e.to_string()}).to_string(),
                );
                return;
            }
        };

        let h1 = child
            .stdout
            .take()
            .map(|out| tokio::spawn(pump_lines(out, "", tx.clone())));
        let h2 = child
            .stderr
            .take()
            .map(|err| tokio::spawn(pump_lines(err, "[stderr] ", tx.clone())));
        // Stop the child when the browser goes away or it runs too long.
        let outcome = tokio::select! {
            status = child.wait() => status.map(|s| s.success()).map_err(|e| e.to_string()),
            _ = tx.closed() => Err("the browser closed the stream".to_string()),
            _ = tokio::time::sleep(INIT_TIMEOUT) => Err("ax init took too long".to_string()),
        };
        if outcome.is_err() {
            let _ = child.kill().await;
        }
        finish_pumps([h1, h2].into_iter().flatten(), PUMP_GRACE).await;

        let ok = outcome.as_ref().is_ok_and(|ok| *ok);
        if ok {
            let _ = touch_recent_project(&path, true);
        }
        let mut done = serde_json::json!({
            "type": "done",
            "ok": ok,
            "path": path.to_string_lossy(),
            "initialized": ok && is_initialized(&path),
        });
        if let Err(error) = outcome {
            done["error"] = serde_json::json!(error);
        }
        let _ = tx.send(done.to_string());
    });

    let stream = async_stream::stream! {
        while let Some(data) = rx.recv().await {
            let is_done = data.contains("\"type\":\"done\"");
            yield Ok::<Event, std::convert::Infallible>(Event::default().data(data));
            if is_done {
                break;
            }
        }
    };
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

#[derive(Deserialize)]
struct AddRecentBody {
    path: String,
}

async fn handle_add_recent(
    State(hub): State<WebHub>,
    Json(body): Json<AddRecentBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    let path = PathBuf::from(&body.path);
    let abs = canonical_or(&path);
    match touch_recent_project(&abs, is_initialized(&abs)) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
}

#[derive(Deserialize)]
struct PurgeQuery {
    path: String,
}

#[derive(Deserialize)]
struct PurgeBody {
    path: String,
    groups: Vec<String>,
}

enum PurgeLookup {
    Present(PathBuf),
    Missing(PathBuf),
}

fn resolve_purge_target(raw: &str) -> Result<PurgeLookup, String> {
    let target = PathBuf::from(raw);
    if !is_path_allowed(&target) {
        return Err("Path must be absolute".into());
    }
    let target = canonical_or(&target);
    if target.is_dir() {
        Ok(PurgeLookup::Present(target))
    } else {
        Ok(PurgeLookup::Missing(target))
    }
}

fn missing_folder_plan(target: &Path) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "missing": true,
        "path": display_path(target),
        "label": path_label(target),
        "current": false,
        "note": crate::project_purge::MISSING_FOLDER_NOTE,
        "groups": crate::project_purge::leftover_groups(),
    }))
}

async fn handle_purge_plan(
    State(hub): State<WebHub>,
    Query(q): Query<PurgeQuery>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    let hub_root = hub.read().await.project_root.clone();
    let target = match resolve_purge_target(&q.path) {
        Ok(PurgeLookup::Present(path)) => path,
        Ok(PurgeLookup::Missing(path)) => return missing_folder_plan(&path).into_response(),
        Err(error) => {
            return Json(serde_json::json!({ "ok": false, "error": error })).into_response();
        }
    };
    let current = canonical_or(&hub_root) == target;
    let mut groups = serde_json::to_value(crate::project_purge::file_groups(&target))
        .unwrap_or_else(|_| serde_json::json!([]));
    if let Some(list) = groups.as_array_mut() {
        if let Ok(serde_json::Value::Array(extra)) =
            serde_json::to_value(crate::project_purge::leftover_groups())
        {
            list.extend(extra);
        }
    }
    Json(serde_json::json!({
        "ok": true,
        "path": display_path(&target),
        "label": path_label(&target),
        "current": current,
        "note": "The repository source stays on disk. Checked items are removed. Uncheck anything that should stay.",
        "groups": groups,
    }))
    .into_response()
}

async fn handle_purge(State(hub): State<WebHub>, Json(body): Json<PurgeBody>) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    if body.groups.is_empty() {
        return Json(serde_json::json!({ "ok": false, "error": "Select at least one item" }))
            .into_response();
    }
    let (target, present) = match resolve_purge_target(&body.path) {
        Ok(PurgeLookup::Present(path)) => (path, true),
        Ok(PurgeLookup::Missing(path)) => (path, false),
        Err(error) => {
            return Json(serde_json::json!({ "ok": false, "error": error })).into_response();
        }
    };

    let wants_database = present && body.groups.iter().any(|id| id == "database");
    let disconnected = if wants_database {
        hub.close_project_if_current(&target).await
    } else {
        false
    };

    let mut report = if present {
        // A recursive delete is blocking IO.
        let (dir, groups) = (target.clone(), body.groups.clone());
        match tokio::task::spawn_blocking(move || {
            crate::project_purge::remove_selected(&dir, &groups)
        })
        .await
        {
            Ok(report) => report,
            Err(_) => crate::project_purge::PurgeReport {
                removed: Vec::new(),
                errors: vec!["the purge stopped unexpectedly".into()],
            },
        }
    } else {
        crate::project_purge::PurgeReport {
            removed: Vec::new(),
            errors: Vec::new(),
        }
    };

    let mut forgot_recent = false;
    if body.groups.iter().any(|id| id == "recent") {
        match forget_recent_project(&target) {
            Ok(removed) => forgot_recent = removed,
            Err(err) => report.errors.push(err),
        }
    }
    if body.groups.iter().any(|id| id == "globalIndex") {
        if let Err(err) = forget_global_project_row(&target).await {
            report.errors.push(err);
        }
    }

    Json(serde_json::json!({
        "ok": report.errors.is_empty(),
        "path": display_path(&target),
        "removed": report.removed,
        "errors": report.errors,
        "disconnected": disconnected,
        "forgotRecent": forgot_recent,
    }))
    .into_response()
}

async fn forget_global_project_row(path: &Path) -> Result<(), String> {
    // The configured global database (AX_GLOBAL_DB wins over ~/.ax).
    let Ok(db) = ax_global_db::global_db_path() else {
        return Ok(());
    };
    if !db.is_file() {
        return Ok(());
    }
    let pool = ax_global_db::open_pool(&db, false)
        .await
        .map_err(|err| format!("global.db: {err:#}"))?;
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let result = sqlx::query("DELETE FROM projects WHERE path = ?1 OR path = ?2")
        .bind(canonical.to_string_lossy().to_string())
        .bind(path.to_string_lossy().to_string())
        .execute(&pool)
        .await;
    pool.close().await;
    result
        .map(|_| ())
        .map_err(|err| format!("global.db: {err}"))
}

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/current", get(handle_current))
        .route("/recent", get(handle_recent))
        .route("/browse", get(handle_browse))
        .route("/switch", post(handle_switch))
        .route("/mkdir", post(handle_mkdir))
        .route("/init/stream", post(handle_init_stream))
        .route("/recent/add", post(handle_add_recent))
        .route("/purge-plan", get(handle_purge_plan))
        .route("/purge", post(handle_purge))
        .layer(axum::middleware::from_fn_with_state(
            hub.clone(),
            local_or_signed_in,
        ))
        .with_state(hub)
}

/// These routes read and change folders anywhere on this computer. They
/// serve a member who passed web sign-in (as Admin or Owner, see
/// [`crate::web_auth::gate`]) or, with sign-in off, only the browser of the
/// person at this computer on the loopback address.
async fn local_or_signed_in(
    State(hub): State<WebHub>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let signed_in = req
        .extensions()
        .get::<crate::web_auth::SignedIn>()
        .is_some();
    if signed_in || crate::local_owner::browser_serves(req.headers(), hub.port) {
        return next.run(req).await;
    }
    (
        axum::http::StatusCode::FORBIDDEN,
        Json(serde_json::json!({
            "ok": false,
            "error": "Open the Command Center on this computer or sign in"
        })),
    )
        .into_response()
}

/// Sends each output line of `reader` to `tx` as an SSE line event.
async fn pump_lines<R>(reader: R, prefix: &str, tx: mpsc::UnboundedSender<String>)
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::{AsyncBufReadExt, BufReader};
    // Read bytes, not `lines()`: a line that is not UTF-8 would end the
    // pump, the pipe would fill and `ax init` would block for good.
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                let line = line.trim_end_matches(['\n', '\r']);
                // A closed stream (the browser left) still drains the pipe.
                let _ = tx.send(
                    serde_json::json!({"type":"line","text": format!("{prefix}{line}")})
                        .to_string(),
                );
            }
        }
    }
}

/// Waits for the output pumps once `ax init` has exited or been killed.
/// A grandchild that inherited stdout or stderr can keep a pipe open after
/// the child is gone, so the pumps share `grace` and are then aborted: the run
/// must always finish, free its folder and send `done`.
async fn finish_pumps(
    pumps: impl IntoIterator<Item = tokio::task::JoinHandle<()>>,
    grace: std::time::Duration,
) {
    let deadline = tokio::time::Instant::now() + grace;
    for mut pump in pumps {
        if tokio::time::timeout_at(deadline, &mut pump).await.is_err() {
            pump.abort();
        }
    }
}

/// How long the output pumps may keep reading after `ax init` has ended.
const PUMP_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Folders with an `ax init` running from the Command Center.
static INITS: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// Marks `path` as being initialized until dropped.
struct InitRun(PathBuf);

impl InitRun {
    fn start(path: &Path) -> Option<Self> {
        let mut running = INITS.lock().unwrap_or_else(|e| e.into_inner());
        if running.iter().any(|p| p == path) {
            return None;
        }
        running.push(path.to_path_buf());
        Some(Self(path.to_path_buf()))
    }
}

impl Drop for InitRun {
    fn drop(&mut self) {
        INITS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|p| p != &self.0);
    }
}

/// The longest an `ax init` started from the Command Center may run.
const INIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60 * 60);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn init_output_keeps_flowing_after_a_line_that_is_not_utf8() {
        let output: &[u8] = b"first\n\xff\xfe broken\nthird\n";
        let (tx, mut rx) = mpsc::unbounded_channel();
        pump_lines(output, "", tx).await;
        let mut texts = Vec::new();
        while let Ok(event) = rx.try_recv() {
            let value: serde_json::Value = serde_json::from_str(&event).unwrap();
            texts.push(value["text"].as_str().unwrap().to_string());
        }
        assert_eq!(texts.len(), 3, "{texts:?}");
        assert_eq!(texts[0], "first");
        assert!(texts[1].ends_with(" broken"), "{texts:?}");
        assert_eq!(texts[2], "third");
    }

    #[tokio::test]
    async fn a_pipe_held_open_by_a_grandchild_does_not_keep_init_running() {
        // The writer half stays alive, as a pipe inherited by a grandchild
        // of `ax init` would: the pump never sees EOF.
        let (_writer, reader) = tokio::io::duplex(64);
        let (tx, _rx) = mpsc::unbounded_channel();
        let pump = tokio::spawn(pump_lines(reader, "", tx));
        let finished = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            finish_pumps([pump], std::time::Duration::from_millis(50)),
        )
        .await;
        assert!(finished.is_ok(), "the pumps were never released");
    }

    #[test]
    fn a_second_init_of_the_same_folder_is_refused_while_one_runs() {
        let dir = tempfile::tempdir().unwrap();
        let first = InitRun::start(dir.path()).expect("first init");
        assert!(
            InitRun::start(dir.path()).is_none(),
            "two inits of one folder"
        );
        drop(first);
        assert!(
            InitRun::start(dir.path()).is_some(),
            "the folder is free again"
        );
    }

    #[test]
    fn only_absolute_paths_are_accepted() {
        let root = std::env::temp_dir();
        assert!(is_path_allowed(&root));
        assert!(!is_path_allowed(Path::new("relative/folder")));
        assert!(!is_path_allowed(Path::new(".")));
        assert!(!is_path_allowed(Path::new("")));
    }
}
