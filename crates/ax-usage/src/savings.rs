//! Context-token savings estimation, MCP audit log, and agent session import.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use serde_json::Value;
use walkdir::WalkDir;

use crate::cursor_state::{import_cursor_composer_state, normalize_cursor_model};
use crate::period::{resolve_period, UsagePeriod};
use crate::pricing::{
    input_cost_usd, price_as_of, price_for_model, price_for_model_with_source, pricing_info,
    reference_pricing, reference_pricing_with_source, refresh_price_cache_from_db, PricingInfo,
};
use crate::store::{open_pool, usage_db_path};
use crate::tokenizer::{
    count_file_line_range_tokens, count_file_tokens, count_tokens, tokenize_text,
    tokenizer_available, truncate_utf8, TokenizeResult,
};

/// Max bytes stored per preview column for token-chip drill-down.
pub const PREVIEW_MAX_BYTES: usize = 4096;

const GRAPH_TOOLS: &[&str] = &[
    "ax_explore",
    "ax_context",
    "ax_node",
    "ax_search",
    "ax_callers",
    "ax_callees",
    "ax_impact",
    "ax_affected",
];

/// JSON array keys whose string elements are project-relative file paths.
const FILE_PATH_ARRAY_KEYS: &[&str] = &[
    "relatedFiles",
    "related_files",
    "files",
    "affected",
    "pendingFiles",
    "pending_files",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CounterfactualMode {
    /// Whole-file Read baseline (default — largest legitimate savings vs Cursor Read).
    Full,
    /// Symbol line span when start+end are known.
    Range,
    /// Per file: max(whole file, line span).
    Max,
}

fn counterfactual_mode() -> CounterfactualMode {
    match std::env::var("AX_SAVINGS_CF_MODE").ok().as_deref() {
        Some(s) if s.eq_ignore_ascii_case("range") => CounterfactualMode::Range,
        Some(s) if s.eq_ignore_ascii_case("max") => CounterfactualMode::Max,
        _ => CounterfactualMode::Full,
    }
}

fn counterfactual_mode_label() -> &'static str {
    match counterfactual_mode() {
        CounterfactualMode::Full => "full",
        CounterfactualMode::Range => "range",
        CounterfactualMode::Max => "max",
    }
}

pub fn is_savings_eligible_tool(tool: &str) -> bool {
    GRAPH_TOOLS.contains(&tool)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn chars_per_token() -> usize {
    env_usize("AX_SAVINGS_CHARS_PER_TOKEN", 4).max(1)
}

fn tokens_per_line() -> i64 {
    env_usize("AX_SAVINGS_TOKENS_PER_LINE", 9).max(1) as i64
}

fn avg_file_tokens() -> i64 {
    env_usize("AX_SAVINGS_AVG_FILE_TOKENS", 3500).max(1) as i64
}

fn json_i64(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_u64().and_then(|u| i64::try_from(u).ok()))
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
}

