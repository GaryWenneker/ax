//! MCP stdio server loop.

use serde_json::{json, Value};

use std::path::PathBuf;

use ax_context::directory::{find_nearest_ax_root, is_initialized};

use crate::engine::McpEngine;
use crate::liveness_watchdog::install_main_thread_watchdog;
use crate::ppid_watchdog::spawn_ppid_watchdog;
use crate::proxy::attach_or_spawn;
use crate::tools::{server_instructions, ToolHandler};
use crate::transport::{is_notification, StdioTransport, PARSE_ERROR, METHOD_NOT_FOUND};
use crate::verbose::{
    deliver_local, format_mcp_log_notification, push_error, push_inbound, push_internal,
    push_outbound, verbose_enabled, with_trace_buffer,
};
use ax_telemetry::telemetry;
use ax_usage::{estimate_savings, spawn_record_mcp_call, McpCallRecord};

/// Result of one MCP JSON-RPC request, plus optional verbose stderr lines.
pub struct RequestOutcome {
    pub result: Result<Value, String>,
    pub verbose_lines: Vec<String>,
}

impl RequestOutcome {
    fn ok(value: Value) -> Self {
        Self {
            result: Ok(value),
            verbose_lines: Vec::new(),
        }
    }

    fn err(msg: String) -> Self {
        Self {
            result: Err(msg),
            verbose_lines: Vec::new(),
        }
    }
}

/// Resolve indexed project root for MCP: explicit `--path` first, then cwd walk-up.
pub fn resolve_mcp_project_root(explicit: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        if let Some(root) = find_nearest_ax_root(&p) {
            return Some(root);
        }
        if is_initialized(&p) {
            return Some(p);
        }
    }
    let cwd = std::env::current_dir().ok()?;
    find_nearest_ax_root(&cwd)
}

pub async fn run_stdio_server(explicit_root: Option<PathBuf>) -> Result<(), Box<dyn std::error::Error>> {
    let project_root = resolve_mcp_project_root(explicit_root);
    if let Some(ref root) = project_root {
        if attach_or_spawn(root).await.is_ok() {
            return Ok(());
        }
    }

    spawn_ppid_watchdog(|| std::process::exit(0));
    let _liveness = install_main_thread_watchdog();

    let mut engine = match project_root {
        Some(root) => {
            McpEngine::start_background_services(&root);
            McpEngine::with_project_root(root)
        }
        None => McpEngine::new(),
    };
    loop {
        match StdioTransport::read_request() {
            Ok(req) => {
                let outcome =
                    handle_request(&mut engine, &req.method, req.params.unwrap_or(Value::Null))
                        .await;
                if is_notification(&req.id) {
                    continue;
                }
                let id = req.id.clone().unwrap_or(Value::Null);
                match outcome.result {
                    Ok(value) => StdioTransport::send_result(id, value)?,
                    Err(msg) => StdioTransport::send_error(Some(id), METHOD_NOT_FOUND, &msg)?,
                }
                // Embedded stdio path (no daemon proxy): stderr + log file +
                // MCP logging notifications on stdout for Cursor Output.
                deliver_local(&outcome.verbose_lines, engine.project_root().map(|p| p.as_path()));
                for text in &outcome.verbose_lines {
                    let _ = StdioTransport::send_notification_line(&format_mcp_log_notification(
                        text,
                    ));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
            Err(e) => {
                StdioTransport::send_error(None, PARSE_ERROR, &e.to_string())?;
            }
        }
    }
}

fn resolve_request_project_root(engine: &McpEngine) -> Option<PathBuf> {
    engine
        .project_root()
        .cloned()
        .or_else(|| resolve_mcp_project_root(None))
}

pub async fn handle_request(engine: &mut McpEngine, method: &str, params: Value) -> RequestOutcome {
    let project_root = resolve_request_project_root(engine);
    let has_policy = project_root
        .as_ref()
        .map(|p| ax_policy::policy_tools_enabled(p.as_path()))
        .unwrap_or(false);

    match method {
        "initialize" => RequestOutcome::ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "ax", "version": env!("CARGO_PKG_VERSION") },
            "instructions": server_instructions(has_policy),
        })),
        "tools/list" => RequestOutcome::ok(ToolHandler::list_tools(has_policy).await),
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
            let verbose = verbose_enabled(project_root.as_deref());
            if verbose {
                let (result, verbose_lines) = with_trace_buffer(async {
                    call_tool_and_wrap(engine, &name, args, project_root.as_deref(), true).await
                })
                .await;
                RequestOutcome {
                    result,
                    verbose_lines,
                }
            } else {
                let result =
                    call_tool_and_wrap(engine, &name, args, project_root.as_deref(), false).await;
                RequestOutcome {
                    result,
                    verbose_lines: Vec::new(),
                }
            }
        }
        "notifications/initialized" => RequestOutcome::ok(Value::Null),
        _ => RequestOutcome::err(format!("method not found: {}", method)),
    }
}

