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

const BATCH_LINES: usize = 400;
const POLL_MS: u64 = 350;

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

fn project_meta(hub_root: &Path, log_path: &Path) -> serde_json::Value {
    let today = ax_usage::rotation_calendar_date(Some(hub_root), Utc::now());
    json!({
        "path": log_path.display().to_string(),
        "projectRoot": hub_root.display().to_string(),
        "projectLabel": project_label(hub_root),
        "scope": "project",
        "logDay": today.format("%Y-%m-%d").to_string(),
        "logPattern": "mcp-verbose-YYYY-MM-DD.log",
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
    let ws = hub.read().await;
    let path = mcp_verbose_log_path(&ws.project_root);
    Json(project_meta(&ws.project_root, &path))
}

#[derive(Debug, Deserialize)]
struct ChunkQuery {
    day: String,
}

async fn handle_mcp_trace_chunk(
    State(hub): State<WebHub>,
    Query(q): Query<ChunkQuery>,
) -> Json<serde_json::Value> {
    let root = hub_project(&hub).await;
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
            let (path, root) = {
                let ws = hub.read().await;
                (mcp_verbose_log_path(&ws.project_root), ws.project_root.clone())
            };
            let path_key = path.display().to_string();
            let root_key = root.display().to_string();

            if root_key != project_following {
                project_following = root_key.clone();
                following_path.clear();
                seeded_path.clear();
                offset = 0;
                pending.clear();
                yield Ok(Event::default().event("reset").data(format!("project {root_key}")));
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
                        .data(project_meta(&root, &path).to_string()),
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
    use std::path::Path;

    #[test]
    fn log_path_is_dated_under_project_ax() {
        let p = mcp_verbose_log_path(Path::new(r"C:\gary\ax"));
        assert!(p.to_string_lossy().contains("mcp-verbose-"));
        assert!(p.to_string_lossy().contains(".ax"));
    }
}
