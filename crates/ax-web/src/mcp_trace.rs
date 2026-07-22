//! Live `tail -f` of `<project>/.ax/mcp-verbose.log` for Command Center.

use std::convert::Infallible;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::Duration;

use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use futures_util::stream::Stream;
use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::workspace_state::WebHub;

const SEED_LINES: usize = 250;
const POLL_MS: u64 = 350;

pub fn mcp_verbose_log_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("mcp-verbose.log")
}

fn project_label(project_root: &Path) -> String {
    project_root
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| project_root.display().to_string())
}

fn project_meta(hub_root: &Path, log_path: &Path) -> serde_json::Value {
    json!({
        "path": log_path.display().to_string(),
        "projectRoot": hub_root.display().to_string(),
        "projectLabel": project_label(hub_root),
        "scope": "project",
    })
}

pub fn router(hub: WebHub) -> Router {
    Router::new()
        .route("/mcp-trace/events", get(handle_mcp_trace_events))
        .route("/mcp-trace/clear", post(handle_mcp_trace_clear))
        .route("/mcp-trace/path", get(handle_mcp_trace_path))
        .with_state(hub)
}

async fn active_log_path(hub: &WebHub) -> PathBuf {
    let ws = hub.read().await;
    mcp_verbose_log_path(&ws.project_root)
}

async fn handle_mcp_trace_path(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let ws = hub.read().await;
    let path = mcp_verbose_log_path(&ws.project_root);
    Json(project_meta(&ws.project_root, &path))
}

async fn handle_mcp_trace_clear(State(hub): State<WebHub>) -> Json<serde_json::Value> {
    let path = active_log_path(&hub).await;
    if let Some(parent) = path.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
    }
    match tokio::fs::write(&path, b"").await {
        Ok(()) => Json(json!({ "ok": true, "path": path.display().to_string() })),
        Err(e) => Json(json!({ "ok": false, "error": e.to_string() })),
    }
}

async fn handle_mcp_trace_events(
    State(hub): State<WebHub>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut offset: u64 = 0;
        let mut pending = String::new();
        let mut following = String::new();
        let mut seeded = false;

        loop {
            let (path, root) = {
                let ws = hub.read().await;
                (mcp_verbose_log_path(&ws.project_root), ws.project_root.clone())
            };
            let path_key = path.display().to_string();

            if path_key != following {
                following = path_key.clone();
                offset = 0;
                pending.clear();
                seeded = false;
                yield Ok(Event::default().event("reset").data(format!("project {path_key}")));
                yield Ok(Event::default().event("path").data(path_key.clone()));
                yield Ok(
                    Event::default()
                        .event("project")
                        .data(project_meta(&root, &path).to_string()),
                );
            }

            if !seeded {
                seeded = true;
                if let Ok(text) = tokio::fs::read_to_string(&path).await {
                    let lines: Vec<&str> = text.lines().collect();
                    let start = lines.len().saturating_sub(SEED_LINES);
                    for line in &lines[start..] {
                        yield Ok(Event::default().event("line").data((*line).to_string()));
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
                yield Ok(Event::default().event("reset").data("log cleared"));
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

async fn read_new_chunk(path: &std::path::Path, offset: u64) -> Result<(String, u64), std::io::Error> {
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
    fn log_path_is_under_project_ax() {
        let p = mcp_verbose_log_path(Path::new(r"C:\gary\ax"));
        assert!(p.ends_with("mcp-verbose.log"));
        assert!(p.to_string_lossy().contains(".ax"));
        assert!(p.to_string_lossy().contains("gary"));
    }
}
