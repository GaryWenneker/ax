//! In-process MCP tool invocation (for ax-agent / ax-web).

use serde_json::Value;

use crate::engine::McpEngine;
use crate::tools::ToolHandler;

pub fn format_tool_result(value: &Value) -> String {
    if let Some(inject) = value.get("inject").and_then(|v| v.as_str()) {
        if !inject.is_empty() {
            return inject.to_string();
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
    // Compact (not pretty) JSON — parity with the stdio server's tool_result_text.
    value.to_string()
}

fn is_policy_tool(name: &str) -> bool {
    matches!(
        name,
        "ax_preflight" | "ax_rules" | "ax_skill" | "ax_policy_capture" | "ax_guard" | "ax_status"
    )
}

/// Call an ax MCP tool in-process (no stdio JSON-RPC).
pub async fn call_tool(
    engine: &mut McpEngine,
    name: &str,
    args: Value,
) -> Result<Value, String> {
    if is_policy_tool(name) {
        engine.ensure_policy_fresh().await?;
    } else {
        engine.ensure_initialized().await?;
        engine.reopen_if_replaced().await?;
    }
    let mut guard = engine.lock_ax().await;
    let ax = guard.as_mut().ok_or("ax not initialized")?;
    ToolHandler::call_tool(ax, name, args).await
}