fn file_path_from_obj(map: &serde_json::Map<String, Value>) -> Option<&str> {
    map.get("filePath")
        .or_else(|| map.get("file_path"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

fn start_line_from_obj(map: &serde_json::Map<String, Value>) -> Option<i64> {
    map.get("startLine")
        .or_else(|| map.get("start_line"))
        .and_then(json_i64)
}

fn end_line_from_obj(map: &serde_json::Map<String, Value>) -> Option<i64> {
    map.get("endLine")
        .or_else(|| map.get("end_line"))
        .and_then(json_i64)
}

#[derive(Debug, Clone, Default)]
struct FileSpan {
    min_start: i64,
    max_end: i64,
    has_start: bool,
    has_end: bool,
    /// Measured tokens from an inline `content` field (e.g. ax_context code blocks).
    content_fallback_tokens: i64,
}

impl FileSpan {
    fn merge(&mut self, start: Option<i64>, end: Option<i64>) {
        if let Some(s) = start.filter(|&n| n > 0) {
            self.min_start = if self.has_start {
                self.min_start.min(s)
            } else {
                s
            };
            self.has_start = true;
        }
        if let Some(e) = end.filter(|&n| n > 0) {
            self.max_end = self.max_end.max(e);
            self.has_end = true;
        }
    }

    fn has_line_span(&self) -> bool {
        self.has_start && self.has_end && self.max_end >= self.min_start
    }
}

fn merge_file_span(files: &mut HashMap<String, FileSpan>, path: &str, start: Option<i64>, end: Option<i64>) {
    files
        .entry(path.to_string())
        .and_modify(|span| span.merge(start, end))
        .or_insert_with(|| {
            let mut span = FileSpan::default();
            span.merge(start, end);
            span
        });
}

fn collect_file_refs(value: &Value, files: &mut HashMap<String, FileSpan>) {
    match value {
        Value::Object(map) => {
            if let Some(fp) = file_path_from_obj(map) {
                merge_file_span(
                    files,
                    fp,
                    start_line_from_obj(map),
                    end_line_from_obj(map),
                );
                if let Some(content) = map.get("content").and_then(|v| v.as_str()) {
                    if !content.is_empty() {
                        let tokens = count_tokens(content) as i64;
                        files
                            .entry(fp.to_string())
                            .and_modify(|span| {
                                span.content_fallback_tokens =
                                    span.content_fallback_tokens.max(tokens);
                            })
                            .or_insert_with(|| {
                                let mut span = FileSpan::default();
                                span.content_fallback_tokens = tokens;
                                span
                            });
                    }
                }
            }
            for (key, v) in map {
                if FILE_PATH_ARRAY_KEYS.iter().any(|k| k == key) {
                    if let Value::Array(arr) = v {
                        for item in arr {
                            if let Some(path) = item.as_str().filter(|s| !s.is_empty()) {
                                merge_file_span(files, path, None, None);
                            } else {
                                collect_file_refs(item, files);
                            }
                        }
                        continue;
                    }
                }
                collect_file_refs(v, files);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_file_refs(v, files);
            }
        }
        _ => {}
    }
}

fn heuristic_file_tokens(span: &FileSpan, tpl: i64, fallback: i64) -> i64 {
    if span.has_end {
        let lines = if span.has_line_span() {
            span.max_end - span.min_start + 1
        } else {
            span.max_end
        };
        lines.max(1) * tpl
    } else if span.content_fallback_tokens > 0 {
        span.content_fallback_tokens
    } else {
        fallback
    }
}

fn counterfactual_tokens_for_file(
    resolved: &Path,
    span: &FileSpan,
    tpl: i64,
    fallback: i64,
) -> (i64, bool) {
    let full = count_file_tokens(resolved);
    let range = if span.has_line_span() {
        count_file_line_range_tokens(
            resolved,
            span.min_start as u32,
            span.max_end as u32,
        )
    } else {
        None
    };
    let heuristic = heuristic_file_tokens(span, tpl, fallback);
    let content = span.content_fallback_tokens;

    let pick = |prefer_range: bool| -> (i64, bool) {
        if prefer_range {
            if let Some(t) = range {
                return (t, true);
            }
        }
        if let Some(t) = full {
            return (t, true);
        }
        if content > 0 {
            return (content, true);
        }
        (heuristic, false)
    };

    match counterfactual_mode() {
        CounterfactualMode::Full => pick(false),
        CounterfactualMode::Range => pick(true),
        CounterfactualMode::Max => {
            let mut best = heuristic;
            let mut exact = false;
            if let Some(t) = full {
                best = best.max(t);
                exact = true;
            }
            if let Some(t) = range {
                best = best.max(t);
                exact = true;
            }
            if content > 0 {
                best = best.max(content);
                exact = true;
            }
            (best, exact)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct SavingsEstimate {
    pub counterfactual_files: i64,
    /// Files whose counterfactual tokens were measured by tokenizing the
    /// actual file contents (as opposed to the line-count heuristic).
    pub counterfactual_exact_files: i64,
    pub counterfactual_tokens_est: i64,
    pub response_tokens_est: i64,
    pub tokens_saved_est: i64,
    pub savings_eligible: bool,
    /// Truncated MCP response text for token-chip visualization.
    pub response_preview: Option<String>,
    /// Truncated counterfactual file contents for token-chip visualization.
    pub counterfactual_preview: Option<String>,
}

fn read_span_preview(path: &Path, span: &FileSpan) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    if span.has_line_span() {
        let lines: Vec<&str> = text.lines().collect();
        let start_idx = (span.min_start.saturating_sub(1)) as usize;
        let end_idx = (span.max_end as usize).min(lines.len());
        if start_idx < end_idx {
            return Some(lines[start_idx..end_idx].join("\n"));
        }
    }
    Some(text.into_owned())
}

fn build_counterfactual_preview(
    files: &HashMap<String, FileSpan>,
    project_root: Option<&Path>,
) -> Option<String> {
    if files.is_empty() {
        return None;
    }
    let mut out = String::new();
    let mut paths: Vec<&String> = files.keys().collect();
    paths.sort();
    for file in paths {
        if out.len() >= PREVIEW_MAX_BYTES {
            break;
        }
        let Some(span) = files.get(file) else {
            continue;
        };
        let resolved = resolve_file_path(file, project_root);
        let Some(chunk) = read_span_preview(&resolved, span) else {
            continue;
        };
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        let remaining = PREVIEW_MAX_BYTES.saturating_sub(out.len());
        if remaining == 0 {
            break;
        }
        out.push_str(&truncate_utf8(&chunk, remaining));
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Resolve a file path from a graph response against the project root.
fn resolve_file_path(path: &str, project_root: Option<&Path>) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    match project_root {
        Some(root) => root.join(p),
        None => p.to_path_buf(),
    }
}

/// Estimate context savings for one MCP call.
///
/// Response tokens are measured with the o200k BPE tokenizer. The
/// counterfactual (what Read/Grep without ax would have cost) uses whole-file
/// BPE when `AX_SAVINGS_CF_MODE=full` (default), symbol line spans when
/// `range`, or the per-file max when `max`. Unreadable files fall back to line
/// heuristics, inline `content` snippets, then avg file size.
pub fn estimate_savings(
    tool: &str,
    structured: &Value,
    response_text: &str,
    project_root: Option<&Path>,
) -> SavingsEstimate {
    let response_tokens_est = count_tokens(response_text) as i64;
    let savings_eligible = is_savings_eligible_tool(tool);
    let response_preview = if response_text.is_empty() {
        None
    } else {
        Some(truncate_utf8(response_text, PREVIEW_MAX_BYTES))
    };

    if !savings_eligible {
        return SavingsEstimate {
            response_tokens_est,
            savings_eligible: false,
            response_preview,
            ..Default::default()
        };
    }

    let mut files: HashMap<String, FileSpan> = HashMap::new();
    collect_file_refs(structured, &mut files);

    let tpl = tokens_per_line();
    let fallback = avg_file_tokens();
    let mut counterfactual_tokens_est: i64 = 0;
    let mut counterfactual_exact_files: i64 = 0;
    for (file, span) in &files {
        let resolved = resolve_file_path(file, project_root);
        let (tokens, exact) = counterfactual_tokens_for_file(&resolved, span, tpl, fallback);
        counterfactual_tokens_est += tokens;
        if exact {
            counterfactual_exact_files += 1;
        }
    }

    let counterfactual_files = files.len() as i64;
    let tokens_saved_est = (counterfactual_tokens_est - response_tokens_est).max(0);
    let counterfactual_preview = build_counterfactual_preview(&files, project_root);

    SavingsEstimate {
        counterfactual_files,
        counterfactual_exact_files,
        counterfactual_tokens_est,
        response_tokens_est,
        tokens_saved_est,
        savings_eligible: true,
        response_preview,
        counterfactual_preview,
    }
}

#[derive(Debug, Clone)]
pub struct McpCallRecord {
    pub tool: String,
    pub project: Option<String>,
    pub response_chars: i64,
    pub response_tokens_est: i64,
    pub counterfactual_files: Option<i64>,
    pub counterfactual_exact_files: Option<i64>,
    pub counterfactual_tokens_est: Option<i64>,
    pub tokens_saved_est: Option<i64>,
    pub duration_ms: Option<i64>,
    pub ok: bool,
    pub savings_eligible: bool,
    pub response_preview: Option<String>,
    pub counterfactual_preview: Option<String>,
}

pub async fn record_mcp_call(record: McpCallRecord) {
    if let Ok(pool) = open_pool().await {
        let now = chrono::Utc::now().timestamp_millis();
        let _ = sqlx::query(
            "INSERT INTO mcp_call_log
             (tool, project, response_chars, response_tokens_est, counterfactual_files,
              counterfactual_exact_files, counterfactual_tokens_est, tokens_saved_est,
              duration_ms, ok, savings_eligible, response_preview, counterfactual_preview,
              created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record.tool)
        .bind(&record.project)
        .bind(record.response_chars)
        .bind(record.response_tokens_est)
        .bind(record.counterfactual_files)
        .bind(record.counterfactual_exact_files)
        .bind(record.counterfactual_tokens_est)
        .bind(record.tokens_saved_est)
        .bind(record.duration_ms)
        .bind(i64::from(record.ok))
        .bind(i64::from(record.savings_eligible))
        .bind(&record.response_preview)
        .bind(&record.counterfactual_preview)
        .bind(now)
        .execute(&pool)
        .await;
    }
}

pub fn spawn_record_mcp_call(record: McpCallRecord) {
    tokio::spawn(async move {
        record_mcp_call(record).await;
    });
}

#[derive(Debug, Clone)]
pub struct SavingsQuery {
    pub period: UsagePeriod,
    pub from: Option<String>,
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolSavingsRow {
    pub tool: String,
    pub calls: i64,
    /// Graph-tool invocations (savings-eligible subset of `calls`).
    pub graph_calls: i64,
    pub failed_calls: i64,
    pub tokens_saved_est: i64,
    pub counterfactual_files: i64,
    pub counterfactual_tokens_est: i64,
    pub graph_response_tokens_est: i64,
    /// Average wall time when `duration_ms` was recorded.
    pub avg_duration_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailySavings {
    pub date: String,
    pub tokens_saved_est: i64,
    pub calls: i64,
    pub graph_calls: i64,
    pub failed_calls: i64,
    pub counterfactual_files: i64,
    pub counterfactual_tokens_est: i64,
    pub graph_response_tokens_est: i64,
    pub cost_saved_usd_est: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeekdaySavingsRow {
    /// 0 = Sunday … 6 = Saturday (SQLite `%w`).
    pub weekday: i64,
    pub label: String,
    pub tokens_saved_est: i64,
    pub calls: i64,
    pub graph_calls: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HourSavingsRow {
    /// 0–23 local hour of day.
    pub hour: i64,
    pub label: String,
    pub tokens_saved_est: i64,
    pub calls: i64,
    pub graph_calls: i64,
}

/// Hourly (or finer) savings buckets for navigable time charts.
#[derive(Debug, Clone, Serialize)]
pub struct TimelineBucket {
    /// Local timestamp label, e.g. `2026-07-21 14:00`.
    pub bucket: String,
    pub tokens_saved_est: i64,
    pub calls: i64,
    pub graph_calls: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProjectSavingsRow {
    pub project: String,
    pub calls: i64,
    pub graph_calls: i64,
    pub tokens_saved_est: i64,
    pub counterfactual_files: i64,
}

/// Savings and session spend grouped by agent model (from imported transcripts).
#[derive(Debug, Clone, Serialize)]
pub struct ModelSavingsRow {
    pub model: String,
    pub sessions: i64,
    pub session_input_tokens: i64,
    pub tokens_saved_est: i64,
    pub ax_calls: i64,
    pub read_calls: i64,
    pub grep_calls: i64,
    pub session_cost_usd_est: f64,
    pub cost_saved_usd_est: f64,
    /// Input $/MTok used for this model's cost estimates.
    pub input_per_mtok: f64,
    /// Pricing source: user | openrouter | artificial_analysis | default.
    pub pricing_source: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RecentCallRow {
    pub id: i64,
    pub tool: String,
    pub project: Option<String>,
    pub tokens_saved_est: i64,
    pub counterfactual_tokens_est: i64,
    pub response_tokens_est: i64,
    pub counterfactual_files: i64,
    pub ok: bool,
    pub savings_eligible: bool,
    pub duration_ms: Option<i64>,
    pub created_at: i64,
    /// True when a response or counterfactual preview is stored for chip view.
    pub has_preview: bool,
}

/// Per-call token-chip payload for the Savings detail blade.
#[derive(Debug, Clone, Serialize)]
pub struct CallTokenDetail {
    pub id: i64,
    pub tool: String,
    pub project: Option<String>,
    pub tokens_saved_est: i64,
    pub counterfactual_tokens_est: i64,
    pub response_tokens_est: i64,
    pub counterfactual_files: i64,
    pub ok: bool,
    pub savings_eligible: bool,
    pub duration_ms: Option<i64>,
    pub created_at: i64,
    pub response_preview: Option<String>,
    pub counterfactual_preview: Option<String>,
    pub response_tokens: TokenizeResult,
    pub counterfactual_tokens: TokenizeResult,
}

/// Estimation constants in effect (defaults or `AX_SAVINGS_*` env overrides).
#[derive(Debug, Clone, Serialize)]
pub struct SavingsAssumptions {
    /// True when the o200k BPE tokenizer is active — response tokens and
    /// counterfactuals for readable files are then measured, not estimated.
    pub exact_tokenizer: bool,
    /// Fallback: characters per token when the tokenizer is unavailable (`AX_SAVINGS_CHARS_PER_TOKEN`).
    pub chars_per_token: usize,
    /// Fallback: tokens per source line for unreadable files (`AX_SAVINGS_TOKENS_PER_LINE`).
    pub tokens_per_line: i64,
    /// Fallback: tokens per file when no line count is known (`AX_SAVINGS_AVG_FILE_TOKENS`).
    pub avg_file_tokens: i64,
    /// Counterfactual baseline: `full` (whole file), `range` (symbol span), or `max`.
    pub counterfactual_mode: String,
}

pub fn current_assumptions() -> SavingsAssumptions {
    SavingsAssumptions {
        exact_tokenizer: tokenizer_available(),
        chars_per_token: chars_per_token(),
        tokens_per_line: tokens_per_line(),
        avg_file_tokens: avg_file_tokens(),
        counterfactual_mode: counterfactual_mode_label().to_string(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSessionRow {
    pub agent: String,
    pub session_id: String,
    pub read_calls: i64,
    pub grep_calls: i64,
    pub ax_calls: i64,
    pub session_input_tokens: Option<i64>,
    pub session_output_tokens: Option<i64>,
    /// Model id captured from the session transcript, when available.
    pub model: Option<String>,
    /// USD cost of the session's input tokens at the model's price.
    pub session_cost_usd_est: Option<f64>,
    /// MCP calls logged during this session's time window.
    pub mcp_calls_in_window: i64,
    /// Tokens saved by graph calls during this session's time window.
    pub tokens_saved_in_window: i64,
    pub started_at: Option<i64>,
    pub ended_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SavingsSummary {
    pub period: UsagePeriod,
    pub from: String,
    pub to: String,
    pub from_ms: i64,
    pub to_ms: i64,
    /// All logged MCP calls in the period (graph + policy tools, incl. failed calls).
    pub mcp_calls: i64,
    /// Graph-tool calls only — the subset that can produce savings.
    pub graph_calls: i64,
    /// Calls that returned an error (never counted as savings).
    pub failed_calls: i64,
    /// Sum of per-call savings, each clamped at 0 (a call never costs "negative" savings).
    pub tokens_saved_est: i64,
    /// Unclamped net difference: counterfactual − graph response. Can be lower than
    /// `tokens_saved_est` when individual responses exceeded their counterfactual.
    pub net_tokens_saved_est: i64,
    pub counterfactual_files: i64,
    /// Estimated tokens if agents had read full files (graph tools only).
    pub counterfactual_tokens_est: i64,
    /// Actual graph MCP response tokens (graph tools only).
    pub graph_response_tokens_est: i64,
    /// All MCP tool response tokens (includes policy tools).
    pub response_tokens_est: i64,
    /// Files whose counterfactual tokens were measured exactly (BPE over file contents).
    pub counterfactual_exact_files: i64,
    /// USD saved: `tokens_saved_est` priced at the reference model's input rate.
    pub cost_saved_usd_est: f64,
    /// USD spent on graph responses at the reference model's input rate.
    pub graph_response_cost_usd_est: f64,
    /// USD the counterfactual full-file reads would have cost.
    pub counterfactual_cost_usd_est: f64,
    /// Non-graph MCP calls (policy, status, …) that succeeded.
    pub policy_calls: i64,
    /// Successful calls ÷ all calls (0–100).
    pub success_rate_pct: i64,
    /// Mean response latency when duration was recorded.
    pub avg_duration_ms: i64,
    /// Distinct projects with at least one MCP call.
    pub projects_active: i64,
    /// Tokens lost to per-call clamping vs aggregate net (`tokens_saved_est − net`).
    pub clamp_tokens_absorbed: i64,
    /// Graph calls where per-call savings were > 0.
    pub graph_calls_with_savings: i64,
    /// Pricing in effect (reference model, rates, config source).
    pub pricing: PricingInfo,
    /// Estimation constants in effect when this summary was computed.
    pub assumptions: SavingsAssumptions,
    pub by_tool: Vec<ToolSavingsRow>,
    pub by_project: Vec<ProjectSavingsRow>,
    pub by_model: Vec<ModelSavingsRow>,
    pub by_weekday: Vec<WeekdaySavingsRow>,
    pub by_hour: Vec<HourSavingsRow>,
    pub timeline: Vec<TimelineBucket>,
    pub daily: Vec<DailySavings>,
    pub recent_calls: Vec<RecentCallRow>,
    pub agent_sessions: Vec<AgentSessionRow>,
    pub db_path: String,
}

fn session_model_label(model: &Option<String>) -> String {
    model
        .as_deref()
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

fn session_tuple_to_row(
    (
        agent,
        session_id,
        read_calls,
        grep_calls,
        ax_calls,
        session_input_tokens,
        session_output_tokens,
        model,
        started_at,
        ended_at,
        mcp_calls_in_window,
        tokens_saved_in_window,
    ): (
        String,
        String,
        i64,
        i64,
        i64,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        i64,
        i64,
    ),
) -> AgentSessionRow {
    let session_cost_usd_est = session_input_tokens.map(|tokens| {
        let pricing = model
            .as_deref()
            .map(price_for_model)
            .unwrap_or_else(reference_pricing);
        input_cost_usd(tokens, pricing)
    });
    AgentSessionRow {
        agent,
        session_id,
        read_calls,
        grep_calls,
        ax_calls,
        session_input_tokens,
        session_output_tokens,
        model,
        session_cost_usd_est,
        mcp_calls_in_window,
        tokens_saved_in_window,
        started_at,
        ended_at,
    }
}

fn aggregate_sessions_by_model(sessions: &[AgentSessionRow]) -> Vec<ModelSavingsRow> {
    let mut by_model: HashMap<String, ModelSavingsRow> = HashMap::new();
    for s in sessions {
        let model = session_model_label(&s.model);
        let (pricing, pricing_source) = s
            .model
            .as_deref()
            .map(price_for_model_with_source)
            .unwrap_or_else(reference_pricing_with_source);
        let session_cost = s.session_cost_usd_est.unwrap_or(0.0);
        let cost_saved = input_cost_usd(s.tokens_saved_in_window, pricing);
        let row = by_model.entry(model.clone()).or_insert_with(|| ModelSavingsRow {
            model,
            sessions: 0,
            session_input_tokens: 0,
            tokens_saved_est: 0,
            ax_calls: 0,
            read_calls: 0,
            grep_calls: 0,
            session_cost_usd_est: 0.0,
            cost_saved_usd_est: 0.0,
            input_per_mtok: pricing.input_per_mtok,
            pricing_source: pricing_source.clone(),
        });
        row.sessions += 1;
        row.session_input_tokens += s.session_input_tokens.unwrap_or(0);
        row.tokens_saved_est += s.tokens_saved_in_window;
        row.ax_calls += s.ax_calls;
        row.read_calls += s.read_calls;
        row.grep_calls += s.grep_calls;
        row.session_cost_usd_est += session_cost;
        row.cost_saved_usd_est += cost_saved;
    }
    let mut rows: Vec<ModelSavingsRow> = by_model.into_values().collect();
    rows.sort_by(|a, b| {
        b.tokens_saved_est
            .cmp(&a.tokens_saved_est)
            .then_with(|| b.session_input_tokens.cmp(&a.session_input_tokens))
            .then_with(|| a.model.cmp(&b.model))
    });
    rows
}

pub async fn query_savings_summary(q: &SavingsQuery) -> Result<SavingsSummary, String> {
    let range = resolve_period(q.period, q.from.as_deref(), q.to.as_deref())?;
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let _ = refresh_price_cache_from_db().await;

    let totals: (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, f64, i64) = sqlx::query_as(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ok = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN tokens_saved_est ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_files ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_tokens_est ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN response_tokens_est ELSE 0 END), 0),
                COALESCE(SUM(response_tokens_est), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_exact_files ELSE 0 END), 0),
                COUNT(DISTINCT CASE WHEN project IS NOT NULL AND project != '' THEN project END),
                COALESCE(AVG(CASE WHEN duration_ms IS NOT NULL THEN duration_ms END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 AND tokens_saved_est > 0 THEN 1 ELSE 0 END), 0)
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_one(&pool)
    .await
    .map_err(|e| e.to_string())?;

    type ToolTuple = (String, i64, i64, i64, i64, i64, i64, i64, f64);
    let by_tool: Vec<ToolSavingsRow> = sqlx::query_as::<_, ToolTuple>(
        "SELECT tool, COUNT(*) as calls,
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ok = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN tokens_saved_est ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_files ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_tokens_est ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN response_tokens_est ELSE 0 END), 0),
                COALESCE(AVG(CASE WHEN duration_ms IS NOT NULL THEN duration_ms END), 0)
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?
         GROUP BY tool ORDER BY 5 DESC, 2 DESC",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(
        |(
            tool,
            calls,
            graph_calls,
            failed_calls,
            saved,
            files,
            cf_tokens,
            resp_tokens,
            avg_duration,
        )| ToolSavingsRow {
            tool,
            calls,
            graph_calls,
            failed_calls,
            tokens_saved_est: saved,
            counterfactual_files: files,
            counterfactual_tokens_est: cf_tokens,
            graph_response_tokens_est: resp_tokens,
            avg_duration_ms: avg_duration.round() as i64,
        },
    )
    .collect();

    type DailyTuple = (String, i64, i64, i64, i64, i64, i64, i64);
    let daily_raw: Vec<DailyTuple> = sqlx::query_as(
        "SELECT date(created_at / 1000, 'unixepoch', 'localtime') as d,
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN tokens_saved_est ELSE 0 END), 0),
                COUNT(*),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ok = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_files ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_tokens_est ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN response_tokens_est ELSE 0 END), 0)
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?
         GROUP BY d ORDER BY d",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut daily: Vec<DailySavings> = Vec::with_capacity(daily_raw.len());
    for (
        date,
        tokens_saved_est,
        calls,
        graph_calls,
        failed_calls,
        counterfactual_files,
        counterfactual_tokens_est,
        graph_response_tokens_est,
    ) in daily_raw
    {
        let (pricing, _) = price_as_of(None, &date).await;
        daily.push(DailySavings {
            date,
            tokens_saved_est,
            calls,
            graph_calls,
            failed_calls,
            counterfactual_files,
            counterfactual_tokens_est,
            graph_response_tokens_est,
            cost_saved_usd_est: input_cost_usd(tokens_saved_est, pricing),
        });
    }

    type ProjectTuple = (String, i64, i64, i64, i64);
    let by_project: Vec<ProjectSavingsRow> = sqlx::query_as::<_, ProjectTuple>(
        "SELECT COALESCE(NULLIF(project, ''), '(no project)') as p,
                COUNT(*),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN tokens_saved_est ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_files ELSE 0 END), 0)
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?
         GROUP BY p ORDER BY 4 DESC, 2 DESC LIMIT 25",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|(project, calls, graph_calls, tokens_saved_est, counterfactual_files)| ProjectSavingsRow {
        project,
        calls,
        graph_calls,
        tokens_saved_est,
        counterfactual_files,
    })
    .collect();

    type RecentTuple = (
        i64,
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
        i64,
        Option<i64>,
        i64,
        i64,
        Option<i64>,
        i64,
        Option<String>,
        Option<String>,
    );
    let recent_calls: Vec<RecentCallRow> = sqlx::query_as::<_, RecentTuple>(
        "SELECT id, tool, project, tokens_saved_est, counterfactual_tokens_est, response_tokens_est,
                counterfactual_files, ok, savings_eligible, duration_ms, created_at,
                response_preview, counterfactual_preview
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?
         ORDER BY created_at DESC LIMIT 40",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(
        |(
            id,
            tool,
            project,
            tokens_saved_est,
            counterfactual_tokens_est,
            response_tokens_est,
            counterfactual_files,
            ok,
            savings_eligible,
            duration_ms,
            created_at,
            response_preview,
            counterfactual_preview,
        )| {
            let has_preview = response_preview
                .as_ref()
                .is_some_and(|s| !s.is_empty())
                || counterfactual_preview.as_ref().is_some_and(|s| !s.is_empty());
            RecentCallRow {
                id,
                tool,
                project,
                tokens_saved_est: tokens_saved_est.unwrap_or(0),
                counterfactual_tokens_est: counterfactual_tokens_est.unwrap_or(0),
                response_tokens_est,
                counterfactual_files: counterfactual_files.unwrap_or(0),
                ok: ok != 0,
                savings_eligible: savings_eligible != 0,
                duration_ms,
                created_at,
                has_preview,
            }
        },
    )
    .collect();

    const WEEKDAY_LABELS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    type WeekdayTuple = (i64, i64, i64, i64);
    let weekday_raw: Vec<WeekdayTuple> = sqlx::query_as(
        "SELECT CAST(strftime('%w', created_at / 1000, 'unixepoch', 'localtime') AS INTEGER) as wd,
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN tokens_saved_est ELSE 0 END), 0),
                COUNT(*),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN 1 ELSE 0 END), 0)
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?
         GROUP BY wd ORDER BY wd",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let by_weekday: Vec<WeekdaySavingsRow> = weekday_raw
        .into_iter()
        .map(|(weekday, tokens_saved_est, calls, graph_calls)| WeekdaySavingsRow {
            label: WEEKDAY_LABELS
                .get(weekday as usize)
                .copied()
                .unwrap_or("?")
                .to_string(),
            weekday,
            tokens_saved_est,
            calls,
            graph_calls,
        })
        .collect();

    type HourTuple = (i64, i64, i64, i64);
    let hour_raw: Vec<HourTuple> = sqlx::query_as(
        "SELECT CAST(strftime('%H', created_at / 1000, 'unixepoch', 'localtime') AS INTEGER) as hr,
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN tokens_saved_est ELSE 0 END), 0),
                COUNT(*),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN 1 ELSE 0 END), 0)
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?
         GROUP BY hr ORDER BY hr",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut hour_map: std::collections::HashMap<i64, (i64, i64, i64)> = std::collections::HashMap::new();
    for (hour, tokens_saved_est, calls, graph_calls) in hour_raw {
        hour_map.insert(hour, (tokens_saved_est, calls, graph_calls));
    }
    let by_hour: Vec<HourSavingsRow> = (0..24)
        .map(|hour| {
            let (tokens_saved_est, calls, graph_calls) = hour_map.get(&hour).copied().unwrap_or((0, 0, 0));
            HourSavingsRow {
                hour,
                label: format!("{hour:02}"),
                tokens_saved_est,
                calls,
                graph_calls,
            }
        })
        .collect();

    type TimelineTuple = (String, i64, i64, i64);
    let timeline: Vec<TimelineBucket> = sqlx::query_as::<_, TimelineTuple>(
        "SELECT strftime('%Y-%m-%d %H:00', created_at / 1000, 'unixepoch', 'localtime') as bucket,
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN tokens_saved_est ELSE 0 END), 0),
                COUNT(*),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN 1 ELSE 0 END), 0)
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?
         GROUP BY bucket ORDER BY bucket",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|(bucket, tokens_saved_est, calls, graph_calls)| TimelineBucket {
        bucket,
        tokens_saved_est,
        calls,
        graph_calls,
    })
    .collect();

    type SessionTuple = (
        String,
        String,
        i64,
        i64,
        i64,
        Option<i64>,
        Option<i64>,
        Option<String>,
        Option<i64>,
        Option<i64>,
        i64,
        i64,
    );
    const SESSIONS_SQL: &str = "SELECT s.agent, s.session_id, s.read_calls, s.grep_calls, s.ax_calls,
                s.session_input_tokens, s.session_output_tokens, s.model,
                s.started_at, s.ended_at,
                (SELECT COUNT(*) FROM mcp_call_log m
                  WHERE s.started_at IS NOT NULL AND s.ended_at IS NOT NULL
                    AND m.created_at BETWEEN s.started_at AND s.ended_at),
                (SELECT COALESCE(SUM(m.tokens_saved_est), 0) FROM mcp_call_log m
                  WHERE s.started_at IS NOT NULL AND s.ended_at IS NOT NULL
                    AND m.savings_eligible = 1
                    AND m.created_at BETWEEN s.started_at AND s.ended_at)
         FROM agent_session_log s
         WHERE COALESCE(s.started_at, s.source_mtime) >= ? AND COALESCE(s.started_at, s.source_mtime) <= ?
         ORDER BY COALESCE(s.started_at, s.source_mtime) DESC";

    let session_rows: Vec<AgentSessionRow> = sqlx::query_as::<_, SessionTuple>(SESSIONS_SQL)
        .bind(range.from_ms)
        .bind(range.to_ms)
        .fetch_all(&pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(session_tuple_to_row)
        .collect();

    let by_model = aggregate_sessions_by_model(&session_rows);
    let agent_sessions: Vec<AgentSessionRow> = session_rows.into_iter().take(50).collect();

    let (
        mcp_calls,
        graph_calls,
        failed_calls,
        tokens_saved_est,
        counterfactual_files,
        counterfactual_tokens_est,
        graph_response_tokens_est,
        response_tokens_est,
        counterfactual_exact_files,
        projects_active,
        avg_duration_raw,
        graph_calls_with_savings,
    ) = totals;

    let net_tokens_saved_est = counterfactual_tokens_est - graph_response_tokens_est;
    let clamp_tokens_absorbed = tokens_saved_est - net_tokens_saved_est;
    let policy_calls = (mcp_calls - graph_calls).max(0);
    let success_rate_pct = if mcp_calls > 0 {
        ((mcp_calls - failed_calls) * 100 / mcp_calls).max(0)
    } else {
        0
    };

    let reference = reference_pricing();
    Ok(SavingsSummary {
        period: range.period,
        from: range.from_date,
        to: range.to_date,
        from_ms: range.from_ms,
        to_ms: range.to_ms,
        mcp_calls,
        graph_calls,
        failed_calls,
        tokens_saved_est,
        net_tokens_saved_est,
        counterfactual_files,
        counterfactual_tokens_est,
        graph_response_tokens_est,
        response_tokens_est,
        counterfactual_exact_files,
        cost_saved_usd_est: input_cost_usd(tokens_saved_est, reference),
        graph_response_cost_usd_est: input_cost_usd(graph_response_tokens_est, reference),
        counterfactual_cost_usd_est: input_cost_usd(counterfactual_tokens_est, reference),
        policy_calls,
        success_rate_pct,
        avg_duration_ms: avg_duration_raw.round() as i64,
        projects_active,
        clamp_tokens_absorbed,
        graph_calls_with_savings,
        pricing: pricing_info(),
        assumptions: current_assumptions(),
        by_tool,
        by_project,
        by_model,
        by_weekday,
        by_hour,
        timeline,
        daily,
        recent_calls,
        agent_sessions,
        db_path: usage_db_path().display().to_string(),
    })
}

