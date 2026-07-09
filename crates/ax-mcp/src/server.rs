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
use ax_telemetry::telemetry;

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
        Some(root) => McpEngine::with_project_root(root),
        None => McpEngine::new(),
    };
    loop {
        match StdioTransport::read_request() {
            Ok(req) => {
                let result = handle_request(&mut engine, &req.method, req.params.unwrap_or(Value::Null)).await;
                if is_notification(&req.id) {
                    continue;
                }
                let id = req.id.clone().unwrap_or(Value::Null);
                match result {
                    Ok(value) => StdioTransport::send_result(id, value)?,
                    Err(msg) => StdioTransport::send_error(Some(id), METHOD_NOT_FOUND, &msg)?,
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

pub async fn handle_request(engine: &mut McpEngine, method: &str, params: Value) -> Result<Value, String> {
    let project_root = resolve_request_project_root(engine);
    let has_policy = project_root
        .as_ref()
        .map(|p| ax_policy::policy_tools_enabled(p.as_path()))
        .unwrap_or(false);

    match method {
        "initialize" => Ok(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "ax", "version": env!("CARGO_PKG_VERSION") },
            "instructions": server_instructions(has_policy),
        })),
        "tools/list" => Ok(ToolHandler::list_tools(has_policy).await),
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
            if is_policy_tool(name) {
                engine.ensure_policy_fresh().await?;
            } else {
                engine.ensure_initialized().await?;
                engine.reopen_if_replaced().await?;
            }
            let result = if let Some(pool) = engine.query_pool() {
                if pool.healthy() && crate::query_pool::is_read_tool(name) {
                    pool
                        .run(|| async {
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
            match result {
                Ok(value) => Ok(wrap_call_tool_result(value, false)),
                Err(msg) => Ok(wrap_call_tool_result(
                    json!({ "error": msg }),
                    true,
                )),
            }
        }
        "notifications/initialized" => Ok(Value::Null),
        _ => Err(format!("method not found: {}", method)),
    }
}

/// MCP `tools/call` must return `{ content: [{ type, text }], structuredContent?, isError? }`.
/// Raw JSON objects are invisible in strict clients (VS Code / Antigravity) and may not reach the model.
fn wrap_call_tool_result(value: Value, is_error: bool) -> Value {
    let text = tool_result_text(&value);
    json!({
        "content": [{ "type": "text", "text": text }],
        "structuredContent": value,
        "isError": is_error,
    })
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
    serde_json::to_string_pretty(value).unwrap_or_else(|_| value.to_string())
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
        .expect("ax_status call");
        let structured = result
            .get("structuredContent")
            .expect("MCP structuredContent");
        let policy = structured.get("policy").expect("policy block");
        let rules = policy.get("rules").and_then(|v| v.as_u64()).unwrap_or(0);
        assert!(rules >= 4, "policy.rules should be >= 4, got {rules}");
    }
}