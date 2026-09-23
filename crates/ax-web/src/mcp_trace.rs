//! Live `tail -f` of daily `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` for Command Center.

use std::convert::Infallible;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::{
    extract::{Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::get,
    Json, Router,
};
use chrono::{NaiveDate, Utc};
use futures_util::stream::Stream;
use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::workspace_state::WebHub;
use ax_agent::config::load_workspace_config;
use ax_usage::verbose_enabled;

const BATCH_LINES: usize = 400;
const POLL_MS: u64 = 350;

/// Follow this project's verbose log, or the most recently written log among
/// recent workspaces when this project has never produced one (typical when
/// Command Center is on one project but Cursor MCP `--path` is another repo).
pub fn pick_trace_project_root(hub_root: &Path, recent: &[PathBuf]) -> (PathBuf, Option<String>) {
    if project_has_verbose_logs(hub_root) {
        return (hub_root.to_path_buf(), None);
    }
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for cand in recent {
        if same_path(cand, hub_root) {
            continue;
        }
        for (_day, file) in ax_usage::list_dated_log_files(cand) {
            let Ok(meta) = std::fs::metadata(&file) else {
                continue;
            };
            let Ok(mtime) = meta.modified() else {
                continue;
            };
            let take = best.as_ref().map(|(t, _)| mtime > *t).unwrap_or(true);
            if take {
                best = Some((mtime, cand.clone()));
            }
        }
    }
    match best {
        Some((_, p)) => {
            let label = project_label(&p);
            (p, Some(label))
        }
        None => (hub_root.to_path_buf(), None),
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.canonicalize().ok() == b.canonicalize().ok() || a == b
}

fn project_has_verbose_logs(root: &Path) -> bool {
    if ax_usage::current_log_path(Some(root)).is_file() {
        return true;
    }
    !ax_usage::list_dated_log_files(root).is_empty()
}

fn recent_project_paths() -> Vec<PathBuf> {
    load_workspace_config()
        .recent
        .into_iter()
        .map(|r| PathBuf::from(r.path))
        .collect()
}

fn resolve_follow_root(hub_root: &Path) -> (PathBuf, Option<String>) {
    pick_trace_project_root(hub_root, &recent_project_paths())
}

pub fn mcp_verbose_log_path(project_root: &Path) -> PathBuf {
    ax_usage::current_log_path(Some(project_root))
}

fn project_label(project_root: &Path) -> String {
    project_root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| project_root.display().to_string())
}

fn project_meta(hub_root: &Path, follow_root: &Path, fallback_label: Option<&str>) -> serde_json::Value {
    let log_path = mcp_verbose_log_path(follow_root);
    let today = ax_usage::rotation_calendar_date(Some(follow_root), Utc::now());
    json!({
        "path": log_path.display().to_string(),
        "projectRoot": hub_root.display().to_string(),
        "projectLabel": project_label(hub_root),
        "followRoot": follow_root.display().to_string(),
        "followLabel": project_label(follow_root),
        "fallbackLabel": fallback_label,
        "scope": "project",
        "logDay": today.format("%Y-%m-%d").to_string(),
        "logPattern": "mcp-verbose-YYYY-MM-DD.log",
        "logExists": log_path.is_file() || !ax_usage::list_dated_log_files(follow_root).is_empty(),
        "verboseMcp": verbose_enabled(Some(hub_root)),
    })
}

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/mcp-trace/events", get(handle_mcp_trace_events))
        .route("/mcp-trace/chunk", get(handle_mcp_trace_chunk))
        .route("/mcp-trace/path", get(handle_mcp_trace_path))
        .with_state(hub)
}

async fn hub_project(hub: &WebHub) -> PathBuf {
    let ws = hub.read().await;
    ws.project_root.clone()
}

async fn handle_mcp_trace_path(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let hub_root = hub_project(&hub).await;
    let (follow, fallback) = resolve_follow_root(&hub_root);
    Json(project_meta(&hub_root, &follow, fallback.as_deref()))
}

#[derive(Debug, Deserialize)]
struct ChunkQuery {
    day: String,
}

async fn handle_mcp_trace_chunk(
    State(hub): State<WebHub>,
    Query(q): Query<ChunkQuery>,
) -> Json<serde_json::Value> {
    let hub_root = hub_project(&hub).await;
    let (root, _fallback) = resolve_follow_root(&hub_root);
    let before = match NaiveDate::parse_from_str(q.day.trim(), "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            return Json(json!({
                "ok": false,
                "error": "invalid day (use YYYY-MM-DD)",
            }));
        }
    };
    // `day` is an exclusive upper bound: find the nearest existing dated file
    // strictly before it, skipping gaps (verbose logging off for a stretch,
    // or a stale daemon that rotated late) instead of dead-ending on the very
    // next calendar day and reporting "no history" when older data exists.
    let Some((day, has_older)) = ax_usage::nearest_dated_log_before(&root, before) else {
        return Json(json!({
            "ok": true,
            "day": serde_json::Value::Null,
            "lines": Vec::<String>::new(),
            "hasOlder": false,
        }));
    };
    let text = ax_usage::read_log_for_day(&root, day);
    let lines: Vec<String> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect();
    Json(json!({
        "ok": true,
        "day": day.format("%Y-%m-%d").to_string(),
        "lines": lines,
        "hasOlder": has_older,
    }))
}

