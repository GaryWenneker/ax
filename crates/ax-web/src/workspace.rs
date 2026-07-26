//! Workspace / project directory API — switch, browse, create, init.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use ax_agent::config::{
    default_browse_roots, load_workspace_config, touch_recent_project,
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

fn allowed_browse_roots(project_root: &Path) -> Vec<PathBuf> {
    let mut roots = default_browse_roots();
    roots.push(canonical_or(project_root));
    if let Some(parent) = project_root.parent() {
        roots.push(canonical_or(parent));
    }
    let ws = load_workspace_config();
    for r in ws.browse_roots {
        roots.push(PathBuf::from(r));
    }
    for r in ws.recent {
        roots.push(PathBuf::from(r.path));
    }
    roots.sort();
    roots.dedup();
    roots
}

fn is_path_allowed(path: &Path, project_root: &Path) -> bool {
    let target = canonical_or(path);
    allowed_browse_roots(project_root)
        .iter()
        .any(|root| target.starts_with(root))
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
    let _ = touch_recent_project(&path, is_initialized(&path));
    Json(serde_json::json!({
        "ok": true,
        "workspace": CurrentWorkspace {
            path: path.to_string_lossy().into_owned(),
            label: path_label(&path),
            initialized: is_initialized(&path),
        },
        "recent": load_workspace_config().recent,
    }))
}

async fn handle_recent(State(_hub): State<WebHub>) -> impl IntoResponse {
    Json(serde_json::json!({
        "ok": true,
        "recent": load_workspace_config().recent,
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

    if !is_path_allowed(&base, &project_root) {
        return Json(serde_json::json!({
            "ok": false,
            "error": "Path is outside allowed browse roots"
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

    let mut entries = Vec::new();
    if let Ok(read) = std::fs::read_dir(&base) {
        for ent in read.flatten() {
            let path = ent.path();
            if path.is_dir() {
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
                    path: path.to_string_lossy().into_owned(),
                    is_dir: true,
                    initialized: is_initialized(&path),
                });
            }
        }
    }
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Json(serde_json::json!({
        "ok": true,
        "path": base.to_string_lossy(),
        "parent": base.parent().map(|p| p.to_string_lossy().into_owned()),
        "initialized": is_initialized(&base),
        "entries": entries,
    }))
    .into_response()
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
    {
        let ws = hub.read().await;
        if !is_path_allowed(&target, &ws.project_root) {
            return Json(serde_json::json!({ "ok": false, "error": "Path not allowed" })).into_response();
        }
    }
    let target = canonical_or(&target);
    if !target.is_dir() {
        return Json(serde_json::json!({ "ok": false, "error": "Directory does not exist" })).into_response();
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

async fn handle_mkdir(
    State(hub): State<WebHub>,
    Json(body): Json<MkdirBody>,
) -> impl IntoResponse {
    if hub.readonly {
        return Json(serde_json::json!({ "ok": false, "error": "Read-only mode" })).into_response();
    }
    let ws = hub.read().await;
    let project_root = ws.project_root.clone();
    drop(ws);

    let parent = PathBuf::from(&body.parent);
    if !is_path_allowed(&parent, &project_root) {
        return Json(serde_json::json!({ "ok": false, "error": "Parent path not allowed" })).into_response();
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
    let ws = hub.read().await;
    let project_root = ws.project_root.clone();
    drop(ws);

    let target = PathBuf::from(&body.path);
    if !is_path_allowed(&target, &project_root) {
        return Json(serde_json::json!({ "ok": false, "error": "Path not allowed" })).into_response();
    }
    let target = canonical_or(&target);
    if !target.is_dir() {
        return Json(serde_json::json!({ "ok": false, "error": "Directory does not exist" })).into_response();
    }
    if is_initialized(&target) {
        return Json(serde_json::json!({
            "ok": true,
            "already_initialized": true,
            "path": target.to_string_lossy(),
        }))
        .into_response();
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let path = target.clone();
    tokio::spawn(async move {
        let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("ax"));
        let path_str = path.to_string_lossy().into_owned();
        let mut child = match tokio::process::Command::new(exe)
            .args(["init", &path_str])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
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

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        async fn pump_stdout(
            tx: mpsc::UnboundedSender<String>,
            reader: Option<tokio::process::ChildStdout>,
            prefix: &str,
        ) {
            use tokio::io::{AsyncBufReadExt, BufReader};
            if let Some(out) = reader {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx.send(
                        serde_json::json!({"type":"line","text": format!("{prefix}{line}")}).to_string(),
                    );
                }
            }
        }

        async fn pump_stderr(
            tx: mpsc::UnboundedSender<String>,
            reader: Option<tokio::process::ChildStderr>,
            prefix: &str,
        ) {
            use tokio::io::{AsyncBufReadExt, BufReader};
            if let Some(out) = reader {
                let mut lines = BufReader::new(out).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    let _ = tx.send(
                        serde_json::json!({"type":"line","text": format!("{prefix}{line}")}).to_string(),
                    );
                }
            }
        }

        let tx2 = tx.clone();
        let tx3 = tx.clone();
        let h1 = tokio::spawn(pump_stdout(tx2, stdout, ""));
        let h2 = tokio::spawn(pump_stderr(tx3, stderr, "[stderr] "));
        let status = child.wait().await;
        let _ = h1.await;
        let _ = h2.await;

        let ok = status.map(|s| s.success()).unwrap_or(false);
        if ok {
            let _ = touch_recent_project(&path, true);
        }
        let _ = tx.send(
            serde_json::json!({
                "type": "done",
                "ok": ok,
                "path": path.to_string_lossy(),
                "initialized": ok && is_initialized(&path),
            })
            .to_string(),
        );
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
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

#[derive(Deserialize)]
struct AddRecentBody {
    path: String,
}

async fn handle_add_recent(Json(body): Json<AddRecentBody>) -> impl IntoResponse {
    let path = PathBuf::from(&body.path);
    let abs = canonical_or(&path);
    match touch_recent_project(&abs, is_initialized(&abs)) {
        Ok(()) => Json(serde_json::json!({ "ok": true })).into_response(),
        Err(e) => Json(serde_json::json!({ "ok": false, "error": e })).into_response(),
    }
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
        .with_state(hub)
}