/// Load one MCP call with tokenized preview chips for the Savings UI.
pub async fn query_call_token_detail(id: i64) -> Result<CallTokenDetail, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    type DetailTuple = (
        i64,
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
        i64,
        Option<i64>,
        i64,
        i64,
        Option<i64>,
        i64,
        Option<String>,
        Option<String>,
    );
    let row = sqlx::query_as::<_, DetailTuple>(
        "SELECT id, tool, project, tokens_saved_est, counterfactual_tokens_est, response_tokens_est,
                counterfactual_files, ok, savings_eligible, duration_ms, created_at,
                response_preview, counterfactual_preview
         FROM mcp_call_log WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("call {id} not found"))?;

    let (
        id,
        tool,
        project,
        tokens_saved_est,
        counterfactual_tokens_est,
        response_tokens_est,
        counterfactual_files,
        ok,
        savings_eligible,
        duration_ms,
        created_at,
        response_preview,
        counterfactual_preview,
    ) = row;

    let response_tokens = tokenize_text(response_preview.as_deref().unwrap_or(""));
    let counterfactual_tokens = tokenize_text(counterfactual_preview.as_deref().unwrap_or(""));

    Ok(CallTokenDetail {
        id,
        tool,
        project,
        tokens_saved_est: tokens_saved_est.unwrap_or(0),
        counterfactual_tokens_est: counterfactual_tokens_est.unwrap_or(0),
        response_tokens_est,
        counterfactual_files: counterfactual_files.unwrap_or(0),
        ok: ok != 0,
        savings_eligible: savings_eligible != 0,
        duration_ms,
        created_at,
        response_preview,
        counterfactual_preview,
        response_tokens,
        counterfactual_tokens,
    })
}

