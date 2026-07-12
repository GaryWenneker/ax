//! Context-token savings estimation, MCP audit log, and agent session import.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;
use serde_json::Value;
use walkdir::WalkDir;

use crate::period::{resolve_period, UsagePeriod};
use crate::pricing::{input_cost_usd, price_for_model, pricing_info, reference_pricing, PricingInfo};
use crate::store::{open_pool, usage_db_path};
use crate::tokenizer::{count_file_tokens, count_tokens, tokenizer_available};

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

fn end_line_from_obj(map: &serde_json::Map<String, Value>) -> Option<i64> {
    map.get("endLine")
        .or_else(|| map.get("end_line"))
        .and_then(json_i64)
}

fn collect_file_lines(value: &Value, files: &mut HashMap<String, i64>) {
    match value {
        Value::Object(map) => {
            if let Some(fp) = file_path_from_obj(map) {
                if let Some(end) = end_line_from_obj(map) {
                    files
                        .entry(fp.to_string())
                        .and_modify(|m| *m = (*m).max(end))
                        .or_insert(end);
                }
            }
            for v in map.values() {
                collect_file_lines(v, files);
            }
        }
        Value::Array(arr) => {
            for v in arr {
                collect_file_lines(v, files);
            }
        }
        _ => {}
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
/// counterfactual (what a full-file read would have cost) is measured by
/// tokenizing the actual file contents when the file is readable, falling
/// back to `end_line x tokens_per_line`, then to a per-file average.
pub fn estimate_savings(
    tool: &str,
    structured: &Value,
    response_text: &str,
    project_root: Option<&Path>,
) -> SavingsEstimate {
    let response_tokens_est = count_tokens(response_text) as i64;
    let savings_eligible = is_savings_eligible_tool(tool);

    if !savings_eligible {
        return SavingsEstimate {
            response_tokens_est,
            savings_eligible: false,
            ..Default::default()
        };
    }

    let mut files = HashMap::new();
    collect_file_lines(structured, &mut files);

    let tpl = tokens_per_line();
    let fallback = avg_file_tokens();
    let mut counterfactual_tokens_est: i64 = 0;
    let mut counterfactual_exact_files: i64 = 0;
    for (file, max_line) in &files {
        let resolved = resolve_file_path(file, project_root);
        if let Some(exact) = count_file_tokens(&resolved) {
            counterfactual_tokens_est += exact;
            counterfactual_exact_files += 1;
        } else if *max_line > 0 {
            counterfactual_tokens_est += max_line * tpl;
        } else {
            counterfactual_tokens_est += fallback;
        }
    }

    let counterfactual_files = files.len() as i64;
    let tokens_saved_est = (counterfactual_tokens_est - response_tokens_est).max(0);

    SavingsEstimate {
        counterfactual_files,
        counterfactual_exact_files,
        counterfactual_tokens_est,
        response_tokens_est,
        tokens_saved_est,
        savings_eligible: true,
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
}

pub async fn record_mcp_call(record: McpCallRecord) {
    if let Ok(pool) = open_pool().await {
        let now = chrono::Utc::now().timestamp_millis();
        let _ = sqlx::query(
            "INSERT INTO mcp_call_log
             (tool, project, response_chars, response_tokens_est, counterfactual_files,
              counterfactual_exact_files, counterfactual_tokens_est, tokens_saved_est,
              duration_ms, ok, savings_eligible, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
    pub tokens_saved_est: i64,
    pub counterfactual_files: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DailySavings {
    pub date: String,
    pub tokens_saved_est: i64,
    pub calls: i64,
    pub counterfactual_tokens_est: i64,
    pub graph_response_tokens_est: i64,
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
}

pub fn current_assumptions() -> SavingsAssumptions {
    SavingsAssumptions {
        exact_tokenizer: tokenizer_available(),
        chars_per_token: chars_per_token(),
        tokens_per_line: tokens_per_line(),
        avg_file_tokens: avg_file_tokens(),
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
    /// Pricing in effect (reference model, rates, config source).
    pub pricing: PricingInfo,
    /// Estimation constants in effect when this summary was computed.
    pub assumptions: SavingsAssumptions,
    pub by_tool: Vec<ToolSavingsRow>,
    pub daily: Vec<DailySavings>,
    pub agent_sessions: Vec<AgentSessionRow>,
    pub db_path: String,
}

pub async fn query_savings_summary(q: &SavingsQuery) -> Result<SavingsSummary, String> {
    let range = resolve_period(q.period, q.from.as_deref(), q.to.as_deref())?;
    let pool = open_pool().await.map_err(|e| e.to_string())?;

    let totals: (i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN ok = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN tokens_saved_est ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_files ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_tokens_est ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN response_tokens_est ELSE 0 END), 0),
                COALESCE(SUM(response_tokens_est), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_exact_files ELSE 0 END), 0)
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_one(&pool)
    .await
    .map_err(|e| e.to_string())?;

    let by_tool: Vec<ToolSavingsRow> = sqlx::query_as(
        "SELECT tool, COUNT(*) as calls,
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN tokens_saved_est ELSE 0 END), 0) as saved,
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_files ELSE 0 END), 0)
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?
         GROUP BY tool ORDER BY saved DESC, calls DESC",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|(tool, calls, saved, files)| ToolSavingsRow {
        tool,
        calls,
        tokens_saved_est: saved,
        counterfactual_files: files,
    })
    .collect();

    let daily: Vec<DailySavings> = sqlx::query_as(
        "SELECT date(created_at / 1000, 'unixepoch', 'localtime') as d,
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN tokens_saved_est ELSE 0 END), 0),
                COUNT(*),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN counterfactual_tokens_est ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN savings_eligible = 1 THEN response_tokens_est ELSE 0 END), 0)
         FROM mcp_call_log WHERE created_at >= ? AND created_at <= ?
         GROUP BY d ORDER BY d",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(
        |(date, tokens_saved_est, calls, counterfactual_tokens_est, graph_response_tokens_est)| {
            DailySavings {
                date,
                tokens_saved_est,
                calls,
                counterfactual_tokens_est,
                graph_response_tokens_est,
            }
        },
    )
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
    let agent_sessions: Vec<AgentSessionRow> = sqlx::query_as::<_, SessionTuple>(
        "SELECT s.agent, s.session_id, s.read_calls, s.grep_calls, s.ax_calls,
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
         ORDER BY COALESCE(s.started_at, s.source_mtime) DESC LIMIT 50",
    )
    .bind(range.from_ms)
    .bind(range.to_ms)
    .fetch_all(&pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(
        |(
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
        )| {
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
        },
    )
    .collect();

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
    ) = totals;

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
        net_tokens_saved_est: counterfactual_tokens_est - graph_response_tokens_est,
        counterfactual_files,
        counterfactual_tokens_est,
        graph_response_tokens_est,
        response_tokens_est,
        counterfactual_exact_files,
        cost_saved_usd_est: input_cost_usd(tokens_saved_est, reference),
        graph_response_cost_usd_est: input_cost_usd(graph_response_tokens_est, reference),
        counterfactual_cost_usd_est: input_cost_usd(counterfactual_tokens_est, reference),
        pricing: pricing_info(),
        assumptions: current_assumptions(),
        by_tool,
        daily,
        agent_sessions,
        db_path: usage_db_path().display().to_string(),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportResult {
    pub claude_sessions: usize,
    pub cursor_sessions: usize,
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
           session_input_tokens = excluded.session_input_tokens,
           session_output_tokens = excluded.session_output_tokens,
           model = COALESCE(excluded.model, agent_session_log.model),
           source_mtime = excluded.source_mtime,
           started_at = excluded.started_at,
           ended_at = excluded.ended_at
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
                    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                        continue;
                    }
                    if !path.to_string_lossy().contains("agent-transcripts") {
                        continue;
                    }
                    if path.to_string_lossy().contains("subagents") {
                        continue;
                    }
                    let parent_name = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str());
                    let file_name = path.file_name().and_then(|n| n.to_str());
                    if parent_name != file_name {
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
    }

    Ok(ImportResult {
        claude_sessions,
        cursor_sessions,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
