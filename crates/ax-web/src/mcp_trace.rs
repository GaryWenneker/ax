//! Live `tail -f` of daily `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` for Command Center.

use std::convert::Infallible;
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axum::{
    extract::{Query, State},
    http::HeaderMap,
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
/// How long the choice of which project's log to follow is reused.
const FOLLOW_REFRESH: Duration = Duration::from_secs(5);
/// How much of today's log a newly connected client is sent, and the most
/// read from any log in one poll.
const SEED_MAX_BYTES: u64 = 4 * 1024 * 1024;

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

/// The project whose log to follow. Following another recent project is only
/// for the local owner's own writable Command Center (`allow_fallback`); a
/// read-only (share) viewer or any other caller sees only this project's log.
fn resolve_follow_root(hub_root: &Path, allow_fallback: bool) -> (PathBuf, Option<String>) {
    if !allow_fallback {
        return (hub_root.to_path_buf(), None);
    }
    pick_trace_project_root(hub_root, &recent_project_paths())
}

fn fallback_allowed(hub: &WebHub, headers: &HeaderMap) -> bool {
    !hub.readonly && crate::local_owner::browser_serves(headers, hub.port)
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

fn project_meta(
    hub_root: &Path,
    follow_root: &Path,
    fallback_label: Option<&str>,
) -> serde_json::Value {
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

async fn handle_mcp_trace_path(
    State(hub): State<WebHub>,
    headers: HeaderMap,
) -> Json<serde_json::Value> {
    let hub_root = hub_project(&hub).await;
    let (follow, fallback) = resolve_follow_root(&hub_root, fallback_allowed(&hub, &headers));
    Json(project_meta(&hub_root, &follow, fallback.as_deref()))
}

#[derive(Debug, Deserialize)]
struct ChunkQuery {
    day: String,
}

async fn handle_mcp_trace_chunk(
    State(hub): State<WebHub>,
    headers: HeaderMap,
    Query(q): Query<ChunkQuery>,
) -> Json<serde_json::Value> {
    let hub_root = hub_project(&hub).await;
    let (root, _fallback) = resolve_follow_root(&hub_root, fallback_allowed(&hub, &headers));
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
    headers: HeaderMap,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let allow_fallback = fallback_allowed(&hub, &headers);
    let stream = async_stream::stream! {
        let mut offset: u64 = 0;
        let mut pending = String::new();
        let mut following_path = String::new();
        let mut seeded_path = String::new();
        let mut project_following = String::new();
        // Read offset per replicated log, so each poll reads only what was added.
        let mut remote_offsets = std::collections::BTreeMap::<PathBuf, u64>::new();
        // Which project to follow reads config and lists directories, so it is
        // worked out off the hub lock and only every FOLLOW_REFRESH.
        let mut follow_cache: Option<(PathBuf, PathBuf, Option<String>, Instant)> = None;

        loop {
            let hub_root = hub.read().await.project_root.clone();
            let fresh = follow_cache.as_ref().is_some_and(|(cached_hub, _, _, at)| {
                *cached_hub == hub_root && at.elapsed() < FOLLOW_REFRESH
            });
            if !fresh {
                let resolve_root = hub_root.clone();
                let resolved = tokio::task::spawn_blocking(move || {
                    resolve_follow_root(&resolve_root, allow_fallback)
                })
                .await
                .unwrap_or_else(|_| (hub_root.clone(), None));
                follow_cache = Some((hub_root.clone(), resolved.0, resolved.1, Instant::now()));
            }
            let (root, fallback) = follow_cache
                .as_ref()
                .map(|(_, follow, fallback, _)| (follow.clone(), fallback.clone()))
                .unwrap_or_else(|| (hub_root.clone(), None));
            let path = mcp_verbose_log_path(&root);
            let path_key = path.display().to_string();
            let follow_key = format!("{}|{}", hub_root.display(), root.display());

            if follow_key != project_following {
                project_following = follow_key;
                remote_offsets.clear();
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
                if let Ok((lines, end)) = seed_log(&path).await {
                    for chunk in lines.chunks(BATCH_LINES) {
                        let payload = serde_json::to_string(chunk).unwrap_or_else(|_| "[]".into());
                        yield Ok(Event::default().event("batch").data(payload));
                    }
                    offset = end;
                    yield Ok(Event::default().event("ready").data(format!("following {offset}")));
                } else {
                    yield Ok(Event::default().event("ready").data("waiting for log file"));
                }
            }

            let today = ax_usage::rotation_calendar_date(Some(&root), Utc::now());
            for (day, remote_path) in ax_usage::replicated_log_files(&root) {
                if day != today {
                    continue;
                }
                let Ok(meta) = tokio::fs::metadata(&remote_path).await else {
                    continue;
                };
                let known = remote_offsets.get(&remote_path).copied();
                if known == Some(meta.len()) {
                    continue;
                }
                // First sight, or the replica was truncated or replaced: seed
                // its tail; otherwise read only what was appended.
                let read = match known {
                    Some(offset) if meta.len() > offset => read_lines_from(&remote_path, offset).await,
                    _ => seed_log(&remote_path).await,
                };
                let Ok((raw, end)) = read else {
                    continue;
                };
                remote_offsets.insert(remote_path.clone(), end);
                let origin = ax_usage::replicated_log_origin(&root, &remote_path);
                let lines: Vec<String> = raw
                    .iter()
                    .map(|line| ax_usage::label_log_line(line.trim_end(), origin.as_deref()))
                    .filter(|line| !line.is_empty())
                    .collect();
                for chunk in lines.chunks(BATCH_LINES) {
                    let payload = serde_json::to_string(chunk).unwrap_or_else(|_| "[]".into());
                    yield Ok(Event::default().event("batch").data(payload));
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

/// The lines a new client starts with (the last [`SEED_MAX_BYTES`] of the
/// log, whole lines only) and the offset just past the last complete line, so
/// a line being written, or appended after the read, comes from the tail.
async fn seed_log(path: &Path) -> Result<(Vec<String>, u64), std::io::Error> {
    let len = tokio::fs::metadata(path).await?.len();
    if len <= SEED_MAX_BYTES {
        return read_lines_window(path, 0, SEED_MAX_BYTES, false).await;
    }
    // Start one byte early: when that byte is a newline, the line at the
    // window start is whole and kept.
    let start = len - SEED_MAX_BYTES - 1;
    read_lines_window(path, start, SEED_MAX_BYTES + 1, true).await
}

/// Complete lines from `offset` on (at most [`SEED_MAX_BYTES`] per call), and
/// the offset just past the last one.
async fn read_lines_from(path: &Path, offset: u64) -> Result<(Vec<String>, u64), std::io::Error> {
    read_lines_window(path, offset, SEED_MAX_BYTES, false).await
}

/// Reads at most `max` bytes from `start` and returns the non-empty complete
/// lines in them with the offset just past the last newline. With
/// `skip_first`, the bytes up to the first newline end a line that began
/// before `start` and are skipped. A single line longer than `max` is skipped
/// rather than waited for forever.
async fn read_lines_window(
    path: &Path,
    start: u64,
    max: u64,
    skip_first: bool,
) -> Result<(Vec<String>, u64), std::io::Error> {
    let mut file = tokio::fs::File::open(path).await?;
    file.seek(SeekFrom::Start(start)).await?;
    let mut buf = Vec::new();
    file.take(max).read_to_end(&mut buf).await?;
    let from = if skip_first {
        match buf.iter().position(|&b| b == b'\n') {
            Some(p) => p + 1,
            None => return Ok((Vec::new(), start + buf.len() as u64)),
        }
    } else {
        0
    };
    let end = match buf[from..].iter().rposition(|&b| b == b'\n') {
        Some(p) => from + p + 1,
        None if buf.len() as u64 >= max => buf.len(),
        None => from,
    };
    let lines = String::from_utf8_lossy(&buf[from..end])
        .lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect();
    Ok((lines, start + end as u64))
}

async fn read_new_chunk(path: &Path, offset: u64) -> Result<(String, u64), std::io::Error> {
    let mut file = tokio::fs::File::open(path).await?;
    file.seek(SeekFrom::Start(offset)).await?;
    let mut buf = Vec::new();
    file.take(SEED_MAX_BYTES).read_to_end(&mut buf).await?;
    // A character still being written stays for the next read.
    let complete = crate::utf8_stream::complete_len(&buf);
    let new_offset = offset + complete as u64;
    let text = String::from_utf8_lossy(&buf[..complete]).into_owned();
    Ok((text, new_offset))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[tokio::test]
    async fn a_character_half_written_to_the_log_is_read_once_complete() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-verbose.log");
        let line = "tool ─ 🦀\n".as_bytes();
        let cut = line.len() - 3; // inside the 4-byte crab
        fs::write(&path, &line[..cut]).unwrap();
        let (first, offset) = read_new_chunk(&path, 0).await.unwrap();
        assert!(!first.contains('\u{fffd}'), "{first:?}");

        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut file, &line[cut..]).unwrap();
        let (second, end) = read_new_chunk(&path, offset).await.unwrap();
        assert_eq!(format!("{first}{second}"), "tool ─ 🦀\n");
        assert_eq!(end, line.len() as u64);
    }

    #[tokio::test]
    async fn the_seed_stops_before_a_line_still_being_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-verbose.log");
        fs::write(&path, "a\nb\npar").unwrap();
        let (lines, offset) = seed_log(&path).await.unwrap();
        assert_eq!(lines, vec!["a", "b"]);
        assert_eq!(offset, 4, "the half line is read by the tail once complete");
    }

    #[tokio::test]
    async fn the_seed_of_a_large_log_is_only_its_last_whole_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp-verbose.log");
        let line = format!("{}\n", "x".repeat(99));
        let count = (SEED_MAX_BYTES as usize / line.len()) * 3;
        fs::write(&path, line.repeat(count)).unwrap();
        let (lines, offset) = seed_log(&path).await.unwrap();
        assert!(!lines.is_empty());
        assert!(
            lines.len() <= SEED_MAX_BYTES as usize / line.len(),
            "{} lines seeded",
            lines.len()
        );
        assert!(lines.iter().all(|l| l.len() == 99), "a cut line was seeded");
        assert_eq!(offset, (line.len() * count) as u64);
    }

    #[tokio::test]
    async fn complete_lines_are_read_from_an_offset_and_a_half_line_waits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("replica.log");
        fs::write(&path, "one\ntwo\nthr").unwrap();
        let (lines, offset) = read_lines_from(&path, 4).await.unwrap();
        assert_eq!(lines, vec!["two"]);
        assert_eq!(offset, 8);
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut file, b"ee\n").unwrap();
        let (lines, offset) = read_lines_from(&path, offset).await.unwrap();
        assert_eq!(lines, vec!["three"]);
        assert_eq!(offset, 14);
    }

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
        fs::write(
            other.join(".ax").join("mcp-verbose-2026-09-15.log"),
            "other\n",
        )
        .unwrap();
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
        fs::write(
            other.join(".ax").join("mcp-verbose-2026-09-14.log"),
            "from-ax\n",
        )
        .unwrap();
        let (got, fallback) = pick_trace_project_root(&hub, std::slice::from_ref(&other));
        assert_eq!(got, other);
        assert_eq!(fallback.as_deref(), Some("ax"));
    }
}