async fn call_tool_and_wrap(
    engine: &mut McpEngine,
    name: &str,
    args: Value,
    project_root: Option<&std::path::Path>,
    verbose: bool,
) -> Result<Value, String> {
    if is_policy_tool(name) {
        engine.ensure_policy_fresh().await?;
    } else {
        engine.ensure_initialized().await?;
        engine.reopen_if_replaced().await?;
    }
    if verbose {
        push_inbound(name, &args);
    }
    let started = std::time::Instant::now();
    let result = if let Some(pool) = engine.query_pool() {
        if pool.healthy() && crate::query_pool::is_read_tool(name) {
            pool.run(|| async {
                let mut guard = engine.lock_ax().await;
                if let Some(ax) = guard.as_mut() {
                    ToolHandler::call_tool(ax, name, args).await
                } else {
                    Err("ax not initialized".to_string())
                }
            })
            .await
        } else {
            let mut guard = engine.lock_ax().await;
            if let Some(ax) = guard.as_mut() {
                ToolHandler::call_tool(ax, name, args).await
            } else {
                Err("ax not initialized".to_string())
            }
        }
    } else {
        let mut guard = engine.lock_ax().await;
        if let Some(ax) = guard.as_mut() {
            ToolHandler::call_tool(ax, name, args).await
        } else {
            Err("ax not initialized".to_string())
        }
    };
    if let Ok(mut t) = telemetry().lock() {
        t.record_usage("mcp_tool", name, result.is_ok(), None);
        t.persist_sync();
    }
    ax_telemetry::trigger_background_flush();
    let duration_ms = started.elapsed().as_millis() as i64;
    let project = project_root.map(|p| p.display().to_string());
    match &result {
        Ok(value) => {
            if verbose {
                push_internal(name, value);
            }
            let text = tool_result_text(value);
            // Savings measurement always runs against the FULL value so
            // counterfactual file detection stays accurate even though the
            // wire payload below is leaner.
            let est = estimate_savings(name, value, &text, project_root);
            let full = render_full();
            let structured = if full {
                Some(value.clone())
            } else {
                lean_structured(name, value)
            };
            let wrapped = wrap_call_tool_result_parts(
                value,
                structured,
                false,
                token_budget_hint(name, est.response_tokens_est),
            );
            if verbose {
                push_outbound(name, &wrapped, full, duration_ms);
            }
            spawn_record_mcp_call(McpCallRecord {
                tool: name.to_string(),
                project,
                response_chars: text.len() as i64,
                response_tokens_est: est.response_tokens_est,
                counterfactual_files: if est.savings_eligible {
                    Some(est.counterfactual_files)
                } else {
                    None
                },
                counterfactual_exact_files: if est.savings_eligible {
                    Some(est.counterfactual_exact_files)
                } else {
                    None
                },
                counterfactual_tokens_est: if est.savings_eligible {
                    Some(est.counterfactual_tokens_est)
                } else {
                    None
                },
                tokens_saved_est: if est.savings_eligible {
                    Some(est.tokens_saved_est)
                } else {
                    None
                },
                duration_ms: Some(duration_ms),
                ok: true,
                savings_eligible: est.savings_eligible,
                response_preview: est.response_preview.clone(),
                counterfactual_preview: est.counterfactual_preview.clone(),
            });
            Ok(wrapped)
        }
        Err(msg) => {
            if verbose {
                push_error(name, msg);
            }
            let err_val = json!({ "error": msg });
            let text = tool_result_text(&err_val);
            spawn_record_mcp_call(McpCallRecord {
                tool: name.to_string(),
                project,
                response_chars: text.len() as i64,
                response_tokens_est: ax_usage::count_tokens(&text) as i64,
                counterfactual_files: None,
                counterfactual_exact_files: None,
                counterfactual_tokens_est: None,
                tokens_saved_est: None,
                duration_ms: Some(duration_ms),
                ok: false,
                savings_eligible: false,
                response_preview: if text.is_empty() {
                    None
                } else {
                    Some(ax_usage::truncate_utf8(&text, ax_usage::PREVIEW_MAX_BYTES))
                },
                counterfactual_preview: None,
            });
            Ok(wrap_call_tool_result(err_val, true))
        }
    }
}