fn cursor_hook_session_id(input: &Value) -> Option<String> {
    for key in ["session_id", "conversation_id"] {
        if let Some(id) = input.get(key).and_then(|v| v.as_str()) {
            let id = id.trim();
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
    }
    None
}

/// Extract Cursor `session_id` from a sessionStart hook payload (model optional).
pub fn parse_cursor_hook_session_id(input: &Value) -> Option<String> {
    let event = input
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if event != "sessionStart" {
        return None;
    }
    cursor_hook_session_id(input)
}

fn cursor_hook_model_params(input: &Value) -> Vec<(String, String)> {
    input
        .get("model_params")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(|v| v.as_str())?.to_string();
                    let value = item
                        .get("value")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some((id, value))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse a Cursor hook stdin payload into `(session_id, model)` for sessionStart.
pub fn parse_cursor_hook_model(input: &Value) -> Option<(String, String)> {
    let event = input
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if event != "sessionStart" {
        return None;
    }
    let session_id = cursor_hook_session_id(input)?;
    let model_id = input
        .get("model_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .or_else(|| input.get("model").and_then(|v| v.as_str()))
        .unwrap_or("")
        .trim()
        .to_string();
    if model_id.is_empty() {
        return None;
    }
    let model = normalize_cursor_model(&model_id, &cursor_hook_model_params(input));
    if model.is_empty() {
        return None;
    }
    Some((session_id, model))
}

/// Persist a model tag from a Cursor sessionStart hook without touching tool-call counts.
pub async fn record_session_model_tag(
    agent: &str,
    session_id: &str,
    model: &str,
) -> Result<(), String> {
    let model = model.trim();
    let session_id = session_id.trim();
    if model.is_empty() || session_id.is_empty() {
        return Err("session_id and model are required".into());
    }
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO agent_session_log
         (agent, session_id, read_calls, grep_calls, ax_calls, source_mtime, model)
         VALUES (?, ?, 0, 0, 0, 0, ?)
         ON CONFLICT(agent, session_id) DO UPDATE SET model = excluded.model",
    )
    .bind(agent)
    .bind(session_id)
    .bind(model)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub claude_sessions: usize,
    pub cursor_sessions: usize,
    pub cursor_state_enriched: usize,
    pub skipped: usize,
}

#[derive(Default)]
struct SessionAccum {
    read_calls: i64,
    grep_calls: i64,
    ax_calls: i64,
    session_input_tokens: i64,
    session_output_tokens: i64,
    model: Option<String>,
    started_at: Option<i64>,
    ended_at: Option<i64>,
}

fn parse_iso_ms(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

fn file_mtime_ms(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

async fn upsert_agent_session(
    agent: &str,
    session_id: &str,
    acc: &SessionAccum,
    source_mtime: i64,
) -> Result<bool, String> {
    let pool = open_pool().await.map_err(|e| e.to_string())?;
    let input = if acc.session_input_tokens > 0 {
        Some(acc.session_input_tokens)
    } else {
        None
    };
    let output = if acc.session_output_tokens > 0 {
        Some(acc.session_output_tokens)
    } else {
        None
    };
    let result = sqlx::query(
        "INSERT INTO agent_session_log
         (agent, session_id, read_calls, grep_calls, ax_calls, session_input_tokens,
          session_output_tokens, model, source_mtime, started_at, ended_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(agent, session_id) DO UPDATE SET
           read_calls = excluded.read_calls,
           grep_calls = excluded.grep_calls,
           ax_calls = excluded.ax_calls,
           session_input_tokens = COALESCE(excluded.session_input_tokens, agent_session_log.session_input_tokens),
           session_output_tokens = COALESCE(excluded.session_output_tokens, agent_session_log.session_output_tokens),
           model = COALESCE(excluded.model, agent_session_log.model),
           source_mtime = excluded.source_mtime,
           started_at = COALESCE(excluded.started_at, agent_session_log.started_at),
           ended_at = COALESCE(excluded.ended_at, agent_session_log.ended_at)
         WHERE excluded.source_mtime >= agent_session_log.source_mtime",
    )
    .bind(agent)
    .bind(session_id)
    .bind(acc.read_calls)
    .bind(acc.grep_calls)
    .bind(acc.ax_calls)
    .bind(input)
    .bind(output)
    .bind(&acc.model)
    .bind(source_mtime)
    .bind(acc.started_at)
    .bind(acc.ended_at)
    .execute(&pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(result.rows_affected() > 0)
}

fn classify_claude_tool(name: &str, acc: &mut SessionAccum) {
    if name.starts_with("mcp__ax__") || name.starts_with("ax_") {
        acc.ax_calls += 1;
    } else if name == "Read" {
        acc.read_calls += 1;
    } else if name == "Grep" {
        acc.grep_calls += 1;
    }
}

fn classify_cursor_tool(name: &str, input: &Value, acc: &mut SessionAccum) {
    if name == "Read" {
        acc.read_calls += 1;
    } else if name == "Grep" {
        acc.grep_calls += 1;
    } else if name == "CallMcpTool" {
        let server = input.get("server").and_then(|v| v.as_str()).unwrap_or("");
        let tool = input
            .get("toolName")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if server.contains("ax") || tool.starts_with("ax_") {
            acc.ax_calls += 1;
        }
    }
}

fn add_usage_from_message(msg: &Value, acc: &mut SessionAccum) {
    if acc.model.is_none() {
        if let Some(model) = msg.get("model").and_then(|m| m.as_str()) {
            if !model.is_empty() {
                acc.model = Some(model.to_string());
            }
        }
    }
    let Some(usage) = msg.get("usage") else {
        return;
    };
    let mut input = usage
        .get("input_tokens")
        .or_else(|| usage.get("prompt_tokens"))
        .and_then(json_i64)
        .unwrap_or(0);
    for key in ["cache_read_input_tokens", "cache_creation_input_tokens"] {
        if let Some(n) = usage.get(key).and_then(json_i64) {
            input += n;
        }
    }
    acc.session_input_tokens += input;
    if let Some(output) = usage
        .get("output_tokens")
        .or_else(|| usage.get("completion_tokens"))
        .and_then(json_i64)
    {
        acc.session_output_tokens += output;
    }
}

fn process_claude_line(line: &str, acc: &mut SessionAccum) {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()).and_then(parse_iso_ms) {
        acc.started_at = Some(acc.started_at.map_or(ts, |s| s.min(ts)));
        acc.ended_at = Some(acc.ended_at.map_or(ts, |e| e.max(ts)));
    }
    let msg = v.get("message").unwrap_or(&v);
    if let Some(role) = msg.get("role").and_then(|r| r.as_str()) {
        if role == "assistant" {
            add_usage_from_message(msg, acc);
        }
    }
    add_usage_from_message(&v, acc);
    let content = msg
        .get("content")
        .and_then(|c| c.as_array())
        .or_else(|| v.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()));
    if let Some(items) = content {
        for item in items {
            if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    classify_claude_tool(name, acc);
                }
            }
        }
    }
}

fn process_cursor_line(line: &str, acc: &mut SessionAccum) {
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if let Some(ts) = v
        .get("timestamp")
        .and_then(|t| t.as_str())
        .and_then(parse_iso_ms)
    {
        acc.started_at = Some(acc.started_at.map_or(ts, |s| s.min(ts)));
        acc.ended_at = Some(acc.ended_at.map_or(ts, |e| e.max(ts)));
    }
    if v.get("role").and_then(|r| r.as_str()) != Some("assistant") {
        return;
    }
    let Some(msg) = v.get("message") else {
        return;
    };
    // Cursor transcripts usually omit usage; capture it (and the model) when present.
    add_usage_from_message(msg, acc);
    let Some(items) = msg.get("content").and_then(|c| c.as_array()) else {
        return;
    };
    for item in items {
        if item.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
            let name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let input = item.get("input").cloned().unwrap_or(Value::Null);
            classify_cursor_tool(name, &input, acc);
        }
    }
}

async fn import_claude_file(path: &Path) -> Result<bool, String> {
    let session_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mtime = file_mtime_ms(path);
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut acc = SessionAccum::default();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        process_claude_line(line, &mut acc);
    }
    if acc.read_calls == 0 && acc.grep_calls == 0 && acc.ax_calls == 0 && acc.session_input_tokens == 0
    {
        return Ok(false);
    }
    upsert_agent_session("claude", &session_id, &acc, mtime).await
}

