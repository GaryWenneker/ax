//! Memory vault HTTP API for the Command Center.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, patch, post},
    Router,
};
use serde::Deserialize;

use crate::workspace_state::WebHub;
use crate::ApiError;

pub fn router_hub(hub: WebHub) -> Router {
    Router::new()
        .route("/", get(handle_list).post(handle_create))
        .route("/recall", get(handle_recall))
        .route("/review", get(handle_review))
        .route("/health", get(handle_memory_health))
        .route("/embed-status", get(handle_embed_status))
        .route("/capture-git", post(handle_capture_git))
        .route("/{id}/enabled", patch(handle_set_enabled))
        .route("/{id}/file-changes", get(handle_file_changes))
        .route("/{id}/file-links", get(handle_file_links))
        .route("/{id}/related", get(handle_related))
        .route("/{id}/diff", get(handle_diff))
        .route("/{id}/image", get(handle_image))
        .route(
            "/{id}",
            get(handle_get).put(handle_update).delete(handle_delete),
        )
        .with_state(hub)
}

fn err(status: StatusCode, msg: impl Into<String>) -> axum::response::Response {
    (status, Json(ApiError { error: msg.into() })).into_response()
}

fn forbidden_readonly() -> axum::response::Response {
    ax_usage::log_share(None, "readonly write denied");
    err(StatusCode::FORBIDDEN, "read-only mode (AX_WEB_READONLY=1)")
}

async fn handle_memory_health(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    let pool = &ws.graph_pool;
    let memories: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM memories")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let stale: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM memory_anchors WHERE stale = 1 OR orphan = 1")
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    let schema: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM schema_versions")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let backup_dir = ws.project_root.join(".ax").join("backups").join("memory");
    let last_backup = std::fs::read_dir(&backup_dir).ok().and_then(|rd| {
        rd.filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some_and(|ext| ext == "zst" || ext == "jsonl")
            })
            .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
    });
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "schemaVersion": schema,
            "memories": memories,
            "staleOrOrphanAnchors": stale,
            "lastBackup": last_backup,
            "mechanisms": {
                "fts": true,
                "vectors": true,
                "anchors": true,
                "prospective": true,
                "backups": last_backup.is_some(),
            }
        })),
    )
        .into_response()
}

async fn handle_review(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    let pool = &ws.graph_pool;
    let stale: Vec<String> = match sqlx::query_scalar(
        "SELECT DISTINCT memory_id FROM memory_anchors WHERE stale = 1 OR orphan = 1 LIMIT 200",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let foreign: Vec<String> = match sqlx::query_scalar(
        "SELECT id FROM memories WHERE trust_level != 'trusted' AND enabled = 1 LIMIT 200",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    let candidates: Vec<String> = match sqlx::query_scalar(
        "SELECT id FROM memories WHERE source = 'extractor' OR tags LIKE '%candidate%' LIMIT 200",
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "stale": stale,
            "foreign": foreign,
            "candidates": candidates,
        })),
    )
        .into_response()
}