async fn handle_mcp_trace_events(
    State(hub): State<WebHub>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut offset: u64 = 0;
        let mut pending = String::new();
        let mut following_path = String::new();
        let mut seeded_path = String::new();
        let mut project_following = String::new();

        loop {
            let (path, root, hub_root, fallback) = {
                let ws = hub.read().await;
                let hub_root = ws.project_root.clone();
                let (follow, fallback) = resolve_follow_root(&hub_root);
                (mcp_verbose_log_path(&follow), follow, hub_root, fallback)
            };
            let path_key = path.display().to_string();
            let follow_key = format!("{}|{}", hub_root.display(), root.display());

            if follow_key != project_following {
                project_following = follow_key;
                following_path.clear();
                seeded_path.clear();
                offset = 0;
                pending.clear();
                yield Ok(Event::default().event("reset").data(format!("project {}", hub_root.display())));
            }

            if path_key != following_path {
                let is_rotation = !following_path.is_empty() && seeded_path == following_path;
                following_path = path_key.clone();
                offset = 0;
                pending.clear();
                seeded_path.clear();
                yield Ok(Event::default().event("path").data(path_key.clone()));
                yield Ok(
                    Event::default()
                        .event("project")
                        .data(project_meta(&hub_root, &root, fallback.as_deref()).to_string()),
                );
                if is_rotation {
                    yield Ok(Event::default().event("rotate").data(path_key.clone()));
                }
            }

            if seeded_path != path_key {
                seeded_path = path_key.clone();
                if let Ok(text) = tokio::fs::read_to_string(&path).await {
                    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
                    for chunk in lines.chunks(BATCH_LINES) {
                        let payload = serde_json::to_string(chunk).unwrap_or_else(|_| "[]".into());
                        yield Ok(Event::default().event("batch").data(payload));
                    }
                    offset = tokio::fs::metadata(&path)
                        .await
                        .map(|m| m.len())
                        .unwrap_or(text.len() as u64);
                    yield Ok(Event::default().event("ready").data(format!("following {offset}")));
                } else {
                    yield Ok(Event::default().event("ready").data("waiting for log file"));
                }
            }

            tokio::time::sleep(Duration::from_millis(POLL_MS)).await;
            let Ok(meta) = tokio::fs::metadata(&path).await else {
                continue;
            };
            let len = meta.len();
            if len < offset {
                offset = 0;
                pending.clear();
                yield Ok(Event::default().event("resync").data("log truncated"));
            }
            if len <= offset {
                continue;
            }
            match read_new_chunk(&path, offset).await {
                Ok((chunk, new_offset)) => {
                    offset = new_offset;
                    pending.push_str(&chunk);
                    while let Some(pos) = pending.find('\n') {
                        let mut line = pending[..pos].to_string();
                        if line.ends_with('\r') {
                            line.pop();
                        }
                        pending.drain(..=pos);
                        if !line.is_empty() {
                            yield Ok(Event::default().event("line").data(line));
                        }
                    }
                }
                Err(_) => continue,
            }
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn read_new_chunk(path: &Path, offset: u64) -> Result<(String, u64), std::io::Error> {
    let mut file = tokio::fs::File::open(path).await?;
    file.seek(SeekFrom::Start(offset)).await?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).await?;
    let new_offset = offset + buf.len() as u64;
    let text = String::from_utf8_lossy(&buf).into_owned();
    Ok((text, new_offset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn log_path_is_dated_under_project_ax() {
        let p = mcp_verbose_log_path(Path::new(r"C:\gary\ax"));
        assert!(p.to_string_lossy().contains("mcp-verbose-"));
        assert!(p.to_string_lossy().contains(".ax"));
    }

    #[test]
    fn pick_trace_keeps_hub_when_it_has_logs() {
        let dir = tempfile::tempdir().unwrap();
        let hub = dir.path().join("contoso-hub");
        let other = dir.path().join("ax");
        fs::create_dir_all(hub.join(".ax")).unwrap();
        fs::create_dir_all(other.join(".ax")).unwrap();
        fs::write(hub.join(".ax").join("mcp-verbose-2026-09-01.log"), "hub\n").unwrap();
        fs::write(other.join(".ax").join("mcp-verbose-2026-09-15.log"), "other\n").unwrap();
        let (got, fallback) = pick_trace_project_root(&hub, &[other]);
        assert_eq!(got, hub);
        assert!(fallback.is_none());
    }

    #[test]
    fn pick_trace_falls_back_to_recent_with_logs() {
        let dir = tempfile::tempdir().unwrap();
        let hub = dir.path().join("contoso-hub");
        let other = dir.path().join("ax");
        fs::create_dir_all(hub.join(".ax")).unwrap();
        fs::create_dir_all(other.join(".ax")).unwrap();
        fs::write(other.join(".ax").join("mcp-verbose-2026-09-14.log"), "from-ax\n").unwrap();
        let (got, fallback) = pick_trace_project_root(&hub, &[other.clone()]);
        assert_eq!(got, other);
        assert_eq!(fallback.as_deref(), Some("ax"));
    }
}