async fn import_cursor_file(path: &Path) -> Result<bool, String> {
    let session_id = path
        .parent()
        .and_then(|p| p.file_name())
        .or_else(|| path.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();
    let mtime = file_mtime_ms(path);
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut acc = SessionAccum::default();
    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        process_cursor_line(line, &mut acc);
    }
    if acc.read_calls == 0 && acc.grep_calls == 0 && acc.ax_calls == 0 {
        return Ok(false);
    }
    upsert_agent_session("cursor", &session_id, &acc, mtime).await
}

fn claude_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

fn cursor_projects_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".cursor").join("projects"))
}

/// Cursor stores one transcript per chat: `agent-transcripts/{id}/{id}.jsonl`.
fn cursor_transcript_matches(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    if !path_str.contains("agent-transcripts") || path_str.contains("subagents") {
        return false;
    }
    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
        return false;
    }
    let parent_name = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str());
    let file_stem = path.file_stem().and_then(|s| s.to_str());
    parent_name.is_some() && parent_name == file_stem
}

pub async fn import_agent_logs(claude: bool, cursor: bool) -> Result<ImportResult, String> {
    let mut claude_sessions = 0usize;
    let mut cursor_sessions = 0usize;
    let mut skipped = 0usize;

    if claude {
        if let Some(root) = claude_projects_dir() {
            if root.is_dir() {
                for entry in WalkDir::new(&root)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                        continue;
                    }
                    if path.to_string_lossy().contains("subagents") {
                        continue;
                    }
                    match import_claude_file(path).await {
                        Ok(true) => claude_sessions += 1,
                        Ok(false) => skipped += 1,
                        Err(e) => eprintln!("ax savings import: {}: {e}", path.display()),
                    }
                }
            }
        }
    }

    if cursor {
        if let Some(root) = cursor_projects_dir() {
            if root.is_dir() {
                for entry in WalkDir::new(&root)
                    .into_iter()
                    .filter_map(|e| e.ok())
                    .filter(|e| e.file_type().is_file())
                {
                    let path = entry.path();
                    if !cursor_transcript_matches(path) {
                        continue;
                    }
                    match import_cursor_file(path).await {
                        Ok(true) => cursor_sessions += 1,
                        Ok(false) => skipped += 1,
                        Err(e) => eprintln!("ax savings import: {}: {e}", path.display()),
                    }
                }
            }
        }

        let (state_enriched, state_skipped) = import_cursor_composer_state().await.unwrap_or((0, 0));
        skipped += state_skipped;
        return Ok(ImportResult {
            claude_sessions,
            cursor_sessions,
            cursor_state_enriched: state_enriched,
            skipped,
        });
    }

    Ok(ImportResult {
        claude_sessions,
        cursor_sessions,
        cursor_state_enriched: 0,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::count_file_tokens;
    use serde_json::json;
    use std::sync::Mutex;

    static DB_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn long_response(chars: usize) -> String {
        "response text ".repeat(chars / 14 + 1)
    }

    #[test]
    fn estimate_from_explore_result() {
        let structured = json!({
            "entries": [{
                "node": { "filePath": "src/a.rs", "endLine": 100 },
                "callers": [{ "filePath": "src/b.rs", "endLine": 50 }],
                "callees": []
            }]
        });
        let est = estimate_savings("ax_explore", &structured, &long_response(800), None);
        assert_eq!(est.counterfactual_files, 2);
        assert!(est.counterfactual_tokens_est > est.response_tokens_est);
        assert!(est.tokens_saved_est > 0);
    }

    #[test]
    fn saved_tokens_clamped_at_zero_per_call() {
        // Response larger than the counterfactual (small file, verbose response):
        // savings clamp to 0 instead of going negative.
        let structured = json!({ "node": { "filePath": "src/tiny.rs", "endLine": 2 } });
        let est = estimate_savings("ax_node", &structured, &long_response(100_000), None);
        assert!(est.response_tokens_est > est.counterfactual_tokens_est);
        assert_eq!(est.tokens_saved_est, 0);
    }

    #[test]
    fn no_file_refs_means_zero_savings() {
        // Conservative: a graph response without file references claims no savings.
        let est = estimate_savings("ax_search", &json!({ "matches": [] }), &long_response(400), None);
        assert!(est.savings_eligible);
        assert_eq!(est.counterfactual_files, 0);
        assert_eq!(est.counterfactual_tokens_est, 0);
        assert_eq!(est.tokens_saved_est, 0);
    }

    #[test]
    fn same_file_counted_once_per_call() {
        // Two hits in the same file — counterfactual uses the max end line, once.
        let structured = json!({
            "entries": [
                { "filePath": "src/a.rs", "endLine": 40 },
                { "filePath": "src/a.rs", "endLine": 120 }
            ]
        });
        let est = estimate_savings("ax_explore", &structured, "", None);
        assert_eq!(est.counterfactual_files, 1);
        assert_eq!(est.counterfactual_exact_files, 0);
        assert_eq!(est.counterfactual_tokens_est, 120 * tokens_per_line());
    }

    #[test]
    fn readable_file_counterfactual_is_measured_exactly() {
        let dir = std::env::temp_dir().join("ax-usage-savings-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("measured.rs");
        std::fs::write(&file, "fn alpha() {}\nfn beta() {}\nfn gamma() {}\n").unwrap();

        let structured = json!({ "node": { "filePath": "measured.rs", "endLine": 3 } });
        let est = estimate_savings("ax_node", &structured, "", Some(&dir));
        assert_eq!(est.counterfactual_files, 1);
        assert_eq!(est.counterfactual_exact_files, 1);
        // Exact measurement, not 3 lines x 9 tokens.
        assert_ne!(est.counterfactual_tokens_est, 3 * tokens_per_line());
        assert!(est.counterfactual_tokens_est > 0);

        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn path_only_ref_still_counts_file() {
        let structured = json!({ "node": { "filePath": "src/a.rs" } });
        let est = estimate_savings("ax_node", &structured, "", None);
        assert_eq!(est.counterfactual_files, 1);
        assert!(est.counterfactual_tokens_est > 0);
    }

    #[test]
    fn related_files_array_increments_counterfactual() {
        let structured = json!({
            "relatedFiles": ["src/a.rs", "src/b.rs"],
            "codeBlocks": [{
                "filePath": "src/c.rs",
                "startLine": 1,
                "endLine": 20,
                "content": "fn main() {}"
            }]
        });
        let est = estimate_savings("ax_context", &structured, &long_response(200), None);
        assert_eq!(est.counterfactual_files, 3);
        assert!(est.tokens_saved_est >= 0);
    }

    #[test]
    fn range_mode_uses_line_span_when_set() {
        let dir = std::env::temp_dir().join("ax-usage-savings-range-mode");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("span.rs");
        std::fs::write(&file, (1..=50).map(|i| format!("fn f{i}() {{}}")).collect::<Vec<_>>().join("\n")).unwrap();

        std::env::set_var("AX_SAVINGS_CF_MODE", "range");
        let structured = json!({ "node": { "filePath": "span.rs", "startLine": 2, "endLine": 4 } });
        let est = estimate_savings("ax_node", &structured, "", Some(&dir));
        let full_only = count_file_tokens(&file).unwrap();
        assert!(est.counterfactual_tokens_est < full_only);
        std::env::remove_var("AX_SAVINGS_CF_MODE");

        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn policy_tool_not_eligible() {
        let est = estimate_savings("ax_preflight", &json!({}), &long_response(5000), None);
        assert!(!est.savings_eligible);
        assert_eq!(est.tokens_saved_est, 0);
    }

    #[test]
    fn claude_tool_classification() {
        let mut acc = SessionAccum::default();
        classify_claude_tool("Read", &mut acc);
        classify_claude_tool("mcp__ax__ax_explore", &mut acc);
        assert_eq!(acc.read_calls, 1);
        assert_eq!(acc.ax_calls, 1);
    }

    #[test]
    fn cursor_transcript_path_filter() {
        let ok = PathBuf::from(
            r"C:\Users\me\.cursor\projects\p\agent-transcripts\uuid\uuid.jsonl",
        );
        assert!(cursor_transcript_matches(&ok));
        let bad_ext = PathBuf::from(
            r"C:\Users\me\.cursor\projects\p\agent-transcripts\uuid\uuid.txt",
        );
        assert!(!cursor_transcript_matches(&bad_ext));
        let bad_name = PathBuf::from(
            r"C:\Users\me\.cursor\projects\p\agent-transcripts\uuid\other.jsonl",
        );
        assert!(!cursor_transcript_matches(&bad_name));
    }

    #[test]
    fn normalize_cursor_model_composer_fast() {
        let params = vec![("fast".to_string(), "true".to_string())];
        assert_eq!(
            normalize_cursor_model("composer-2.5", &params),
            "composer-2.5-fast"
        );
    }

    #[test]
    fn parse_cursor_hook_payload() {
        let input = json!({
            "hook_event_name": "sessionStart",
            "session_id": "218bb987-86eb-45f0-a8e7-eedae17f995c",
            "model_id": "composer-2.5",
            "model_params": [{ "id": "fast", "value": "true" }]
        });
        let (id, model) = parse_cursor_hook_model(&input).expect("parsed");
        assert_eq!(id, "218bb987-86eb-45f0-a8e7-eedae17f995c");
        assert_eq!(model, "composer-2.5-fast");
    }

    #[test]
    fn parse_cursor_hook_session_without_model() {
        let input = json!({
            "hook_event_name": "sessionStart",
            "session_id": "9805fda6-d881-438e-9221-88a0342bdf7a"
        });
        assert_eq!(
            parse_cursor_hook_session_id(&input).as_deref(),
            Some("9805fda6-d881-438e-9221-88a0342bdf7a")
        );
        assert!(
            parse_cursor_hook_model(&input).is_none(),
            "model-less payload should not parse as model tag"
        );
    }

    fn temp_usage_db_path(label: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("ax-usage-{label}-{n}.db"))
    }

    async fn session_counts(session_id: &str) -> (Option<String>, i64, i64, i64) {
        let pool = open_pool().await.expect("pool");
        sqlx::query_as::<_, (Option<String>, i64, i64, i64)>(
            "SELECT model, read_calls, grep_calls, ax_calls
             FROM agent_session_log WHERE agent = 'cursor' AND session_id = ?",
        )
        .bind(session_id)
        .fetch_one(&pool)
        .await
        .expect("row")
    }

    #[tokio::test]
    async fn tag_session_model_merge() {
        let _guard = DB_TEST_LOCK.lock().unwrap();
        let db = temp_usage_db_path("merge");
        let _ = std::fs::remove_file(&db);
        std::env::set_var("AX_USAGE_DB", &db);

        record_session_model_tag("cursor", "sess-1", "composer-2.5-fast")
            .await
            .expect("tag");
        let acc = SessionAccum {
            read_calls: 3,
            grep_calls: 1,
            ax_calls: 2,
            ..SessionAccum::default()
        };
        upsert_agent_session("cursor", "sess-1", &acc, 1000)
            .await
            .expect("import");
        let (model, read, grep, ax) = session_counts("sess-1").await;
        assert_eq!(model.as_deref(), Some("composer-2.5-fast"));
        assert_eq!((read, grep, ax), (3, 1, 2));

        let acc2 = SessionAccum {
            read_calls: 7,
            grep_calls: 4,
            ax_calls: 3,
            ..SessionAccum::default()
        };
        upsert_agent_session("cursor", "sess-2", &acc2, 2000)
            .await
            .expect("import");
        record_session_model_tag("cursor", "sess-2", "composer-2.5-fast")
            .await
            .expect("tag");
        let (model2, read2, grep2, ax2) = session_counts("sess-2").await;
        assert_eq!(model2.as_deref(), Some("composer-2.5-fast"));
        assert_eq!((read2, grep2, ax2), (7, 4, 3));

        std::env::remove_var("AX_USAGE_DB");
        let _ = std::fs::remove_file(db);
    }

    #[tokio::test]
    async fn transcript_import_does_not_wipe_state_tokens() {
        let _guard = DB_TEST_LOCK.lock().unwrap();
        let db = temp_usage_db_path("vscdb-merge");
        let _ = std::fs::remove_file(&db);
        std::env::set_var("AX_USAGE_DB", &db);

        let state_row = crate::cursor_state::ComposerStateRow {
            session_id: "sess-vscdb".to_string(),
            model: Some("composer-2.5-fast".to_string()),
            input_tokens: Some(126_877),
            started_at: Some(1_000),
            ended_at: Some(2_000),
        };
        crate::cursor_state::upsert_composer_state_row(&state_row)
            .await
            .expect("state");

        let acc = SessionAccum {
            read_calls: 5,
            grep_calls: 2,
            ax_calls: 1,
            ..SessionAccum::default()
        };
        upsert_agent_session("cursor", "sess-vscdb", &acc, 5000)
            .await
            .expect("transcript");

        let pool = open_pool().await.expect("pool");
        let (model, input, read): (Option<String>, Option<i64>, i64) = sqlx::query_as(
            "SELECT model, session_input_tokens, read_calls
             FROM agent_session_log WHERE agent = 'cursor' AND session_id = 'sess-vscdb'",
        )
        .fetch_one(&pool)
        .await
        .expect("row");

        assert_eq!(model.as_deref(), Some("composer-2.5-fast"));
        assert_eq!(input, Some(126_877));
        assert_eq!(read, 5);

        std::env::remove_var("AX_USAGE_DB");
        let _ = std::fs::remove_file(db);
    }
}
