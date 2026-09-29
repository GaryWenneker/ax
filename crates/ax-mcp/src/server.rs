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
        repair_hooks_at_startup(root);
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

fn repair_hooks_at_startup(root: &std::path::Path) {
    if let Err(e) = ax_sync::repair_git_hooks(root) {
        eprintln!("ax: git hook repair skipped: {e}");
    }
}

fn resolve_request_project_root(engine: &McpEngine) -> Option<PathBuf> {
    engine
        .project_root()
        .cloned()
        .or_else(|| resolve_mcp_project_root(None))
}

const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

fn negotiate_protocol_version(params: &Value) -> &'static str {
    let requested = params.get("protocolVersion").and_then(|v| v.as_str());
    SUPPORTED_PROTOCOL_VERSIONS
        .iter()
        .find(|v| Some(**v) == requested)
        .copied()
        .unwrap_or(SUPPORTED_PROTOCOL_VERSIONS[0])
}

pub async fn handle_request(engine: &mut McpEngine, method: &str, params: Value) -> RequestOutcome {
    let project_root = resolve_request_project_root(engine);
    let has_policy = project_root
        .as_ref()
        .map(|p| ax_policy::policy_tools_enabled(p.as_path()))
        .unwrap_or(false);

    match method {
        "initialize" => {
            let client = params
                .pointer("/clientInfo/name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            engine.policy_sessions().begin(client);
            RequestOutcome::ok(json!({
            "protocolVersion": negotiate_protocol_version(&params),
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "ax",
                "version": env!("CARGO_PKG_VERSION"),
                "icons": [{
                    "src": concat!("data:image/png;base64,", include_str!("../assets/ax-icon.png.b64")),
                    "mimeType": "image/png",
                    "sizes": ["128x128"]
                }]
            },
            "instructions": server_instructions(has_policy),
            }))
        }
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

/// Private args key: what this connection's agent already received.
pub(crate) const SESSION_ARG: &str = "__axSession";
/// Private result key: bodies preflight sent this time. Stripped before the reply.
pub(crate) const DELIVERED_KEY: &str = "__axDelivered";

fn policy_session_ttl() -> std::time::Duration {
    std::env::var("AX_POLICY_SESSION_TTL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(std::time::Duration::from_secs)
        .unwrap_or(crate::policy_session::DEFAULT_TTL)
}

fn attach_policy_session(engine: &mut McpEngine, mut args: Value) -> Value {
    let chat = ax_usage::read_active_cursor_session();
    let view = engine
        .policy_sessions()
        .view(chat.as_deref(), std::time::Instant::now(), policy_session_ttl());
    if !args.is_object() {
        args = json!({});
    }
    args[SESSION_ARG] = json!({ "client": view.client_name, "delivered": view.delivered });
    args
}

fn record_policy_delivery(engine: &mut McpEngine, mut value: Value) -> Value {
    let Some(delivered) = value.as_object_mut().and_then(|obj| obj.remove(DELIVERED_KEY)) else {
        return value;
    };
    let pairs: Vec<(String, u64)> = delivered
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|pair| Some((pair.get(0)?.as_str()?.to_string(), pair.get(1)?.as_u64()?)))
        .collect();
    engine.policy_sessions().record(pairs);
    value
}

/// Run one tool, wrap the reply for MCP, and log the call with its token
/// savings estimate via `spawn_record_mcp_call`.
async fn call_tool_and_wrap(
    engine: &mut McpEngine,
    name: &str,
    args: Value,
    project_root: Option<&std::path::Path>,
    verbose: bool,
) -> Result<Value, String> {
    if is_policy_tool(name) {
        if let Err(e) = engine.ensure_policy_fresh().await {
            tracing::warn!("ensure_policy_fresh failed (tool {name} continues): {e}");
            engine.ensure_initialized().await?;
            engine.reopen_if_replaced().await?;
        }
    } else {
        engine.ensure_initialized().await?;
        engine.reopen_if_replaced().await?;
    }
    if verbose {
        push_inbound(name, &args);
    }
    let args = if matches!(name, "ax_preflight" | "ax_skill") { attach_policy_session(engine, args) } else { args };
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
    let result = result.map(|value| record_policy_delivery(engine, value));
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
            let pending = project_root
                .map(ax_sync::global_pending_files)
                .unwrap_or_default();
            let annotated = crate::staleness::annotate_staleness(&text, value, &pending);
            let cached = ax_usage::cache_oversized_reply(name, &annotated).await;
            let (model_text, structured, hint, response_tokens, response_chars, tokens_saved) =
                match &cached {
                    ax_usage::CacheOutcome::Stubbed {
                        text: stub,
                        id,
                        original_tokens,
                        sent_tokens,
                        removed_tokens,
                    } => {
                        let saved = if est.savings_eligible {
                            est.tokens_saved_est.saturating_add(*removed_tokens)
                        } else {
                            est.tokens_saved_est
                        };
                        (
                            stub.clone(),
                            Some(json!({
                                "contextCacheId": id,
                                "originalTokens": original_tokens,
                                "sentTokens": sent_tokens,
                                "removedTokens": removed_tokens,
                            })),
                            None,
                            *sent_tokens,
                            stub.len() as i64,
                            saved,
                        )
                    }
                    ax_usage::CacheOutcome::Passthrough => {
                        let model_text = annotated.clone();
                        (
                            model_text.clone(),
                            structured,
                            token_budget_hint(name, est.response_tokens_est),
                            est.response_tokens_est,
                            model_text.len() as i64,
                            est.tokens_saved_est,
                        )
                    }
                };
            let text_source = json!({ "text": model_text });
            let wrapped = wrap_call_tool_result_parts(
                &text_source,
                structured,
                value
                    .get("isError")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                hint,
            );
            if verbose {
                push_outbound(name, &wrapped, full, duration_ms);
            }
            let cache_id = match &cached {
                ax_usage::CacheOutcome::Stubbed { id, .. } => Some(id.clone()),
                ax_usage::CacheOutcome::Passthrough => None,
            };
            ax_usage::spawn_note_session_event(
                ax_usage::read_active_cursor_session(),
                name.to_string(),
                annotated,
                cache_id,
            );
            spawn_record_mcp_call(McpCallRecord {
                tool: name.to_string(),
                project,
                response_chars,
                response_tokens_est: response_tokens,
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
                    Some(tokens_saved)
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
    // Preflight size follows team policy, not the query, so the advice would not help.
    if response_tokens < TOKEN_HINT_THRESHOLD || tool == "ax_preflight" {
        return None;
    }
    let advice = match tool {
        "ax_explore" | "ax_context" => "narrow the question or pass a smaller depth",
        "ax_search" | "ax_files" | "ax_recall" => "add a `limit` or a more specific query",
        "ax_impact" | "ax_affected" | "ax_callers" | "ax_callees" => "reduce depth or target a more specific symbol",
        _ => "use a more specific query",
    };
    Some(ax_usage::format_ax_tagged(format!(
        "token budget: this response is ~{}k tokens; {} to keep context small.",
        (response_tokens + 500) / 1000,
        advice
    )))
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
    fn preflight_gets_no_token_budget_banner() {
        assert!(token_budget_hint("ax_preflight", 9_000).is_none());
        assert!(token_budget_hint("ax_explore", 9_000).is_some());
    }

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

    fn reply_text(result: &Value) -> String {
        result["content"][0]["text"].as_str().unwrap_or_default().to_string()
    }

    /// A project whose only policy is three always-apply rules without links, so reply sizes
    /// depend on session skipping alone and not on this repo's live policy.
    async fn session_fixture() -> PathBuf {
        let root = std::env::temp_dir().join(format!("ax-mcp-session-{}", uuid::Uuid::new_v4()));
        let rules = root.join(".agents").join("rules");
        std::fs::create_dir_all(&rules).unwrap();
        for id in ["alpha", "beta", "gamma"] {
            let body = format!("## Rules\n\n{}", format!("- Rule {id} applies to every change.\n").repeat(80));
            std::fs::write(
                rules.join(format!("{id}.mdc")),
                format!("---\nid: {id}\nlevel: WARNING\nalwaysApply: true\n---\n\n{body}"),
            )
            .unwrap();
        }
        let ax = ax_core::Ax::init(&root).await.unwrap();
        ax_policy::index_policy(ax.db_pool(), &root, true).await.unwrap();
        root
    }

    #[tokio::test]
    async fn second_preflight_on_a_connection_skips_unchanged_bodies() {
        let root = session_fixture().await;
        let mut engine = McpEngine::with_project_root(root.clone());
        handle_request(&mut engine, "initialize", json!({ "clientInfo": { "name": "gauntlet" } })).await;
        let call = json!({ "name": "ax_preflight", "arguments": { "prompt": "fix a bug" } });
        let first = handle_request(&mut engine, "tools/call", call.clone()).await.result.expect("first");
        let second = handle_request(&mut engine, "tools/call", call).await.result.expect("second");
        let (first, second) = (reply_text(&first), reply_text(&second));
        assert!(first.contains("## Rules (always apply)"), "{first}");
        assert!(!first.contains(DELIVERED_KEY) && !second.contains(DELIVERED_KEY));
        assert!(second.contains("Unchanged since earlier in this session"), "{second}");
        assert!(second.len() * 2 < first.len(), "first={} second={}", first.len(), second.len());
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn node_signature_mode_returns_location_and_declaration_only() {
        let Some(root) = repo_root() else {
            return;
        };
        let mut engine = McpEngine::with_project_root(root);
        let call = |mode: Option<&str>| {
            let mut args = json!({ "name": "lean_structured" });
            if let Some(m) = mode {
                args["mode"] = json!(m);
            }
            json!({ "name": "ax_node", "arguments": args })
        };
        let sig = handle_request(&mut engine, "tools/call", call(Some("signature"))).await.result.expect("sig");
        let full = handle_request(&mut engine, "tools/call", call(None)).await.result.expect("full");
        let (sig, full) = (reply_text(&sig), reply_text(&full));
        assert!(sig.contains("crates/ax-mcp/src/server.rs"), "{sig}");
        assert!(sig.contains("fn lean_structured"), "{sig}");
        assert!(!sig.contains("explore_entries_compact(value)"), "{sig}");
        assert!(!sig.contains("### Callers"), "{sig}");
        assert!(full.contains("explore_entries_compact(value)"), "{full}");
        assert!(sig.len() * 3 < full.len(), "sig={} full={}", sig.len(), full.len());
    }

    #[tokio::test]
    async fn unchanged_catalog_and_memory_titles_are_sent_once_per_session() {
        let Some(root) = repo_root() else {
            return;
        };
        let mut engine = McpEngine::with_project_root(root);
        handle_request(&mut engine, "initialize", json!({ "clientInfo": { "name": "gauntlet" } })).await;
        let call = json!({ "name": "ax_preflight", "arguments": { "prompt": "fix a bug" } });
        let first = reply_text(&handle_request(&mut engine, "tools/call", call.clone()).await.result.expect("first"));
        let second = reply_text(&handle_request(&mut engine, "tools/call", call).await.result.expect("second"));
        assert!(!second.contains("<ax_memory_titles>"), "memory titles resent:\n{second}");
        assert!(first.contains("<ax_index"), "{first}");
        assert!(!second.contains("<ax_index"), "unchanged index snapshot resent:\n{second}");
        let catalog_ids = |text: &str| -> Vec<String> {
            text.split("<ax_context_catalog>")
                .nth(1)
                .map(|rest| {
                    rest.lines()
                        .filter_map(|l| l.strip_prefix("- ")?.split_whitespace().next().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        };
        let first_ids = catalog_ids(&first);
        for id in catalog_ids(&second) {
            assert!(!first_ids.contains(&id), "catalog entry {id} resent");
        }
        assert!(first.contains("<ax_memory_titles>"), "fixture repo should have memories:\n{first}");
    }

    #[tokio::test]
    async fn node_with_exact_qualified_name_returns_only_that_symbol() {
        let Some(root) = repo_root() else {
            return;
        };
        let mut engine = McpEngine::with_project_root(root);
        let call = json!({ "name": "ax_node", "arguments": { "name": "crates/ax-mcp/src/tools.rs::skill" } });
        let text = reply_text(&handle_request(&mut engine, "tools/call", call).await.result.expect("node"));
        assert_eq!(text.matches("## Entry").count(), 1, "{text}");
        assert!(text.contains("crates/ax-mcp/src/tools.rs::skill"), "{text}");
    }

    #[tokio::test]
    async fn second_skill_load_in_a_session_is_a_short_notice() {
        let Some(root) = repo_root() else {
            return;
        };
        let mut engine = McpEngine::with_project_root(root);
        handle_request(&mut engine, "initialize", json!({ "clientInfo": { "name": "gauntlet" } })).await;
        let preflight = json!({ "name": "ax_preflight", "arguments": { "prompt": "start" } });
        handle_request(&mut engine, "tools/call", preflight).await.result.expect("preflight");
        let call = json!({ "name": "ax_skill", "arguments": { "name": "startup" } });
        let first = reply_text(&handle_request(&mut engine, "tools/call", call.clone()).await.result.expect("first"));
        let second = reply_text(&handle_request(&mut engine, "tools/call", call).await.result.expect("second"));
        assert!(first.contains("SS-00"), "first load must carry the body:\n{first}");
        assert!(!second.contains("SS-00"), "second load resent the body:\n{second}");
        assert!(second.contains("startup"), "{second}");
        assert!(second.len() < 400, "{second}");
    }

    #[tokio::test]
    async fn new_initialize_resends_bodies() {
        let Some(root) = repo_root() else {
            return;
        };
        let mut engine = McpEngine::with_project_root(root);
        let call = json!({ "name": "ax_preflight", "arguments": { "prompt": "fix a bug" } });
        handle_request(&mut engine, "initialize", json!({ "clientInfo": { "name": "gauntlet" } })).await;
        let first = handle_request(&mut engine, "tools/call", call.clone()).await.result.expect("first");
        handle_request(&mut engine, "initialize", json!({ "clientInfo": { "name": "gauntlet" } })).await;
        let again = handle_request(&mut engine, "tools/call", call).await.result.expect("again");
        assert!(!reply_text(&again).contains("Unchanged since earlier in this session"));
        assert!(reply_text(&again).len() * 10 > reply_text(&first).len() * 9);
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

#[cfg(test)]
mod hook_repair {
    use super::repair_hooks_at_startup;
    use std::fs;
    use std::path::PathBuf;

    fn temp_repo(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ax-hook-repair-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn startup_repairs_broken_ax_hook() {
        let dir = temp_repo("broken");
        let hooks = dir.join(".git").join("hooks");
        fs::create_dir_all(&hooks).unwrap();
        fs::write(hooks.join("post-commit"), "ax sync --quiet\n").unwrap();
        repair_hooks_at_startup(&dir);
        let content = fs::read_to_string(hooks.join("post-commit")).unwrap();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(content, "#!/bin/sh\nax sync --quiet\n");
    }

    #[test]
    fn startup_repair_never_panics_on_unreadable_repo() {
        let dir = temp_repo("gitfile");
        fs::write(dir.join(".git"), "gitdir: elsewhere\n").unwrap();
        repair_hooks_at_startup(&dir);
        fs::remove_dir_all(&dir).unwrap();
    }
}
