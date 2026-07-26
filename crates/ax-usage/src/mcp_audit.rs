//! MCP quality audit: correlate Cursor transcripts with daily `.ax/mcp-verbose-*.log` files.
//!
//! Scores policy-tool usage (preflight, explore-before-grep, enrichment) and
//! estimates token waste when agents fall back to Read/Grep instead of graph tools.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use walkdir::WalkDir;

/// Default rolling window for live quality (minutes).
pub const DEFAULT_WINDOW_MINUTES: u64 = 30;

/// Heuristic tokens burned by one unnecessary Read/Grep when ax_explore would suffice.
const WASTE_READ_TOKENS: i64 = 2_500;
const WASTE_GREP_TOKENS: i64 = 800;
const WASTE_MISSING_PREFLIGHT: i64 = 4_000;
const WASTE_EMPTY_ENRICH: i64 = 1_500;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualitySnapshot {
    pub project_root: String,
    pub project_label: String,
    pub log_path: String,
    pub mode: String,
    pub window_minutes: u64,
    pub updated_at_ms: i64,
    pub score: u8,
    pub grade: String,
    pub correlation_pct: f64,
    pub matched_calls: usize,
    pub unmatched_ax_calls: usize,
    pub verbose_clusters: usize,
    pub enrichment: EnrichmentMetrics,
    pub tool_mix: ToolMix,
    pub findings: Vec<Finding>,
    pub tokens_at_risk: i64,
    pub critical_count: usize,
    pub verbose_enabled: bool,
    pub verbose_present: bool,
    pub session_id: Option<String>,
    pub session_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnrichmentMetrics {
    pub inject_chars_p50: i64,
    pub inject_chars_p95: i64,
    pub enrich_done_rate: f64,
    pub empty_enrich_count: usize,
    pub matched_rules_rate: f64,
    pub preflight_count: usize,
    pub inbound_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolMix {
    pub preflight: usize,
    pub explore: usize,
    pub guard: usize,
    pub graph: usize,
    pub other_ax: usize,
    pub read: usize,
    pub grep: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub id: String,
    pub check: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub waste_hint: String,
    pub tokens_est: i64,
    pub tool: Option<String>,
    pub ts_ms: Option<i64>,
    pub log_line_hint: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AuditOptions {
    pub window_minutes: Option<u64>,
    pub session_path: Option<PathBuf>,
    pub session_id: Option<String>,
    pub persist: bool,
}

#[derive(Debug, Clone)]
struct VerboseLine {
    ts_ms: i64,
    kind: String,
    tool: String,
    body: String,
    inject_chars: Option<i64>,
    matched_rules: Option<bool>,
    session_id: Option<String>,
    /// True for `[ax-mcp] …` tool traces. Domain `[ax] lsp|workspace|…` lines are
    /// kept for quality checks but must not form tool clusters (false VerboseGap).
    is_mcp: bool,
}

#[derive(Debug, Clone)]
struct VerboseCluster {
    tool: String,
    ts_ms: i64,
    has_inbound: bool,
    has_outbound: bool,
    has_error: bool,
    inject_chars: Option<i64>,
    matched_rules: Option<bool>,
    session_id: Option<String>,
    lines: Vec<String>,
}

#[derive(Debug, Clone)]
struct TranscriptEvent {
    ts_ms: i64,
    kind: TranscriptKind,
    name: String,
    ax_tool: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TranscriptKind {
    AxTool,
    Read,
    Grep,
    Other,
}

/// Cursor folder slug for a workspace path (`C:\gary\ax` → `c-gary-ax`).
pub fn cursor_project_slug(project_root: &Path) -> String {
    let s = project_root.to_string_lossy().to_lowercase();
    let mut out = String::new();
    let mut prev_dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

pub fn audit_dir(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("audit")
}

pub fn latest_snapshot_path(project_root: &Path) -> PathBuf {
    audit_dir(project_root).join("latest.json")
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn parse_iso_ms(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
        .or_else(|| {
            // Truncate fractional seconds if needed for chrono.
            let trimmed = s.trim();
            if let Some(dot) = trimmed.find('.') {
                let z = if trimmed.ends_with('Z') { "Z" } else { "" };
                let base = &trimmed[..dot];
                let rebuilt = format!("{base}{z}");
                return DateTime::parse_from_rfc3339(&rebuilt)
                    .ok()
                    .map(|dt| dt.timestamp_millis());
            }
            None
        })
}

/// Parse Cursor agent-transcript embed: `<timestamp>Tuesday, Jul 21, 2026, 7:31 PM (UTC+2)</timestamp>`.
fn parse_cursor_embed_timestamp(raw: &str) -> Option<i64> {
    let start = raw.find("<timestamp>")? + "<timestamp>".len();
    let end = start + raw[start..].find("</timestamp>")?;
    let inner = raw[start..end].trim();
    if inner.is_empty() {
        return None;
    }
    let (datetime_part, tz_part) = match inner.rsplit_once('(') {
        Some(parts) => parts,
        None => (inner, "UTC"),
    };
    let datetime_part = datetime_part.trim().trim_end_matches(',').trim();
    let tz_part = tz_part.trim().trim_end_matches(')').trim();
    let offset = parse_utc_offset_secs(tz_part)?;
    let tz = FixedOffset::east_opt(offset)?;
    const FMTS: &[&str] = &[
        "%A, %b %e, %Y, %l:%M %p",
        "%A, %b %d, %Y, %l:%M %p",
        "%A, %b %e, %Y, %I:%M %p",
        "%A, %b %d, %Y, %I:%M %p",
        "%A, %B %e, %Y, %l:%M %p",
        "%A, %B %d, %Y, %I:%M %p",
    ];
    for fmt in FMTS {
        if let Ok(naive) = NaiveDateTime::parse_from_str(datetime_part, fmt) {
            return Some(tz.from_local_datetime(&naive).single()?.timestamp_millis());
        }
    }
    None
}

fn parse_utc_offset_secs(tz: &str) -> Option<i32> {
    let t = tz.trim();
    if t.eq_ignore_ascii_case("UTC") || t.eq_ignore_ascii_case("Z") {
        return Some(0);
    }
    let rest = t
        .strip_prefix("UTC")
        .or_else(|| t.strip_prefix("utc"))
        .or_else(|| t.strip_prefix("Gmt"))
        .or_else(|| t.strip_prefix("GMT"))
        .unwrap_or(t)
        .trim();
    if rest.is_empty() {
        return Some(0);
    }
    let (sign, num) = if let Some(n) = rest.strip_prefix('+') {
        (1, n)
    } else if let Some(n) = rest.strip_prefix('-') {
        (-1, n)
    } else {
        (1, rest)
    };
    let hours: i32 = if let Some((h, m)) = num.split_once(':') {
        let h: i32 = h.parse().ok()?;
        let m: i32 = m.parse().ok()?;
        h * 3600 + m * 60
    } else {
        let h: i32 = num.parse().ok()?;
        h * 3600
    };
    Some(sign * hours)
}

/// Max Δt when matching transcript tool calls to verbose clusters (ms).
/// Cursor embed timestamps are per user-turn, so allow several minutes of skew.
const CORRELATE_MAX_DT_MS: i64 = 600_000;

fn project_label(root: &Path) -> String {
    root.file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| root.display().to_string())
}

fn extract_kv(body: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let start = body.find(&needle)? + needle.len();
    let rest = &body[start..];
    if rest.starts_with('{') || rest.starts_with('[') {
        // JSON payload — take until matching close is hard; grab rest of line.
        return Some(rest.to_string());
    }
    let end = rest
        .find(|c: char| c.is_whitespace())
        .unwrap_or(rest.len());
    Some(rest[..end].to_string())
}

fn parse_inject_chars(body: &str) -> Option<i64> {
    extract_kv(body, "final_inject_chars")
        .or_else(|| extract_kv(body, "inject_chars"))
        .and_then(|s| s.parse::<i64>().ok())
}

fn parse_verbose_line(raw: &str) -> Option<VerboseLine> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let (ts_part, after_ts) = raw.split_once(' ')?;
    let ts_ms = parse_iso_ms(ts_part).unwrap_or(0);
    let after_ts = after_ts.trim();
    let is_mcp = after_ts.starts_with("[ax-mcp]");
    let body = after_ts
        .strip_prefix("[ax-mcp] ")
        .or_else(|| after_ts.strip_prefix("[ax-mcp]"))
        .unwrap_or(after_ts)
        .trim();
    let kind = body
        .split_whitespace()
        .next()
        .unwrap_or("other")
        .to_string();
    let tool = extract_kv(body, "tool").unwrap_or_default();
    let inject_chars = parse_inject_chars(body);
    let matched_rules = if body.contains("matched_rules=") {
        extract_kv(body, "matched_rules").map(|s| {
            s == "1" || s.eq_ignore_ascii_case("true") || s.parse::<i64>().unwrap_or(0) > 0
        })
    } else if body.contains("keys=[") && tool.contains("preflight") {
        // internal preflight with keys present ⇒ rules likely matched
        Some(body.contains("rules") || body.contains("ax_policy") || inject_chars.unwrap_or(0) > 0)
    } else {
        None
    };
    Some(VerboseLine {
        ts_ms,
        kind,
        tool,
        body: body.to_string(),
        inject_chars,
        matched_rules,
        session_id: extract_kv(body, "session"),
        is_mcp,
    })
}

fn cluster_verbose_lines(lines: &[VerboseLine]) -> Vec<VerboseCluster> {
    let mut clusters: Vec<VerboseCluster> = Vec::new();
    for line in lines {
        // Domain `[ax] workspace|lsp|…` lines are quality signals, not tool clusters.
        if !line.is_mcp {
            continue;
        }
        let is_inbound = line.kind == "inbound";
        // Enrich side-channel lines have no tool= — keep them on the open preflight cluster.
        let is_enrich = line.kind == "enrich" || line.body.starts_with("enrich ");
        let same_tool = clusters
            .last()
            .map(|c| {
                !c.has_outbound
                    && ((!line.tool.is_empty() && c.tool == line.tool)
                        || (is_enrich && c.tool == "ax_preflight"))
            })
            .unwrap_or(false);
        if is_inbound || !same_tool {
            let tool = if line.tool.is_empty() && is_enrich {
                "ax_preflight".into()
            } else {
                line.tool.clone()
            };
            clusters.push(VerboseCluster {
                tool,
                ts_ms: line.ts_ms,
                has_inbound: is_inbound,
                has_outbound: line.kind == "outbound" || line.kind == "preview",
                has_error: line.kind == "error",
                inject_chars: line.inject_chars,
                matched_rules: line.matched_rules,
                session_id: line.session_id.clone(),
                lines: vec![line.body.clone()],
            });
        } else if let Some(c) = clusters.last_mut() {
            if line.kind == "outbound" || line.kind == "preview" {
                c.has_outbound = true;
            }
            if line.kind == "error" {
                c.has_error = true;
            }
            // Prefer the largest known inject size (don't let a later 0 overwrite enrich done).
            if let Some(n) = line.inject_chars {
                c.inject_chars = Some(c.inject_chars.map(|cur| cur.max(n)).unwrap_or(n));
            }
            if c.session_id.is_none() {
                c.session_id = line.session_id.clone();
            }
            if let Some(m) = line.matched_rules {
                c.matched_rules = Some(m);
            }
            c.lines.push(line.body.clone());
        }
    }
    clusters
}

fn domain_blob_from(lines: &[VerboseLine]) -> String {
    lines
        .iter()
        .map(|l| l.body.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_verbose_log(text: &str, since_ms: i64) -> (Vec<VerboseLine>, Vec<VerboseCluster>) {
    let mut lines = Vec::new();
    for raw in text.lines() {
        if let Some(line) = parse_verbose_line(raw) {
            if line.ts_ms == 0 || line.ts_ms >= since_ms {
                lines.push(line);
            }
        }
    }
    let clusters = cluster_verbose_lines(&lines);
    (lines, clusters)
}

fn classify_cursor_tool_event(name: &str, input: &Value) -> TranscriptEvent {
    let mut ev = TranscriptEvent {
        ts_ms: 0,
        kind: TranscriptKind::Other,
        name: name.to_string(),
        ax_tool: None,
    };
    if name == "Read" {
        ev.kind = TranscriptKind::Read;
    } else if name == "Grep" {
        ev.kind = TranscriptKind::Grep;
    } else if name == "CallMcpTool" || name == "CallDynamicTool" {
        let nested = input.get("arguments").cloned().unwrap_or(Value::Null);
        let server = input
            .get("server")
            .or_else(|| input.get("namespace"))
            .or_else(|| nested.get("namespace"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let tool = input
            .get("toolName")
            .or_else(|| input.get("tool_name"))
            .or_else(|| nested.get("toolName"))
            .or_else(|| nested.get("tool_name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !tool.is_empty() && (server.contains("ax") || tool.starts_with("ax_")) {
            ev.kind = TranscriptKind::AxTool;
            ev.ax_tool = Some(tool.to_string());
            ev.name = tool.to_string();
        }
    } else if name.starts_with("ax_") || name.starts_with("mcp__ax__") {
        ev.kind = TranscriptKind::AxTool;
        let tool = name
            .strip_prefix("mcp__ax__")
            .unwrap_or(name)
            .to_string();
        ev.ax_tool = Some(tool.clone());
        ev.name = tool;
    }
    ev
}

fn parse_transcript_line(line: &str) -> Vec<TranscriptEvent> {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let ts_ms = v
        .get("timestamp")
        .and_then(|t| t.as_str())
        .and_then(parse_iso_ms)
        .unwrap_or(0);
    if v.get("role").and_then(|r| r.as_str()) != Some("assistant") {
        return Vec::new();
    }
    let Some(msg) = v.get("message") else {
        return Vec::new();
    };
    let Some(items) = msg.get("content").and_then(|c| c.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in items {
        if item.get("type").and_then(|t| t.as_str()) != Some("tool_use") {
            continue;
        }
        let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let input = item.get("input").cloned().unwrap_or(Value::Null);
        let mut ev = classify_cursor_tool_event(name, &input);
        ev.ts_ms = ts_ms;
        if ev.kind != TranscriptKind::Other {
            out.push(ev);
        }
    }
    out
}

fn load_transcript_events(path: &Path, since_ms: i64) -> Result<Vec<TranscriptEvent>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut events = Vec::new();
    let mut last_ts: i64 = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(embed_ts) = parse_cursor_embed_timestamp(line) {
            last_ts = embed_ts;
        }
        for mut ev in parse_transcript_line(line) {
            if ev.ts_ms > 0 {
                last_ts = ev.ts_ms;
            } else if last_ts > 0 {
                // Carry forward Cursor `<timestamp>` / prior JSON ts onto tool_use rows.
                ev.ts_ms = last_ts;
            }
            if ev.ts_ms == 0 || ev.ts_ms >= since_ms {
                events.push(ev);
            }
        }
    }
    // Untimed Cursor transcripts include the whole chat; when auditing a rolling
    // window, keep only the most recent events so Read/Grep don't dominate.
    let timed = events.iter().any(|e| e.ts_ms > 0);
    if !timed && since_ms > 0 && events.len() > UNTIMED_EVENT_CAP {
        let skip = events.len() - UNTIMED_EVENT_CAP;
        events = events.split_off(skip);
    }
    Ok(events)
}

/// Cap for transcript events when timestamps are missing (rolling-window audits).
const UNTIMED_EVENT_CAP: usize = 80;

/// Find recent Cursor transcript JSONL files for a workspace.
pub fn find_cursor_transcripts(project_root: &Path) -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let slug = cursor_project_slug(project_root);
    let dir = home
        .join(".cursor")
        .join("projects")
        .join(&slug)
        .join("agent-transcripts");
    if !dir.is_dir() {
        // Fallback: scan all projects for matching slug substring
        return find_transcripts_fallback(&home.join(".cursor").join("projects"), &slug);
    }
    collect_transcripts(&dir)
}

fn find_transcripts_fallback(projects: &Path, slug: &str) -> Vec<PathBuf> {
    if !projects.is_dir() {
        return Vec::new();
    }
    let mut best: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(projects).max_depth(1).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        if name.contains(slug) || slug.contains(name.as_ref()) {
            let transcripts = entry.path().join("agent-transcripts");
            if transcripts.is_dir() {
                best.extend(collect_transcripts(&transcripts));
            }
        }
    }
    best
}

fn collect_transcripts(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<(i64, PathBuf)> = WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            let p = e.path();
            let s = p.to_string_lossy();
            s.contains("agent-transcripts")
                && !s.contains("subagents")
                && p.extension().and_then(|x| x.to_str()) == Some("jsonl")
                && {
                    let parent = p.parent().and_then(|x| x.file_name()).and_then(|n| n.to_str());
                    let stem = p.file_stem().and_then(|n| n.to_str());
                    parent.is_some() && parent == stem
                }
        })
        .map(|e| {
            let mtime = e
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            (mtime, e.path().to_path_buf())
        })
        .collect();
    paths.sort_by(|a, b| b.0.cmp(&a.0));
    paths.into_iter().map(|(_, p)| p).collect()
}

fn resolve_session_path(opts: &AuditOptions, project_root: &Path) -> Option<PathBuf> {
    if let Some(p) = &opts.session_path {
        if p.is_file() {
            return Some(p.clone());
        }
        // Treat as session uuid under agent-transcripts
        let slug = cursor_project_slug(project_root);
        if let Some(home) = dirs::home_dir() {
            let candidate = home
                .join(".cursor")
                .join("projects")
                .join(&slug)
                .join("agent-transcripts")
                .join(p)
                .join(format!("{}.jsonl", p.display()));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    if let Some(id) = &opts.session_id {
        let slug = cursor_project_slug(project_root);
        if let Some(home) = dirs::home_dir() {
            let candidate = home
                .join(".cursor")
                .join("projects")
                .join(&slug)
                .join("agent-transcripts")
                .join(id)
                .join(format!("{id}.jsonl"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    pick_best_transcript(find_cursor_transcripts(project_root))
}

/// Prefer a recent transcript that actually contains ax tool calls over an empty newest chat.
fn pick_best_transcript(paths: Vec<PathBuf>) -> Option<PathBuf> {
    if paths.is_empty() {
        return None;
    }
    let mut best: Option<(i64, PathBuf)> = None;
    for (idx, path) in paths.into_iter().take(10).enumerate() {
        let events = load_transcript_events(&path, 0).unwrap_or_default();
        let ax = events
            .iter()
            .filter(|e| e.kind == TranscriptKind::AxTool)
            .count() as i64;
        let read_grep = events
            .iter()
            .filter(|e| matches!(e.kind, TranscriptKind::Read | TranscriptKind::Grep))
            .count() as i64;
        // Newest-first list: small idx is fresher. Prefer ax activity heavily.
        let score = ax.saturating_mul(100) + read_grep.min(40) + (20 - idx as i64).max(0);
        if best.as_ref().map(|(s, _)| score > *s).unwrap_or(true) {
            best = Some((score, path));
        }
    }
    best.map(|(_, p)| p)
}

fn percentile(sorted: &[i64], p: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((p * (sorted.len() as f64 - 1.0)).round() as usize).min(sorted.len() - 1);
    sorted[idx]
}

fn is_graph_tool(tool: &str) -> bool {
    matches!(
        tool,
        "ax_explore"
            | "ax_context"
            | "ax_node"
            | "ax_search"
            | "ax_callers"
            | "ax_callees"
            | "ax_impact"
            | "ax_affected"
            | "ax_insights"
            | "ax_report"
    )
}

fn tool_mix_from(clusters: &[VerboseCluster], events: &[TranscriptEvent]) -> ToolMix {
    let mut mix = ToolMix::default();
    for c in clusters {
        let t = c.tool.as_str();
        if t == "ax_preflight" {
            if c.has_inbound {
                mix.preflight += 1;
            }
        } else if t == "ax_explore" {
            mix.explore += 1;
            mix.graph += 1;
        } else if t == "ax_guard" {
            mix.guard += 1;
        } else if is_graph_tool(t) {
            mix.graph += 1;
        } else if t.starts_with("ax_") {
            mix.other_ax += 1;
        }
    }

    let timed_count = events.iter().filter(|e| e.ts_ms > 0).count();
    let mut unique_ts: Vec<i64> = events
        .iter()
        .filter_map(|e| (e.ts_ms > 0).then_some(e.ts_ms))
        .collect();
    unique_ts.sort_unstable();
    unique_ts.dedup();
    // Cursor embed timestamps are often one value per user turn — treat those as
    // untimed for Read/Grep capping so full-session audits don't explode.
    let transcript_timed =
        timed_count > 0 && unique_ts.len().saturating_mul(2) >= timed_count.max(1);
    let mut tr_explore = 0usize;
    let mut tr_graph = 0usize;
    let mut tr_read = 0usize;
    let mut tr_grep = 0usize;
    let mut tr_preflight = 0usize;
    let mut tr_guard = 0usize;
    for e in events {
        match e.kind {
            TranscriptKind::Read => tr_read += 1,
            TranscriptKind::Grep => tr_grep += 1,
            TranscriptKind::AxTool => {
                if let Some(t) = e.ax_tool.as_deref() {
                    if t == "ax_preflight" {
                        tr_preflight += 1;
                    } else if t == "ax_guard" {
                        tr_guard += 1;
                    } else if t == "ax_explore" {
                        tr_explore += 1;
                        tr_graph += 1;
                    } else if is_graph_tool(t) {
                        tr_graph += 1;
                    }
                }
            }
            TranscriptKind::Other => {}
        }
    }

    // Prefer the richer signal so CallDynamicTool explores aren't invisible.
    mix.explore = mix.explore.max(tr_explore);
    mix.graph = mix.graph.max(tr_graph);
    mix.preflight = mix.preflight.max(tr_preflight);
    mix.guard = mix.guard.max(tr_guard);

    if transcript_timed {
        mix.read = tr_read;
        mix.grep = tr_grep;
    } else {
        // Untimed transcripts span the whole chat; don't compare them raw to a
        // rolling verbose window (false ExploreBeforeGrep megawaste).
        let cap = (mix.graph.max(1).saturating_mul(6)).max(20);
        mix.read = tr_read.min(cap);
        mix.grep = tr_grep.min(cap.saturating_mul(2) / 3);
    }
    mix
}

fn correlate(
    clusters: &[VerboseCluster],
    events: &[TranscriptEvent],
) -> (usize, usize, f64, Vec<usize>) {
    let ax_events: Vec<&TranscriptEvent> = events
        .iter()
        .filter(|e| e.kind == TranscriptKind::AxTool)
        .collect();
    if ax_events.is_empty() {
        // Nothing to correlate — healthy idle (not a 0% gap).
        return (0, 0, 100.0, Vec::new());
    }
    let mut used = vec![false; clusters.len()];
    let mut matched = 0usize;
    let mut unmatched_idxs = Vec::new();
    // Prefer time matching only when timestamps are diverse enough. Cursor embed
    // timestamps are often one value per user turn (many tools share the same ms),
    // which would false-fail a tight Δt match — fall back to order matching then.
    let timed_count = ax_events.iter().filter(|e| e.ts_ms > 0).count();
    let mut unique_ts: Vec<i64> = ax_events
        .iter()
        .filter_map(|e| (e.ts_ms > 0).then_some(e.ts_ms))
        .collect();
    unique_ts.sort_unstable();
    unique_ts.dedup();
    let transcripts_have_ts =
        timed_count > 0 && unique_ts.len().saturating_mul(2) >= timed_count.max(1);

    for (ei, ev) in ax_events.iter().enumerate() {
        let tool = ev.ax_tool.as_deref().unwrap_or("");
        let mut best: Option<usize> = None;
        let mut best_dt = i64::MAX;
        for (ci, c) in clusters.iter().enumerate() {
            if used[ci] || c.tool != tool {
                continue;
            }
            if transcripts_have_ts && ev.ts_ms > 0 {
                let dt = (c.ts_ms - ev.ts_ms).abs();
                if dt < best_dt && dt < CORRELATE_MAX_DT_MS {
                    best_dt = dt;
                    best = Some(ci);
                }
            } else {
                // No reliable transcript timestamps — first unused same-tool cluster.
                best = Some(ci);
                break;
            }
        }
        if let Some(ci) = best {
            used[ci] = true;
            matched += 1;
        } else {
            unmatched_idxs.push(ei);
        }
    }
    // Untimed transcripts often retain more ax calls than the rolling verbose
    // window can supply — score against cluster capacity so we don't false-flag
    // VerboseGap when every available cluster already matched.
    let pct = if transcripts_have_ts {
        (matched as f64 / ax_events.len() as f64) * 100.0
    } else {
        let capacity = clusters.len().max(1);
        let denom = ax_events.len().min(capacity);
        if denom == 0 {
            0.0
        } else {
            (matched.min(denom) as f64 / denom as f64) * 100.0
        }
    };
    (matched, unmatched_idxs.len(), pct, unmatched_idxs)
}

fn score_and_findings(
    clusters: &[VerboseCluster],
    events: &[TranscriptEvent],
    mix: &ToolMix,
    enrichment: &EnrichmentMetrics,
    correlation_pct: f64,
    mode: &str,
    verbose_present: bool,
    verbose_enabled: bool,
    domain_blob: &str,
) -> (u8, Vec<Finding>, i64) {
    let mut findings = Vec::new();
    let mut score: i32 = 100;
    let mut tokens_at_risk: i64 = 0;

    // PreflightOnce
    if enrichment.inbound_count > 0 && enrichment.preflight_count == 0 {
        let tokens = WASTE_MISSING_PREFLIGHT;
        tokens_at_risk += tokens;
        score -= 25;
        findings.push(Finding {
            id: "preflight-once".into(),
            check: "PreflightOnce".into(),
            severity: "critical".into(),
            title: "No ax_preflight in window".into(),
            detail: "MCP traffic arrived without a preflight call — inject/rules may be missing."
                .into(),
            waste_hint: "Without inject, agents re-grep and re-read instead of using policy context."
                .into(),
            tokens_est: tokens,
            tool: Some("ax_preflight".into()),
            ts_ms: None,
            log_line_hint: None,
        });
    } else if enrichment.preflight_count > 5 && enrichment.inbound_count > 0 {
        // Multiple preflights across turns is normal; only warn on clear spam
        // vs non-preflight inbound volume.
        let non_preflight_inbound = enrichment
            .inbound_count
            .saturating_sub(enrichment.preflight_count);
        let ratio = enrichment.preflight_count as f64
            / enrichment.inbound_count.max(1) as f64;
        if ratio > 0.85 && non_preflight_inbound < 2 {
            score -= 5;
            findings.push(Finding {
                id: "preflight-spam".into(),
                check: "PreflightOnce".into(),
                severity: "low".into(),
                title: "Frequent ax_preflight calls".into(),
                detail: format!(
                    "{} preflight vs {} inbound in window — prefer once per turn.",
                    enrichment.preflight_count, enrichment.inbound_count
                ),
                waste_hint: "Extra preflight responses add tokens without new policy context."
                    .into(),
                tokens_est: 400,
                tool: Some("ax_preflight".into()),
                ts_ms: None,
                log_line_hint: None,
            });
            tokens_at_risk += 400;
        }
    }

    // EnrichPresent
    if enrichment.empty_enrich_count > 0 {
        let tokens = enrichment.empty_enrich_count as i64 * WASTE_EMPTY_ENRICH;
        tokens_at_risk += tokens;
        score -= 15.min(enrichment.empty_enrich_count as i32 * 5);
        findings.push(Finding {
            id: "enrich-empty".into(),
            check: "EnrichPresent".into(),
            severity: "high".into(),
            title: "Empty or weak enrichment".into(),
            detail: format!(
                "{} preflight/enrich clusters with inject_chars=0 or missing inject.",
                enrichment.empty_enrich_count
            ),
            waste_hint: "Empty inject forces longer tool loops and duplicated discovery.".into(),
            tokens_est: tokens,
            tool: Some("ax_preflight".into()),
            ts_ms: None,
            log_line_hint: None,
        });
    }

    if enrichment.preflight_count > 0 && enrichment.matched_rules_rate < 0.5 {
        score -= 10;
        findings.push(Finding {
            id: "rules-injected".into(),
            check: "RulesInjected".into(),
            severity: "medium".into(),
            title: "Low matched_rules rate".into(),
            detail: format!(
                "Only {:.0}% of preflight enrichments show matched rules.",
                enrichment.matched_rules_rate * 100.0
            ),
            waste_hint: "Weak rule injection reduces policy compliance and increases rework."
                .into(),
            tokens_est: 1_000,
            tool: Some("ax_preflight".into()),
            ts_ms: None,
            log_line_hint: None,
        });
        tokens_at_risk += 1_000;
    }

    // ExploreBeforeGrep — skip when MCP is quiet/unreachable (DEGRADED agents must Grep/Read).
    if enrichment.inbound_count > 0 && mix.read + mix.grep > 0 && mix.explore == 0 && mix.graph == 0
    {
        let tokens = mix.read as i64 * WASTE_READ_TOKENS + mix.grep as i64 * WASTE_GREP_TOKENS;
        tokens_at_risk += tokens;
        score -= 20;
        findings.push(Finding {
            id: "explore-before-grep".into(),
            check: "ExploreBeforeGrep".into(),
            severity: "high".into(),
            title: "Read/Grep without graph tools".into(),
            detail: format!(
                "{} Read + {} Grep with no ax_explore/graph calls in window.",
                mix.read, mix.grep
            ),
            waste_hint: "Graph answers usually replace many file reads — burn tokens otherwise."
                .into(),
            tokens_est: tokens,
            tool: Some("ax_explore".into()),
            ts_ms: None,
            log_line_hint: None,
        });
    } else if enrichment.inbound_count > 0
        && mix.read + mix.grep > mix.graph.saturating_mul(4).max(4)
        && mix.graph > 0
    {
        let excess = (mix.read + mix.grep).saturating_sub(mix.graph * 2);
        let tokens = excess as i64 * 600;
        tokens_at_risk += tokens;
        score -= 10;
        findings.push(Finding {
            id: "heavy-read-grep".into(),
            check: "ExploreBeforeGrep".into(),
            severity: "medium".into(),
            title: "Heavy Read/Grep vs graph tools".into(),
            detail: format!(
                "Read+Grep={} vs graph={} — possible redundant file scanning.",
                mix.read + mix.grep,
                mix.graph
            ),
            waste_hint: "Prefer ax_node / ax_callers after one explore instead of broad greps."
                .into(),
            tokens_est: tokens,
            tool: Some("ax_explore".into()),
            ts_ms: None,
            log_line_hint: None,
        });
    }

    // GuardBeforeWrite — soft: if Write-like tools aren't in transcript we skip;
    // detect ax_guard absence when there were many other ax calls.
    if mix.graph + mix.other_ax > 5 && mix.guard == 0 {
        score -= 5;
        findings.push(Finding {
            id: "guard-before-write".into(),
            check: "GuardBeforeWrite".into(),
            severity: "low".into(),
            title: "No ax_guard in window".into(),
            detail: "Active MCP traffic without ax_guard — ensure guards run before Write/Delete."
                .into(),
            waste_hint: "Policy misses cause rework edits that cost tokens.".into(),
            tokens_est: 500,
            tool: Some("ax_guard".into()),
            ts_ms: None,
            log_line_hint: None,
        });
        tokens_at_risk += 500;
    }

    // v4 domain lines (workspace/lsp/ship-ci/…) — scanned separately from MCP clusters
    let joined = domain_blob;
    if joined.contains("ship-ci") && joined.contains("status=failed") {
        score -= 15;
        findings.push(Finding {
            id: "ship-ci-failed".into(),
            check: "ShipCiFailed".into(),
            severity: "critical".into(),
            title: "ax ship --ci failed in window".into(),
            detail: "Verbose log shows ship-ci status=failed.".into(),
            waste_hint: "Fix quality gate failures before merging.".into(),
            tokens_est: 0,
            tool: None,
            ts_ms: None,
            log_line_hint: Some("ship-ci status=failed".into()),
        });
    }
    if joined.contains("plugin") && joined.contains("fail") {
        score -= 8;
        findings.push(Finding {
            id: "plugin-extract-errors".into(),
            check: "PluginExtractErrors".into(),
            severity: "medium".into(),
            title: "Extractor plugin failures".into(),
            detail: "Verbose log contains plugin extract failures.".into(),
            waste_hint: "Fix .ax/plugins/*/plugin.toml or the extractor process.".into(),
            tokens_est: 200,
            tool: None,
            ts_ms: None,
            log_line_hint: Some("plugin".into()),
        });
        tokens_at_risk += 200;
    }

    // LspAvailableUnused — only when MCP is active (not domain-only noise)
    let lsp_on_path = ["rust-analyzer", "typescript-language-server", "pyright-langserver", "gopls"]
        .iter()
        .any(|bin| command_on_path(bin));
    if lsp_on_path && enrichment.inbound_count > 0 && !joined.contains("lsp enrich") {
        score -= 5;
        findings.push(Finding {
            id: "lsp-available-unused".into(),
            check: "LspAvailableUnused".into(),
            severity: "low".into(),
            title: "LSP on PATH but unused".into(),
            detail: "A language server is available, but no `lsp enrich` ran in this window. Run `ax lsp enrich` or Unresolved → Enrich with LSP."
                .into(),
            waste_hint: "Unresolved refs stay expensive for agents without Exact LSP edges.".into(),
            tokens_est: 300,
            tool: None,
            ts_ms: None,
            log_line_hint: Some("lsp enrich".into()),
        });
        tokens_at_risk += 300;
    }

    // ShareReadonlyWrite
    if joined.contains("readonly write denied") {
        score -= 4;
        findings.push(Finding {
            id: "share-readonly-write".into(),
            check: "ShareReadonlyWrite".into(),
            severity: "medium".into(),
            title: "Write attempted in share/read-only session".into(),
            detail: "A mutating API was blocked while sharing or AX_WEB_READONLY=1.".into(),
            waste_hint: "Use a local non-share session for edits.".into(),
            tokens_est: 0,
            tool: None,
            ts_ms: None,
            log_line_hint: Some("readonly write denied".into()),
        });
    }

    // EmbedBackend — informational only
    if let Some(backend_line) = joined.lines().find(|l| l.contains("embed backend=")) {
        let backend = backend_line
            .split("backend=")
            .nth(1)
            .unwrap_or("unknown")
            .split_whitespace()
            .next()
            .unwrap_or("unknown");
        findings.push(Finding {
            id: "embed-backend".into(),
            check: "EmbedBackend".into(),
            severity: "info".into(),
            title: format!("Memory embed backend: {backend}"),
            detail: format!(
                "Verbose log reports embed backend={backend}. Place tokenizer.json beside the ONNX model (or set AX_ONNX_TOKENIZER) for production recall."
            ),
            waste_hint: String::new(),
            tokens_est: 0,
            tool: None,
            ts_ms: None,
            log_line_hint: Some("embed backend=".into()),
        });
    }

    // Correlation / verbose gap
    let ax_event_count = events
        .iter()
        .filter(|e| e.kind == TranscriptKind::AxTool)
        .count();
    let transcripts_have_ts = events.iter().any(|e| e.ts_ms > 0);
    // When MCP clusters are empty, UncorrelatedTool covers the gap — avoid double-counting.
    if mode == "transcript_linked"
        && ax_event_count > 0
        && correlation_pct < 50.0
        && !clusters.is_empty()
    {
        // Untimed Cursor transcripts inflate the denominator; treat as medium unless
        // correlation is near-zero (likely wrong project / verbose off).
        let (penalty, severity) = if !transcripts_have_ts && correlation_pct >= 25.0 {
            (5, "medium")
        } else {
            (15, "high")
        };
        score -= penalty;
        findings.push(Finding {
            id: "verbose-gap".into(),
            check: "VerboseGap".into(),
            severity: severity.into(),
            title: "Low transcript↔verbose correlation".into(),
            detail: format!(
                "Only {:.0}% of ax tool calls matched a verbose cluster — enable verbose MCP or check project path.",
                correlation_pct
            ),
            waste_hint: "Blind spots prevent improving the quality loop.".into(),
            tokens_est: 0,
            tool: None,
            ts_ms: None,
            log_line_hint: None,
        });
    }

    if clusters.is_empty() && events.iter().any(|e| e.kind == TranscriptKind::AxTool) {
        if verbose_present {
            if verbose_enabled {
                // Verbose logging is on; MCP simply produced no traffic in-window
                // (disconnected after reinstall, idle, or restart pending). Not a defect.
                findings.push(Finding {
                    id: "uncorrelated-tool".into(),
                    check: "UncorrelatedTool".into(),
                    severity: "info".into(),
                    title: "No MCP verbose clusters in window".into(),
                    detail: "Verbose logging is enabled but the MCP server wrote no tool clusters in this window — usually ax MCP is disconnected or was restarted. Reconnect MCP in the agent; CallMcpTool failures while offline do not count against quality."
                        .into(),
                    waste_hint: String::new(),
                    tokens_est: 0,
                    tool: None,
                    ts_ms: None,
                    log_line_hint: None,
                });
            } else {
                // Log file exists (maybe from another day) but verbose is off now.
                score -= 10;
                findings.push(Finding {
                    id: "uncorrelated-tool".into(),
                    check: "UncorrelatedTool".into(),
                    severity: "medium".into(),
                    title: "Transcript ax calls outside verbose window".into(),
                    detail: "Verbose log files exist but have no clusters in this window. Widen `--window-minutes`, pass `--session <uuid>`, or enable Verbose MCP logging and restart MCP."
                        .into(),
                    waste_hint: "Window mismatch blocks enrichment measurement for this slice.".into(),
                    tokens_est: 0,
                    tool: None,
                    ts_ms: None,
                    log_line_hint: None,
                });
            }
        } else {
            score -= 20;
            findings.push(Finding {
                id: "uncorrelated-tool".into(),
                check: "UncorrelatedTool".into(),
                severity: "critical".into(),
                title: "Ax tools in transcript but no verbose log".into(),
                detail: "Enable Verbose MCP logging in Settings → Interface to close the quality loop."
                    .into(),
                waste_hint: "Without verbose traces, enrichment quality cannot be measured.".into(),
                tokens_est: 0,
                tool: None,
                ts_ms: None,
                log_line_hint: None,
            });
        }
    }

    // Error rate — ignore client mistakes that recovered with a successful retry.
    let error_clusters = unrecovered_error_clusters(clusters);
    let errors = error_clusters.len();
    if errors > 0 {
        score -= (errors as i32 * 3).min(12);
        let samples: Vec<String> = error_clusters
            .iter()
            .take(3)
            .map(|c| {
                let msg = c
                    .lines
                    .iter()
                    .find(|l| l.starts_with("error ") || l.contains(" error "))
                    .cloned()
                    .unwrap_or_else(|| c.tool.clone());
                truncate_hint(&msg, 120)
            })
            .collect();
        let hint = error_clusters
            .first()
            .and_then(|c| {
                c.lines
                    .iter()
                    .find(|l| l.starts_with("error ") || l.contains(" error "))
                    .cloned()
            });
        findings.push(Finding {
            id: "mcp-errors".into(),
            check: "VerboseGap".into(),
            severity: if errors > 3 { "high" } else { "medium" }.into(),
            title: format!("{errors} MCP error cluster(s)"),
            detail: format!(
                "Unrecovered errors may force agent retries and wasted tokens. Samples: {}",
                samples.join(" · ")
            ),
            waste_hint: "Failed tools often trigger duplicate Read/Grep fallbacks.".into(),
            tokens_est: errors as i64 * 300,
            tool: error_clusters.first().map(|c| c.tool.clone()),
            ts_ms: error_clusters.first().map(|c| c.ts_ms),
            log_line_hint: hint,
        });
        tokens_at_risk += errors as i64 * 300;
    }

    let score = score.clamp(0, 100) as u8;
    findings.sort_by(|a, b| {
        severity_rank(&b.severity)
            .cmp(&severity_rank(&a.severity))
            .then(b.tokens_est.cmp(&a.tokens_est))
    });
    (score, findings, tokens_at_risk)
}

fn severity_rank(s: &str) -> u8 {
    match s {
        "critical" => 4,
        "high" => 3,
        "medium" => 2,
        "low" => 1,
        _ => 0,
    }
}

/// Errors that were followed by a successful same-tool call within 60s are treated as recovered.
fn unrecovered_error_clusters(clusters: &[VerboseCluster]) -> Vec<&VerboseCluster> {
    const RECOVER_MS: i64 = 60_000;
    clusters
        .iter()
        .filter(|c| {
            if !c.has_error {
                return false;
            }
            !clusters.iter().any(|other| {
                other.tool == c.tool
                    && !other.has_error
                    && other.has_outbound
                    && other.ts_ms >= c.ts_ms
                    && other.ts_ms - c.ts_ms <= RECOVER_MS
            })
        })
        .collect()
}

fn truncate_hint(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let trimmed: String = s.chars().take(max_chars.saturating_sub(1)).collect();
    format!("{trimmed}…")
}

fn command_on_path(bin: &str) -> bool {
    #[cfg(windows)]
    let lookup = format!("{bin}.exe");
    #[cfg(not(windows))]
    let lookup = bin.to_string();
    let Some(path) = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(&lookup))
            .find(|p| p.is_file())
    }) else {
        return false;
    };
    // rustup shims exist on PATH but exit non-zero until `rustup component add …`.
    std::process::Command::new(&path)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn grade_for(score: u8) -> String {
    match score {
        90..=100 => "A".into(),
        80..=89 => "B".into(),
        70..=79 => "C".into(),
        60..=69 => "D".into(),
        _ => "F".into(),
    }
}

fn verbose_enabled_for(project_root: &Path) -> bool {
    if std::env::var("AX_MCP_VERBOSE")
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
    {
        return true;
    }
    let path = project_root.join(".ax").join("ship.toml");
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    text.lines().any(|l| {
        let t = l.trim();
        t.starts_with("verbose_mcp")
            && (t.contains("true") || t.ends_with("=1") || t.contains("= true"))
    })
}

fn enrichment_from(clusters: &[VerboseCluster]) -> EnrichmentMetrics {
    let mut injects: Vec<i64> = Vec::new();
    let mut empty = 0usize;
    let mut matched = 0usize;
    let mut matched_known = 0usize;
    let mut preflight = 0usize;
    let mut inbound = 0usize;
    for c in clusters {
        if c.has_inbound {
            inbound += 1;
        }
        // Only score real preflight calls (inbound), not enrich-only fragments.
        if c.tool == "ax_preflight" && c.has_inbound {
            preflight += 1;
            if let Some(n) = c.inject_chars {
                injects.push(n);
                if n == 0 {
                    empty += 1;
                }
            } else {
                // Missing inject metric after inbound — treat as weak only when
                // there was also no enrich done line in the cluster.
                let has_enrich_done = c.lines.iter().any(|l| l.contains("enrich done"));
                if !has_enrich_done {
                    empty += 1;
                }
            }
            if let Some(m) = c.matched_rules {
                matched_known += 1;
                if m {
                    matched += 1;
                }
            } else if c.inject_chars.unwrap_or(0) > 0
                || c.lines.iter().any(|l| l.contains("matched_rules="))
            {
                matched_known += 1;
                matched += 1;
            }
        }
    }
    injects.sort_unstable();
    EnrichmentMetrics {
        inject_chars_p50: percentile(&injects, 0.50),
        inject_chars_p95: percentile(&injects, 0.95),
        enrich_done_rate: if preflight == 0 {
            0.0
        } else {
            1.0 - (empty as f64 / preflight as f64)
        },
        empty_enrich_count: empty,
        matched_rules_rate: if matched_known == 0 {
            0.0
        } else {
            matched as f64 / matched_known as f64
        },
        preflight_count: preflight,
        inbound_count: inbound,
    }
}

/// Run a quality audit for a project (rolling window or full session).
pub fn audit_project(project_root: &Path, opts: &AuditOptions) -> Result<QualitySnapshot, String> {
    let window = opts.window_minutes.unwrap_or(DEFAULT_WINDOW_MINUTES);
    let explicit_session = opts.session_path.is_some() || opts.session_id.is_some();
    let since_ms = if explicit_session {
        0
    } else {
        now_ms().saturating_sub((window as i64) * 60_000)
    };

    let verbose_enabled = verbose_enabled_for(project_root);
    let log_path = crate::mcp_verbose_log::current_log_path(Some(project_root));
    let verbose_text = crate::mcp_verbose_log::read_merged_verbose_log(project_root);
    let verbose_present = !verbose_text.trim().is_empty();
    let (lines, mut clusters) = parse_verbose_log(&verbose_text, since_ms);
    let domain_blob = domain_blob_from(&lines);

    let session_path = resolve_session_path(opts, project_root);
    let mut events = Vec::new();
    let mut session_id = None;
    if let Some(ref sp) = session_path {
        events = load_transcript_events(sp, since_ms)?;
        session_id = sp
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
    }

    // Rolling-window + untimed transcript: if verbose is quiet in-window, drop the
    // whole-session transcript tail so we don't false-flag UncorrelatedTool /
    // VerboseGap while verbose logging is actually healthy.
    let transcripts_timed = events.iter().any(|e| e.ts_ms > 0);
    if !explicit_session && since_ms > 0 && !transcripts_timed && clusters.is_empty() {
        events.clear();
    }

    // Rolling window + verbose on + no MCP clusters: timed CallMcpTool rows are often
    // failed "Not connected" attempts after reinstall/restart. Drop them so the live
    // Q chip doesn't false-flag UncorrelatedTool while logging is correctly configured.
    if !explicit_session && since_ms > 0 && clusters.is_empty() && verbose_enabled {
        events.retain(|e| e.kind != TranscriptKind::AxTool);
    }

    // When verbose lines carry session=, keep only clusters for the audited session.
    if let Some(ref sid) = session_id {
        let has_tagged = clusters
            .iter()
            .any(|c| c.session_id.as_deref() == Some(sid.as_str()));
        if has_tagged {
            clusters.retain(|c| c.session_id.as_deref() == Some(sid.as_str()));
        }
    }

    let mode = if session_path.is_some() && !events.is_empty() {
        "transcript_linked"
    } else {
        "verbose_only"
    };

    let mix = tool_mix_from(&clusters, &events);
    let enrichment = enrichment_from(&clusters);
    let (matched, unmatched, corr_pct, _) = correlate(&clusters, &events);
    let correlation_pct = if mode == "verbose_only" {
        // Idle window (no clusters) is healthy — not a 0% correlation failure.
        100.0
    } else {
        corr_pct
    };

    let (score, findings, tokens_at_risk) = score_and_findings(
        &clusters,
        &events,
        &mix,
        &enrichment,
        correlation_pct,
        mode,
        verbose_present,
        verbose_enabled,
        &domain_blob,
    );
    let critical_count = findings
        .iter()
        .filter(|f| f.severity == "critical")
        .count();

    let snap = QualitySnapshot {
        project_root: project_root.display().to_string(),
        project_label: project_label(project_root),
        log_path: log_path.display().to_string(),
        mode: mode.into(),
        window_minutes: window,
        updated_at_ms: now_ms(),
        score,
        grade: grade_for(score),
        correlation_pct,
        matched_calls: matched,
        unmatched_ax_calls: unmatched,
        verbose_clusters: clusters.len(),
        enrichment,
        tool_mix: mix,
        findings,
        tokens_at_risk,
        critical_count,
        verbose_enabled,
        verbose_present,
        session_id,
        session_path: session_path.map(|p| p.display().to_string()),
    };

    if opts.persist {
        let _ = persist_snapshot(project_root, &snap);
    }
    Ok(snap)
}

pub fn persist_snapshot(project_root: &Path, snap: &QualitySnapshot) -> Result<(), String> {
    let dir = audit_dir(project_root);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = latest_snapshot_path(project_root);
    let json = serde_json::to_string_pretty(snap).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

pub fn load_latest_snapshot(project_root: &Path) -> Option<QualitySnapshot> {
    let path = latest_snapshot_path(project_root);
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Markdown report for CLI / deep-dive.
pub fn format_markdown_report(snap: &QualitySnapshot) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# MCP quality audit — {}\n\n",
        snap.project_label
    ));
    out.push_str(&format!(
        "**Score:** {} ({}) · **Mode:** {} · **Window:** {}m\n\n",
        snap.score, snap.grade, snap.mode, snap.window_minutes
    ));
    out.push_str(&format!(
        "| Metric | Value |\n|--------|-------|\n| Correlation | {:.0}% |\n| Matched / unmatched | {} / {} |\n| Verbose clusters | {} |\n| Tokens at risk | {} |\n| Critical findings | {} |\n\n",
        snap.correlation_pct,
        snap.matched_calls,
        snap.unmatched_ax_calls,
        snap.verbose_clusters,
        snap.tokens_at_risk,
        snap.critical_count
    ));
    out.push_str("## Enrichment\n\n");
    out.push_str(&format!(
        "- inject_chars p50/p95: {} / {}\n- enrich-done rate: {:.0}%\n- empty enrich: {}\n- matched_rules rate: {:.0}%\n- preflight / inbound: {} / {}\n\n",
        snap.enrichment.inject_chars_p50,
        snap.enrichment.inject_chars_p95,
        snap.enrichment.enrich_done_rate * 100.0,
        snap.enrichment.empty_enrich_count,
        snap.enrichment.matched_rules_rate * 100.0,
        snap.enrichment.preflight_count,
        snap.enrichment.inbound_count
    ));
    out.push_str("## Tool mix\n\n");
    out.push_str(&format!(
        "- preflight {} · explore {} · guard {} · graph {} · other ax {}\n- Read {} · Grep {}\n\n",
        snap.tool_mix.preflight,
        snap.tool_mix.explore,
        snap.tool_mix.guard,
        snap.tool_mix.graph,
        snap.tool_mix.other_ax,
        snap.tool_mix.read,
        snap.tool_mix.grep
    ));
    if snap.findings.is_empty() {
        out.push_str("## Findings\n\n_No findings in this window._\n");
    } else {
        out.push_str("## Findings\n\n");
        for f in &snap.findings {
            out.push_str(&format!(
                "### [{}] {} — {}\n\n{}\n\n*Waste:* {} (~{} tokens)\n\n",
                f.severity, f.check, f.title, f.detail, f.waste_hint, f.tokens_est
            ));
        }
    }
    if let Some(ref sid) = snap.session_id {
        out.push_str(&format!("\n_Session:_ `{sid}`\n"));
    }
    out.push_str(&format!("\n_Log:_ `{}`\n", snap.log_path));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_sanitizes_windows_path() {
        let slug = cursor_project_slug(Path::new(r"C:\gary\ax"));
        assert_eq!(slug, "c-gary-ax");
    }

    #[test]
    fn parses_verbose_inbound() {
        let line = "2026-07-21T19:07:03.142Z [ax-mcp] inbound tool=ax_explore args={\"q\":\"x\"}";
        let parsed = parse_verbose_line(line).unwrap();
        assert_eq!(parsed.kind, "inbound");
        assert_eq!(parsed.tool, "ax_explore");
        assert!(parsed.ts_ms > 0);
    }

    #[test]
    fn scores_missing_preflight() {
        let text = "\
2026-07-21T19:07:03.142Z [ax-mcp] inbound tool=ax_explore args={}\n\
2026-07-21T19:07:03.200Z [ax-mcp] outbound tool=ax_explore mode=lean text_chars=10 structured=false duration_ms=1\n";
        let (_l, clusters) = parse_verbose_log(text, 0);
        let mix = tool_mix_from(&clusters, &[]);
        let enrichment = enrichment_from(&clusters);
        let (score, findings, _) =
            score_and_findings(&clusters, &[], &mix, &enrichment, 100.0, "verbose_only", true, true, "");
        assert!(score < 100);
        assert!(findings.iter().any(|f| f.check == "PreflightOnce"));
    }

    #[test]
    fn enrich_lines_attach_to_preflight_cluster() {
        let text = "\
2026-07-21T19:07:03.100Z [ax-mcp] inbound tool=ax_preflight args={}\n\
2026-07-21T19:07:03.101Z [ax-mcp] enrich policy matched_rules=5 matched_skills=2 inject_chars=9000 mode=database\n\
2026-07-21T19:07:03.102Z [ax-mcp] enrich done final_inject_chars=12000 directive=false\n\
2026-07-21T19:07:03.103Z [ax-mcp] internal tool=ax_preflight keys=[inject,text] inject_chars=12000 text_chars=100\n\
2026-07-21T19:07:03.104Z [ax-mcp] outbound tool=ax_preflight mode=lean text_chars=100 structured=false duration_ms=1\n";
        let (_l, clusters) = parse_verbose_log(text, 0);
        let pre = clusters
            .iter()
            .find(|c| c.tool == "ax_preflight" && c.has_inbound)
            .expect("preflight cluster");
        assert_eq!(pre.inject_chars, Some(12000));
        let enrichment = enrichment_from(&clusters);
        assert_eq!(enrichment.empty_enrich_count, 0);
        assert_eq!(enrichment.preflight_count, 1);
        assert!(enrichment.inject_chars_p50 >= 9000);
    }

    #[test]
    fn classifies_call_dynamic_tool() {
        let input = serde_json::json!({
            "namespace": "user-ax",
            "toolName": "ax_preflight",
            "arguments": { "prompt": "hi" }
        });
        let ev = classify_cursor_tool_event("CallDynamicTool", &input);
        assert_eq!(ev.kind, TranscriptKind::AxTool);
        assert_eq!(ev.ax_tool.as_deref(), Some("ax_preflight"));
    }

    #[test]
    fn skips_call_dynamic_without_tool_name() {
        let input = serde_json::json!({
            "namespace": "user-ax",
            "arguments": {}
        });
        let ev = classify_cursor_tool_event("CallDynamicTool", &input);
        assert_eq!(ev.kind, TranscriptKind::Other);
    }

    #[test]
    fn parses_verbose_session_id() {
        let line = "2026-07-22T11:32:18.722Z [ax-mcp] error tool=ax_guard message=path required session=abc-123";
        let parsed = parse_verbose_line(line).unwrap();
        assert_eq!(parsed.kind, "error");
        assert_eq!(parsed.tool, "ax_guard");
        assert_eq!(parsed.session_id.as_deref(), Some("abc-123"));
    }

    #[test]
    fn recovered_guard_error_does_not_penalize() {
        let text = "\
2026-07-22T11:32:18.722Z [ax-mcp] inbound tool=ax_guard args={\"paths\":[\"a.rs\"]}\n\
2026-07-22T11:32:18.722Z [ax-mcp] error tool=ax_guard message=path required\n\
2026-07-22T11:32:33.948Z [ax-mcp] inbound tool=ax_guard args={\"path\":\"a.rs\",\"operation\":\"write\"}\n\
2026-07-22T11:32:33.948Z [ax-mcp] outbound tool=ax_guard mode=lean text_chars=32 structured=true duration_ms=1\n\
2026-07-22T11:32:17.000Z [ax-mcp] inbound tool=ax_preflight args={}\n\
2026-07-22T11:32:17.001Z [ax-mcp] enrich policy matched_rules=1 matched_skills=0 inject_chars=100 mode=database\n\
2026-07-22T11:32:17.002Z [ax-mcp] enrich done final_inject_chars=100 directive=false\n\
2026-07-22T11:32:17.003Z [ax-mcp] outbound tool=ax_preflight mode=lean text_chars=100 structured=false duration_ms=1\n";
        let (_l, clusters) = parse_verbose_log(text, 0);
        let mix = tool_mix_from(&clusters, &[]);
        let enrichment = enrichment_from(&clusters);
        let (_score, findings, _) =
            score_and_findings(&clusters, &[], &mix, &enrichment, 100.0, "verbose_only", true, true, "");
        assert!(
            !findings.iter().any(|f| f.id == "mcp-errors"),
            "recovered ax_guard error should not create VerboseGap finding"
        );
    }

    #[test]
    fn parses_cursor_embed_timestamp() {
        let line = r#"{"role":"user","message":{"content":[{"type":"text","text":"<timestamp>Tuesday, Jul 21, 2026, 7:31 PM (UTC+2)</timestamp>\nhello"}]}}"#;
        let ts = parse_cursor_embed_timestamp(line).expect("embed ts");
        // 2026-07-21 19:31 UTC+2 = 17:31 UTC
        assert!(ts > 0);
        let utc = chrono::DateTime::from_timestamp_millis(ts).expect("utc");
        assert_eq!(utc.format("%Y-%m-%d %H:%M").to_string(), "2026-07-21 17:31");
    }

    #[test]
    fn carry_forward_embed_ts_filters_old_events() {
        let path = std::env::temp_dir().join(format!(
            "ax-mcp-audit-embed-{}.jsonl",
            std::process::id()
        ));
        let body = r#"{"role":"user","message":{"content":[{"type":"text","text":"<timestamp>Tuesday, Jul 21, 2026, 7:31 PM (UTC+2)</timestamp>"}]}}
{"role":"assistant","message":{"content":[{"type":"tool_use","name":"CallDynamicTool","input":{"namespace":"user-ax","toolName":"ax_preflight","arguments":{"prompt":"x"}}}]}}
"#;
        std::fs::write(&path, body).unwrap();
        let events = load_transcript_events(&path, 9_999_999_999_999).unwrap();
        let _ = std::fs::remove_file(&path);
        assert!(
            events.is_empty(),
            "embed timestamps should exclude old tool calls from a future since_ms"
        );
        std::fs::write(&path, body).unwrap();
        let events_all = load_transcript_events(&path, 0).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(events_all.len(), 1);
        assert_eq!(events_all[0].ax_tool.as_deref(), Some("ax_preflight"));
        assert!(events_all[0].ts_ms > 0);
    }

    #[test]
    fn uncorrelated_softens_when_verbose_present() {
        let events = vec![TranscriptEvent {
            ts_ms: 1,
            kind: TranscriptKind::AxTool,
            name: "ax_preflight".into(),
            ax_tool: Some("ax_preflight".into()),
        }];
        let mix = ToolMix::default();
        let enrichment = EnrichmentMetrics::default();
        // verbose present but not enabled → medium
        let (_score, findings, _) = score_and_findings(
            &[],
            &events,
            &mix,
            &enrichment,
            0.0,
            "transcript_linked",
            true,
            false,
            "",
        );
        let f = findings
            .iter()
            .find(|f| f.id == "uncorrelated-tool")
            .expect("finding");
        assert_eq!(f.severity, "medium");
        assert!(f.detail.contains("window"));
    }

    #[test]
    fn uncorrelated_is_info_when_verbose_enabled() {
        let events = vec![TranscriptEvent {
            ts_ms: 1,
            kind: TranscriptKind::AxTool,
            name: "ax_preflight".into(),
            ax_tool: Some("ax_preflight".into()),
        }];
        let mix = ToolMix::default();
        let enrichment = EnrichmentMetrics::default();
        let (score, findings, _) = score_and_findings(
            &[],
            &events,
            &mix,
            &enrichment,
            0.0,
            "transcript_linked",
            true,
            true,
            "",
        );
        assert_eq!(score, 100, "verbose-enabled idle MCP must not penalize score");
        let f = findings
            .iter()
            .find(|f| f.id == "uncorrelated-tool")
            .expect("info finding");
        assert_eq!(f.severity, "info");
    }

    #[test]
    fn domain_lines_do_not_form_tool_clusters() {
        let text = "\
2026-07-26T13:02:14.805Z [ax] workspace switch path=C:\\gary\\ax\n\
2026-07-26T13:02:15.000Z [ax] lsp enrich start limit=200\n";
        let (lines, clusters) = parse_verbose_log(text, 0);
        assert_eq!(clusters.len(), 0, "domain lines must not create MCP clusters");
        let blob = domain_blob_from(&lines);
        assert!(blob.contains("lsp enrich"));
        assert!(blob.contains("workspace switch"));
    }

    #[test]
    fn quiet_mcp_skips_explore_before_grep_and_verbose_gap() {
        let events = vec![
            TranscriptEvent {
                ts_ms: 1,
                kind: TranscriptKind::AxTool,
                name: "ax_preflight".into(),
                ax_tool: Some("ax_preflight".into()),
            },
            TranscriptEvent {
                ts_ms: 2,
                kind: TranscriptKind::Read,
                name: "Read".into(),
                ax_tool: None,
            },
            TranscriptEvent {
                ts_ms: 3,
                kind: TranscriptKind::Grep,
                name: "Grep".into(),
                ax_tool: None,
            },
        ];
        let mix = ToolMix {
            preflight: 1,
            read: 1,
            grep: 1,
            ..ToolMix::default()
        };
        let enrichment = EnrichmentMetrics::default(); // inbound_count = 0
        let (_score, findings, _) = score_and_findings(
            &[],
            &events,
            &mix,
            &enrichment,
            0.0,
            "transcript_linked",
            true,
            true,
            "[ax] workspace switch path=x",
        );
        assert!(
            !findings.iter().any(|f| f.id == "explore-before-grep"),
            "DEGRADED/quiet MCP must not flag ExploreBeforeGrep"
        );
        assert!(
            !findings.iter().any(|f| f.id == "verbose-gap"),
            "empty MCP clusters should not also fire VerboseGap"
        );
        assert!(
            !findings.iter().any(|f| f.id == "lsp-available-unused"),
            "domain-only window must not flag LspAvailableUnused"
        );
        assert!(findings.iter().any(|f| f.id == "uncorrelated-tool"));
    }

    #[test]
    fn markdown_includes_score() {
        let snap = QualitySnapshot {
            project_root: "/tmp/p".into(),
            project_label: "p".into(),
            log_path: "/tmp/p/.ax/mcp-verbose.log".into(),
            mode: "verbose_only".into(),
            window_minutes: 30,
            updated_at_ms: 0,
            score: 82,
            grade: "B".into(),
            correlation_pct: 100.0,
            matched_calls: 0,
            unmatched_ax_calls: 0,
            verbose_clusters: 1,
            enrichment: EnrichmentMetrics::default(),
            tool_mix: ToolMix::default(),
            findings: vec![],
            tokens_at_risk: 0,
            critical_count: 0,
            verbose_enabled: true,
            verbose_present: true,
            session_id: None,
            session_path: None,
        };
        let md = format_markdown_report(&snap);
        assert!(md.contains("82"));
        assert!(md.contains("**Score:**"));
    }
}