/// MCP `tools/call` must return `{ content: [{ type, text }], structuredContent?, isError? }`.
/// Raw JSON objects are invisible in strict clients (VS Code / Antigravity) and may not reach the model.
fn wrap_call_tool_result(value: Value, is_error: bool) -> Value {
    let structured = Some(value.clone());
    wrap_call_tool_result_parts(&value, structured, is_error, None)
}

/// Build the MCP `tools/call` envelope from an explicit text source and an
/// optional structured payload. `content.text` is what strict clients (and
/// Cursor) feed the model; `structuredContent` is machine-readable metadata for
/// clients that consume it. Passing `structured = None` omits it entirely so a
/// text-authoritative response is not duplicated on the wire.
fn wrap_call_tool_result_parts(
    text_source: &Value,
    structured: Option<Value>,
    is_error: bool,
    hint: Option<String>,
) -> Value {
    let mut text = tool_result_text(text_source);
    if let Some(hint) = hint {
        text.push_str("\n\n");
        text.push_str(&hint);
    }
    let mut out = json!({
        "content": [{ "type": "text", "text": text }],
        "isError": is_error,
    });
    if let Some(structured) = structured {
        out["structuredContent"] = structured;
    }
    out
}

/// Lean by default. Set `AX_MCP_FULL=1` (or `true`/`yes`) to restore the full
/// structuredContent payload for clients that rely on it.
fn render_full() -> bool {
    std::env::var("AX_MCP_FULL")
        .map(|v| {
            let v = v.trim();
            v == "1" || v.eq_ignore_ascii_case("true") || v.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

/// Project a tool's full result down to a lean `structuredContent` payload that
/// drops fields already carried verbatim in `content.text`. Returns `None` for
/// text-authoritative data tools so `structuredContent` is omitted entirely.
fn lean_structured(name: &str, value: &Value) -> Option<Value> {
    match name {
        // Numbered source, callers, and callees are already in content.text.
        // Keep only a compact entry index for programmatic use.
        "ax_explore" => Some(json!({
            "query": value.get("query"),
            "summary": value.get("summary"),
            "blastRadius": value.get("blastRadius"),
            "entries": explore_entries_compact(value),
        })),
        // The inject block (with full rule/skill/memory/index bodies) is in
        // content.text. Keep only counts and machine-actionable fields.
        "ax_preflight" => Some(json!({
            "policyStatus": value.get("policyStatus"),
            "matchedRules": value.get("matchedRules"),
            "matchedSkills": value.get("matchedSkills"),
            "matchedMemories": value.get("matchedMemories"),
            "guardRequired": value.get("guardRequired"),
            "mode": value.get("mode"),
            "directiveDetected": value.get("directiveDetected"),
            "captureProposal": value.get("captureProposal"),
            "instruction": value.get("instruction"),
            "indexStats": value.get("indexStats"),
            "pendingFiles": value.get("pendingFiles"),
        })),
        // Summary text is in content.text; keep structured stats for scripts.
        "ax_status" => Some(json!({
            "stats": value.get("stats"),
            "lastIndexedAt": value.get("lastIndexedAt"),
            "pendingFiles": value.get("pendingFiles"),
            "policy": value.get("policy"),
        })),
        // The markdown context is in content.text; drop the heavy graph payload.
        "ax_context" => Some(json!({
            "query": value.get("query"),
            "summary": value.get("summary"),
            "stats": value.get("stats"),
            "relatedFiles": value.get("relatedFiles"),
        })),
        // The skill body is content.text; keep the metadata envelope.
        "ax_skill" => {
            let mut trimmed = value.clone();
            if let Some(obj) = trimmed.as_object_mut() {
                obj.remove("body");
            }
            Some(trimmed)
        }
        // Data tools carry a compact text projection; the JSON would only
        // duplicate it, so omit structuredContent.
        "ax_search" | "ax_node" | "ax_callers" | "ax_callees" | "ax_impact" | "ax_files"
        | "ax_affected" => None,
        // Everything else (status, index, rules, capture, remember, recall,
        // insights, report) is already compact or machine-first — keep it.
        _ => Some(value.clone()),
    }
}

/// Reduce `entries[]` to `{name,file,startLine,endLine,score}` — dropping the
/// duplicated `source`, `callers`, and `callees` that live in content.text.
fn explore_entries_compact(value: &Value) -> Value {
    let entries = value.get("entries").and_then(|v| v.as_array());
    let compact: Vec<Value> = entries
        .map(|arr| {
            arr.iter()
                .map(|e| {
                    let node = e.get("node").unwrap_or(e);
                    json!({
                        "name": node.get("qualifiedName").or_else(|| node.get("name")),
                        "file": node.get("filePath"),
                        "startLine": node.get("startLine"),
                        "endLine": node.get("endLine"),
                        "score": e.get("score"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Value::Array(compact)
}

/// Above this size, nudge the agent toward narrower queries instead of a follow-up dump.
const TOKEN_HINT_THRESHOLD: i64 = 3_000;

/// One-line budget hint appended to large tool responses so agents self-correct
/// (narrower depth/limit) instead of pulling ever-bigger contexts.
fn token_budget_hint(tool: &str, response_tokens: i64) -> Option<String> {
    if response_tokens < TOKEN_HINT_THRESHOLD {
        return None;
    }
    let advice = match tool {
        "ax_explore" | "ax_context" => "narrow the question or pass a smaller depth",
        "ax_search" | "ax_files" | "ax_recall" => "add a `limit` or a more specific query",
        "ax_impact" | "ax_affected" | "ax_callers" | "ax_callees" => "reduce depth or target a more specific symbol",
        _ => "use a more specific query",
    };
    Some(format!(
        "[ax] token budget: this response is ~{}k tokens; {} to keep context small.",
        (response_tokens + 500) / 1000,
        advice
    ))
}

fn tool_result_text(value: &Value) -> String {
    if let Some(inject) = value.get("inject").and_then(|v| v.as_str()) {
        if !inject.is_empty() {
            return inject.to_string();
        }
    }
    if let Some(preview) = value.get("preview").and_then(|v| v.as_str()) {
        if !preview.is_empty() {
            return preview.to_string();
        }
    }
    if let Some(proposal) = value.get("proposal") {
        if let Some(preview) = proposal.get("preview").and_then(|v| v.as_str()) {
            if !preview.is_empty() {
                return preview.to_string();
            }
        }
    }
    if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            return text.to_string();
        }
    }
    if let Some(body) = value.get("body").and_then(|v| v.as_str()) {
        if !body.is_empty() {
            return body.to_string();
        }
    }
    // Compact (not pretty) JSON: no gain from indentation whitespace for a model.
    value.to_string()
}

fn is_policy_tool(name: &str) -> bool {
    matches!(
        name,
        "ax_preflight" | "ax_rules" | "ax_skill" | "ax_policy_capture" | "ax_guard" | "ax_status"
    )
}

#[cfg(test)]
mod wrap_tests {
    use super::*;

    #[test]
    fn wrap_preflight_puts_inject_in_content_text() {
        let raw = json!({
            "inject": "<ax_policy>team rules</ax_policy>",
            "matchedRules": 4,
        });
        let wrapped = wrap_call_tool_result(raw.clone(), false);
        let text = wrapped["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("<ax_policy>"));
        assert_eq!(wrapped["structuredContent"], raw);
        assert_eq!(wrapped["isError"], false);
    }

    #[test]
    fn wrap_error_sets_is_error() {
        let wrapped = wrap_call_tool_result(json!({ "error": "skill not found" }), true);
        assert_eq!(wrapped["isError"], true);
        assert!(wrapped["content"][0]["text"].as_str().unwrap().contains("skill not found"));
    }

    #[test]
    fn lean_explore_drops_source_and_neighbors() {
        let raw = json!({
            "text": "# Explore: f\n...",
            "query": "f",
            "summary": "Found 1",
            "blastRadius": "1 entry",
            "entries": [{
                "node": { "qualifiedName": "f", "filePath": "a.rs", "startLine": 1, "endLine": 9, "kind": "Function" },
                "score": 0.9,
                "source": "1\tfn f() {}",
                "callers": [{ "qualifiedName": "c" }],
                "callees": []
            }]
        });
        let lean = lean_structured("ax_explore", &raw).expect("explore keeps structured");
        assert!(lean.get("text").is_none(), "text must not be duplicated");
        let entry = &lean["entries"][0];
        assert_eq!(entry["name"], "f");
        assert_eq!(entry["file"], "a.rs");
        assert_eq!(entry["startLine"], 1);
        assert!(entry.get("source").is_none(), "source lives in content.text");
        assert!(entry.get("callers").is_none(), "callers live in content.text");
    }

    #[test]
    fn lean_status_drops_text_keeps_stats() {
        let raw = json!({
            "text": "## ax Status\n\nDocs: 56 — 43 md, 10 json",
            "stats": { "nodeCount": 100, "docsByExtension": { "md": 43 } },
            "lastIndexedAt": 123,
            "pendingFiles": [],
        });
        let lean = lean_structured("ax_status", &raw).expect("status keeps structured");
        assert!(lean.get("text").is_none(), "text lives in content.text");
        assert_eq!(lean["stats"]["nodeCount"], 100);
    }

    #[test]
    fn lean_preflight_drops_bodies_keeps_actionable() {
        let raw = json!({
            "inject": "<ax_policy>huge bodies</ax_policy>",
            "rules": [{ "body": "full rule body" }],
            "skills": [{ "body": "full skill body" }],
            "memories": [{ "body": "memory" }],
            "matchedRules": 3,
            "matchedSkills": 1,
            "matchedMemories": 0,
            "directiveDetected": true,
            "captureProposal": { "questions": [] },
            "guardRequired": true,
            "mode": "enforce",
            "instruction": "Apply CRITICAL rules",
            "policyStatus": {}
        });
        let lean = lean_structured("ax_preflight", &raw).expect("preflight keeps structured");
        assert!(lean.get("rules").is_none());
        assert!(lean.get("skills").is_none());
        assert!(lean.get("memories").is_none());
        assert!(lean.get("inject").is_none());
        assert_eq!(lean["directiveDetected"], true);
        assert_eq!(lean["matchedRules"], 3);
        assert!(lean.get("captureProposal").is_some());
    }

    #[test]
    fn lean_data_tools_omit_structured() {
        assert!(lean_structured("ax_search", &json!({ "results": [] })).is_none());
        assert!(lean_structured("ax_node", &json!({ "nodes": [] })).is_none());
        assert!(lean_structured("ax_impact", &json!({ "nodes": {} })).is_none());
    }

    #[test]
    fn lean_context_drops_graph_payload() {
        let raw = json!({
            "text": "# Task Context",
            "query": "q",
            "summary": "s",
            "stats": { "nodeCount": 1 },
            "relatedFiles": ["a.rs"],
            "subgraph": { "nodes": {}, "edges": [] },
            "codeBlocks": [{ "content": "big" }]
        });
        let lean = lean_structured("ax_context", &raw).expect("context keeps structured");
        assert!(lean.get("subgraph").is_none());
        assert!(lean.get("codeBlocks").is_none());
        assert_eq!(lean["relatedFiles"][0], "a.rs");
    }

    #[test]
    fn wrap_parts_omits_structured_when_none() {
        let raw = json!({ "text": "compact list" });
        let wrapped = wrap_call_tool_result_parts(&raw, None, false, None);
        assert!(wrapped.get("structuredContent").is_none());
        assert_eq!(wrapped["content"][0]["text"], "compact list");
    }

    #[test]
    fn wrap_parts_keeps_structured_when_some() {
        let raw = json!({ "text": "t", "entries": [1, 2] });
        let wrapped = wrap_call_tool_result_parts(&raw, Some(raw.clone()), false, None);
        assert_eq!(wrapped["structuredContent"], raw);
    }
}

#[cfg(test)]
mod policy_integration {
    use super::*;
    use std::path::PathBuf;

    fn repo_root() -> Option<PathBuf> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .canonicalize()
            .ok()?;
        if root.join(".ax").join("ax.db").exists() {
            Some(root)
        } else {
            None
        }
    }

    #[tokio::test]
    async fn mcp_ax_rules_returns_indexed_rules() {
        let Some(root) = repo_root() else {
            return;
        };
        let mut engine = McpEngine::with_project_root(root);
        let result = handle_request(
            &mut engine,
            "tools/call",
            json!({ "name": "ax_rules", "arguments": {} }),
        )
        .await
        .result
        .expect("ax_rules call");
        let structured = result
            .get("structuredContent")
            .expect("MCP structuredContent");
        let rules = structured
            .get("rules")
            .and_then(|v| v.as_array())
            .expect("rules array");
        assert!(
            rules.len() >= 4,
            "expected indexed rules, got {}",
            rules.len()
        );
    }

    #[tokio::test]
    async fn mcp_ax_status_policy_counts() {
        let Some(root) = repo_root() else {
            return;
        };
        let mut engine = McpEngine::with_project_root(root);
        let result = handle_request(
            &mut engine,
            "tools/call",
            json!({ "name": "ax_status", "arguments": {} }),
        )
        .await
        .result
        .expect("ax_status call");
        let structured = result
            .get("structuredContent")
            .expect("MCP structuredContent");
        let policy = structured.get("policy").expect("policy block");
        let rules = policy.get("rules").and_then(|v| v.as_u64()).unwrap_or(0);
        assert!(rules >= 4, "policy.rules should be >= 4, got {rules}");
    }
}