async fn handle_related(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    let ws = hub.read().await;
    let pool = &ws.graph_pool;
    let rows: Result<Vec<(String, String, String)>, _> = sqlx::query_as(
        "SELECT from_id, to_id, kind FROM memory_links
         WHERE from_id = ? OR to_id = ?
         LIMIT 50",
    )
    .bind(&id)
    .bind(&id)
    .fetch_all(pool)
    .await;
    match rows {
        Ok(rows) => {
            let links: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(from_id, to_id, kind)| {
                    let other = if from_id == id { to_id } else { from_id };
                    serde_json::json!({ "id": other, "kind": kind })
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({ "related": links })),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn handle_embed_status() -> Json<serde_json::Value> {
    let status = ax_memory::embed::embed_status();
    static LOGGED: std::sync::Once = std::sync::Once::new();
    LOGGED.call_once(|| {
        ax_usage::log_embed(
            None,
            format!("backend={} model={}", status.backend, status.model),
        );
    });
    Json(serde_json::json!({
        "backend": status.backend,
        "model": status.model,
    }))
}

#[derive(Deserialize)]
struct ListQuery {
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    session: Option<String>,
}

fn default_limit() -> usize {
    50
}

async fn handle_list(State(hub): State<WebHub>, Query(p): Query<ListQuery>) -> impl IntoResponse {
    let ws = hub.read().await;
    let limit = p.limit.min(500);
    let session = p
        .session
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let listed = match session {
        Some(session) => {
            ax_memory::list_for_session(&ws.graph_pool, session, limit, p.offset).await
        }
        None => ax_memory::list(&ws.graph_pool, limit, p.offset).await,
    };
    match listed {
        Ok((memories, total)) => {
            let ids: Vec<String> = memories.iter().map(|memory| memory.id.clone()).collect();
            let sessions = ax_memory::session_ids(&ws.graph_pool, &ids)
                .await
                .unwrap_or_default();
            let memories: Vec<serde_json::Value> = memories
                .into_iter()
                .map(|memory| {
                    let id = memory.id.clone();
                    let mut value =
                        serde_json::to_value(&memory).unwrap_or_else(|_| serde_json::json!({}));
                    if let Some(object) = value.as_object_mut() {
                        let session_id = sessions.get(&id).cloned().unwrap_or_default();
                        object.insert(
                            "session_id".to_string(),
                            serde_json::Value::String(session_id),
                        );
                    }
                    value
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({ "memories": memories, "total": total })),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct RecallQuery {
    q: String,
    #[serde(default = "default_recall_limit")]
    limit: usize,
    #[serde(default)]
    depth: String,
    #[serde(default)]
    session: Option<String>,
    #[serde(default)]
    turn: Option<String>,
    #[serde(default)]
    keyword: Option<String>,
    #[serde(default)]
    trigger: Option<String>,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    since: Option<i64>,
    #[serde(default)]
    until: Option<i64>,
}

fn default_recall_limit() -> usize {
    10
}

async fn handle_recall(
    State(hub): State<WebHub>,
    Query(p): Query<RecallQuery>,
) -> impl IntoResponse {
    let ws = hub.read().await;
    let depth = if p.depth == "archive" {
        ax_memory::Depth::Archive
    } else {
        ax_memory::Depth::Hot
    };
    let filter = ax_memory::MemoryFilter {
        session: p.session,
        turn: p.turn,
        keyword: p.keyword,
        trigger: p.trigger,
        file: p.file,
        symbol: p.symbol,
        kind: p.kind,
        since: p.since,
        until: p.until,
    };
    match ax_memory::search(
        &ws.graph_pool,
        &ax_memory::SearchRequest {
            query: p.q,
            cue: None,
            id: None,
            depth,
            files: Vec::new(),
            limit: p.limit.min(50),
        },
    )
    .await
    {
        Ok(mut matches) => {
            if let Err(e) =
                ax_memory::apply_memory_filter(&ws.graph_pool, &mut matches, &filter).await
            {
                return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({ "matches": matches })),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct MemoryInput {
    #[serde(default)]
    title: String,
    body: String,
    kind: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    files: Vec<String>,
}

impl MemoryInput {
    fn into_remember(self) -> ax_memory::RememberInput {
        ax_memory::RememberInput {
            title: self.title,
            body: self.body,
            kind: self.kind,
            tags: self.tags,
            files: self.files,
            source: Some("manual".into()),
        }
    }
}

async fn handle_create(
    State(hub): State<WebHub>,
    Json(input): Json<MemoryInput>,
) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    match ax_memory::remember(&ws.graph_pool, input.into_remember()).await {
        Ok(row) => {
            let similar = ax_memory::find_similar(
                &ws.graph_pool,
                &format!("{} {}", row.title, row.body),
                Some(&row.id),
                0.80,
                3,
            )
            .await
            .unwrap_or_default();
            (
                StatusCode::OK,
                Json(serde_json::json!({ "memory": row, "similar": similar })),
            )
                .into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn handle_file_changes(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let (row, root) = match memory_and_root(&hub, &id).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    let files = blocking(move || {
        let mut files: std::collections::BTreeMap<String, &'static str> =
            ax_memory::file_changes_from_body(&row.body)
                .into_iter()
                .map(|(path, kind)| (path, kind.as_str()))
                .collect();
        if files.is_empty() {
            for rev in ax_memory::commit_revs(&row.body, &row.id) {
                for (path, kind) in git_name_status(&root, &rev) {
                    files.insert(path, kind.as_str());
                }
            }
        }
        files
    })
    .await;
    match files {
        Ok(files) => (StatusCode::OK, Json(serde_json::json!({ "files": files }))).into_response(),
        Err(r) => *r,
    }
}

async fn handle_file_links(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    let (row, root) = match memory_and_root(&hub, &id).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    match blocking(move || file_links(&root, row)).await {
        Ok(links) => (StatusCode::OK, Json(serde_json::json!({ "files": links }))).into_response(),
        Err(r) => *r,
    }
}

/// Web links for the files of one memory. Runs git, so call it off the
/// async runtime.
fn file_links(
    root: &std::path::Path,
    row: ax_memory::MemoryRow,
) -> std::collections::BTreeMap<String, String> {
    let mut links = std::collections::BTreeMap::new();
    let Some(remote) = git_remote_url(root) else {
        return links;
    };
    let revs = ax_memory::commit_revs(&row.body, &row.id);
    let Some(newest) = revs.last().cloned() else {
        return links;
    };
    let mut kinds: std::collections::BTreeMap<String, ax_memory::FileChangeKind> =
        ax_memory::file_changes_from_body(&row.body)
            .into_iter()
            .collect();
    if kinds.is_empty() {
        for rev in &revs {
            for (path, kind) in git_name_status(root, rev) {
                kinds.insert(path, kind);
            }
        }
    }
    let paths = if row.files.is_empty() {
        files_from_body(&row.body)
    } else {
        row.files
    };
    // A deleted file links to the parent commit. Every path shares the same
    // newest commit, so its parent is looked up at most once.
    let mut parent: Option<String> = None;
    for path in paths {
        if !ax_memory::safe_diff_path(&path) {
            continue;
        }
        let rev = if kinds.get(&path) == Some(&ax_memory::FileChangeKind::Deleted) {
            parent
                .get_or_insert_with(|| git_parent(root, &newest).unwrap_or_else(|| newest.clone()))
                .clone()
        } else {
            newest.clone()
        };
        if let Some(url) = ax_memory::git_web_file_url(&remote, &rev, &path) {
            links.insert(path, url);
        }
    }
    links
}

/// The memory `id` and the project root, read under the workspace lock that
/// is released before the caller runs git.
async fn memory_and_root(
    hub: &WebHub,
    id: &str,
) -> Result<(ax_memory::MemoryRow, std::path::PathBuf), Box<axum::response::Response>> {
    let ws = hub.read().await;
    match ax_memory::get(&ws.graph_pool, id).await {
        Ok(Some(row)) => Ok((row, ws.project_root.clone())),
        Ok(None) => Err(Box::new(err(StatusCode::NOT_FOUND, "memory not found"))),
        Err(e) => Err(Box::new(err(
            StatusCode::INTERNAL_SERVER_ERROR,
            e.to_string(),
        ))),
    }
}

/// Runs synchronous work (git, file reads) on the blocking thread pool so it
/// never stalls the async workers.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, Box<axum::response::Response>> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|e| Box::new(err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())))
}

fn files_from_body(body: &str) -> Vec<String> {
    let before = body.split(ax_memory::OUTCOME_MARKER).next().unwrap_or(body);
    let Some((_, note)) = before.split_once("\n\nFiles:") else {
        return Vec::new();
    };
    let line = note
        .lines()
        .next()
        .unwrap_or("")
        .split(" (+")
        .next()
        .unwrap_or("");
    line.split(',')
        .map(|part| part.trim().to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

fn git_remote_url(root: &std::path::Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(["remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!url.is_empty()).then_some(url)
}

fn git_parent(root: &std::path::Path, rev: &str) -> Option<String> {
    if !(7..=40).contains(&rev.len()) || !rev.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--verify", &format!("{rev}^")])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let parent = String::from_utf8_lossy(&output.stdout).trim().to_string();
    ((7..=40).contains(&parent.len()) && parent.chars().all(|c| c.is_ascii_hexdigit()))
        .then_some(parent)
}

#[derive(Deserialize)]
struct DiffQuery {
    path: Option<String>,
    commit: Option<String>,
}

const DIFF_BYTE_CAP: usize = 2_000_000;

async fn handle_diff(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Query(q): Query<DiffQuery>,
) -> impl IntoResponse {
    let (row, root) = match memory_and_root(&hub, &id).await {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if let Some(path) = q.path.as_deref() {
        if !ax_memory::safe_diff_path(path) {
            return err(StatusCode::BAD_REQUEST, "invalid path");
        }
    }
    let known = ax_memory::commit_revs(&row.body, &row.id);
    let revs: Vec<String> = if let Some(commit) = q.commit.filter(|c| !c.is_empty()) {
        if !known.contains(&commit) {
            return err(StatusCode::NOT_FOUND, "commit is not part of this memory");
        }
        vec![commit]
    } else {
        known
    };
    let path = q.path;
    match blocking(move || memory_diff(&root, &row, &revs, path.as_deref())).await {
        Ok(parsed) => (StatusCode::OK, Json(diff_json(&parsed))).into_response(),
        Err(r) => *r,
    }
}

/// The parsed patch of `revs`, limited to `path` when given. Runs git, so
/// call it off the async runtime.
fn memory_diff(
    root: &std::path::Path,
    row: &ax_memory::MemoryRow,
    revs: &[String],
    path: Option<&str>,
) -> ax_memory::ParsedDiff {
    let mut raw = Vec::new();
    let mut cut = false;
    for rev in revs {
        let Some(chunk) = git_show_patch(root, rev, path) else {
            continue;
        };
        if raw.len() + chunk.len() > DIFF_BYTE_CAP {
            let room = DIFF_BYTE_CAP.saturating_sub(raw.len());
            raw.extend_from_slice(&chunk[..room]);
            cut = true;
            break;
        }
        raw.extend_from_slice(&chunk);
        raw.push(b'\n');
    }
    // A captured turn often has no commit. Show the working tree for a file
    // that belongs to this memory, instead of an empty diff.
    if raw.is_empty() {
        if let Some(path) = path {
            if memory_owns_path(row, path) {
                if let Some(chunk) = git_worktree_patch(root, path) {
                    if chunk.len() > DIFF_BYTE_CAP {
                        raw.extend_from_slice(&chunk[..DIFF_BYTE_CAP]);
                        cut = true;
                    } else {
                        raw.extend_from_slice(&chunk);
                    }
                }
            }
        }
    }
    let text = String::from_utf8_lossy(&raw);
    let mut parsed = ax_memory::parse_unified_diff(&text, ax_memory::DIFF_LINE_CAP);
    if cut {
        parsed.truncated = true;
    }
    parsed
}

fn diff_json(parsed: &ax_memory::ParsedDiff) -> serde_json::Value {
    serde_json::json!({
        "truncated": parsed.truncated,
        "files": parsed.files.iter().map(|file| serde_json::json!({
            "path": file.path,
            "binary": file.binary,
            "truncated": file.truncated,
            "hunks": file.hunks.iter().map(|hunk| serde_json::json!({
                "header": hunk.header,
                "lines": hunk.lines.iter().map(|line| {
                    let mut value = serde_json::json!({
                        "kind": line.kind.as_str(),
                        "text": line.text,
                    });
                    if let Some(n) = line.old_no {
                        value["old"] = serde_json::json!(n);
                    }
                    if let Some(n) = line.new_no {
                        value["new"] = serde_json::json!(n);
                    }
                    value
                }).collect::<Vec<_>>()
            })).collect::<Vec<_>>()
        })).collect::<Vec<_>>()
    })
}

fn git_show_patch(root: &std::path::Path, rev: &str, path: Option<&str>) -> Option<Vec<u8>> {
    if !(7..=40).contains(&rev.len()) || !rev.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut cmd = std::process::Command::new("git");
    cmd.current_dir(root).args([
        "show",
        "--format=",
        "--unified=3",
        "--no-color",
        "--no-ext-diff",
        rev,
    ]);
    if let Some(path) = path {
        cmd.arg("--").arg(path);
    }
    match cmd.output() {
        Ok(output) if output.status.success() => Some(output.stdout),
        _ => None,
    }
}

fn memory_owns_path(row: &ax_memory::MemoryRow, path: &str) -> bool {
    row.files.iter().any(|file| file == path)
        || ax_memory::file_changes_from_body(&row.body)
            .iter()
            .any(|(file, _)| file == path)
}

fn git_worktree_patch(root: &std::path::Path, path: &str) -> Option<Vec<u8>> {
    let output = std::process::Command::new("git")
        .current_dir(root)
        .args([
            "diff",
            "--unified=3",
            "--no-color",
            "--no-ext-diff",
            "HEAD",
            "--",
            path,
        ])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    if !output.stdout.is_empty() {
        return Some(output.stdout);
    }
    if !git_untracked(root, path) {
        return None;
    }
    added_file_patch(root, path)
}

fn git_untracked(root: &std::path::Path, path: &str) -> bool {
    let Ok(output) = std::process::Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain", "--", path])
        .output()
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.starts_with("??"))
}

fn added_file_patch(root: &std::path::Path, path: &str) -> Option<Vec<u8>> {
    let full = root.join(path);
    let root = root.canonicalize().ok()?;
    let canon = full.canonicalize().ok()?;
    if !canon.starts_with(&root) {
        return None;
    }
    let bytes = std::fs::read(&canon).ok()?;
    let header = format!("diff --git a/{path} b/{path}\n");
    if bytes.contains(&0) || bytes.len() > DIFF_BYTE_CAP {
        return Some(format!("{header}Binary files a/{path} and b/{path} differ\n").into_bytes());
    }
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let mut out = format!(
        "{header}new file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{} @@\n",
        lines.len()
    );
    for line in lines {
        out.push('+');
        out.push_str(line);
        out.push('\n');
    }
    Some(out.into_bytes())
}

fn git_name_status(root: &std::path::Path, rev: &str) -> Vec<(String, ax_memory::FileChangeKind)> {
    if !(7..=40).contains(&rev.len()) || !rev.chars().all(|c| c.is_ascii_hexdigit()) {
        return Vec::new();
    }
    let output = match std::process::Command::new("git")
        .current_dir(root)
        .args(["show", "-z", "--name-status", "--format=", rev])
        .output()
    {
        Ok(output) if output.status.success() => output.stdout,
        _ => return Vec::new(),
    };
    ax_memory::parse_git_name_status(&output)
}

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;

type ImageRefusal = (StatusCode, &'static str);

/// The file behind `src` and its content type, if `src` is an image target written in `body`.
fn memory_image_file(
    body: &str,
    src: &str,
) -> Result<(std::path::PathBuf, &'static str), ImageRefusal> {
    let path = src.strip_prefix("file://").unwrap_or(src);
    let bytes = path.as_bytes();
    let unix = path.starts_with('/') && !path.starts_with("//");
    let windows = bytes.len() > 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    if !unix && !windows {
        return Err((StatusCode::BAD_REQUEST, "image path must be absolute"));
    }
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    let content_type = match ext.as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => return Err((StatusCode::BAD_REQUEST, "not a supported image type")),
    };
    if !body.contains(&format!("]({src})")) {
        return Err((StatusCode::NOT_FOUND, "image is not part of this memory"));
    }
    Ok((std::path::PathBuf::from(path), content_type))
}

fn read_image_file(path: &std::path::Path, max: u64) -> Result<Vec<u8>, ImageRefusal> {
    let meta =
        std::fs::metadata(path).map_err(|_| (StatusCode::NOT_FOUND, "image file not found"))?;
    if meta.len() > max {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, "image file is too large"));
    }
    std::fs::read(path).map_err(|_| (StatusCode::NOT_FOUND, "image file not found"))
}

#[derive(Deserialize)]
struct ImageQuery {
    src: String,
}

async fn handle_image(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Query(q): Query<ImageQuery>,
) -> impl IntoResponse {
    let body = {
        let ws = hub.read().await;
        match ax_memory::get(&ws.graph_pool, &id).await {
            Ok(Some(row)) => row.body,
            Ok(None) => return err(StatusCode::NOT_FOUND, "memory not found"),
            Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        }
    };
    let (path, content_type) = match memory_image_file(&body, &q.src) {
        Ok(found) => found,
        Err((status, msg)) => return err(status, msg),
    };
    match tokio::task::spawn_blocking(move || read_image_file(&path, MAX_IMAGE_BYTES)).await {
        Ok(Ok(bytes)) => (
            StatusCode::OK,
            [
                (axum::http::header::CONTENT_TYPE, content_type),
                (axum::http::header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
                (axum::http::header::CACHE_CONTROL, "private, max-age=300"),
            ],
            bytes,
        )
            .into_response(),
        Ok(Err((status, msg))) => err(status, msg),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn handle_get(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    let ws = hub.read().await;
    match ax_memory::get(&ws.graph_pool, &id).await {
        Ok(Some(row)) => (StatusCode::OK, Json(row)).into_response(),
        Ok(None) => err(StatusCode::NOT_FOUND, "memory not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn handle_update(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Json(input): Json<MemoryInput>,
) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    match ax_memory::update(&ws.graph_pool, &id, input.into_remember()).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "memory not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn handle_delete(State(hub): State<WebHub>, Path(id): Path<String>) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    match ax_memory::delete(&ws.graph_pool, &id).await {
        Ok(true) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "memory not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct EnabledPayload {
    enabled: bool,
}

async fn handle_set_enabled(
    State(hub): State<WebHub>,
    Path(id): Path<String>,
    Json(payload): Json<EnabledPayload>,
) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    match ax_memory::set_enabled(&ws.graph_pool, &id, payload.enabled).await {
        Ok(true) => (
            StatusCode::OK,
            Json(serde_json::json!({ "ok": true, "id": id, "enabled": payload.enabled })),
        )
            .into_response(),
        Ok(false) => err(StatusCode::NOT_FOUND, "memory not found"),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[derive(Deserialize)]
struct CaptureGitInput {
    #[serde(default = "default_capture_limit")]
    limit: usize,
}

fn default_capture_limit() -> usize {
    100
}

async fn handle_capture_git(
    State(hub): State<WebHub>,
    Json(input): Json<CaptureGitInput>,
) -> impl IntoResponse {
    if hub.readonly {
        return forbidden_readonly();
    }
    let ws = hub.read().await;
    match ax_memory::capture_git_history(&ws.graph_pool, &ws.project_root, input.limit).await {
        Ok(result) => (StatusCode::OK, Json(result)).into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = "Done.\n\n![shot](/tmp/ax/page.png)\n\nSee [log](/tmp/ax/run.PNG) and ![w](file:///tmp/ax/w.webp)\n![c](C:\\shots\\a.jpeg)";

    #[test]
    fn an_image_target_written_in_the_memory_is_served_with_its_type() {
        assert_eq!(
            memory_image_file(BODY, "/tmp/ax/page.png"),
            Ok((std::path::PathBuf::from("/tmp/ax/page.png"), "image/png"))
        );
        assert_eq!(
            memory_image_file(BODY, "/tmp/ax/run.PNG").unwrap().1,
            "image/png"
        );
        assert_eq!(
            memory_image_file(BODY, "file:///tmp/ax/w.webp"),
            Ok((std::path::PathBuf::from("/tmp/ax/w.webp"), "image/webp"))
        );
        assert_eq!(
            memory_image_file(BODY, "C:\\shots\\a.jpeg").unwrap().1,
            "image/jpeg"
        );
    }

    #[test]
    fn a_path_not_written_as_a_target_in_the_memory_is_refused() {
        assert_eq!(
            memory_image_file(BODY, "/tmp/ax/other.png").unwrap_err().0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            memory_image_file(BODY, "/tmp/ax/../ax/page.png")
                .unwrap_err()
                .0,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            memory_image_file("text mentions /tmp/ax/page.png only", "/tmp/ax/page.png")
                .unwrap_err()
                .0,
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn relative_paths_svg_and_non_images_are_refused() {
        let body = "![a](shots/a.png) ![b](/tmp/b.svg) ![c](/etc/passwd) ![d](/tmp/d.txt)";
        for src in ["shots/a.png", "/tmp/b.svg", "/etc/passwd", "/tmp/d.txt"] {
            assert_eq!(
                memory_image_file(body, src).unwrap_err().0,
                StatusCode::BAD_REQUEST,
                "{src}"
            );
        }
    }

    #[test]
    fn a_protocol_relative_path_is_not_absolute() {
        let body = "![a](//host/share/a.png)";
        assert_eq!(
            memory_image_file(body, "//host/share/a.png").unwrap_err().0,
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn an_image_file_is_read_up_to_the_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\nrest").unwrap();
        assert_eq!(
            read_image_file(&png, 100).unwrap(),
            b"\x89PNG\r\n\x1a\nrest"
        );
        assert_eq!(
            read_image_file(&png, 5).unwrap_err().0,
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            read_image_file(&dir.path().join("gone.png"), 100)
                .unwrap_err()
                .0,
            StatusCode::NOT_FOUND
        );
    }
}
