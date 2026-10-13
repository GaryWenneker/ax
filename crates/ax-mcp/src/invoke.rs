//! In-process MCP tool invocation (for ax-agent / ax-web).

use serde_json::Value;

use crate::engine::McpEngine;
use crate::tools::ToolHandler;

pub fn format_tool_result(value: &Value) -> String {
    crate::smart_output::model_text(value)
}

fn is_policy_tool(name: &str) -> bool {
    matches!(
        name,
        "ax_preflight" | "ax_rules" | "ax_skill" | "ax_policy_capture" | "ax_guard" | "ax_status"
    )
}

/// Call an ax MCP tool in-process (no stdio JSON-RPC).
pub async fn call_tool(engine: &mut McpEngine, name: &str, args: Value) -> Result<Value, String> {
    let args = crate::request_context::public_args(args);
    let root = crate::server::resolve_request_project_root(engine)
        .ok_or("no initialized project for MCP request")?;
    crate::request_context::validate(&root, &args)?;
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
    let (args, _, _) = crate::server::prepare_call_args(engine, name, args);
    let result = {
        let mut guard = engine.lock_ax().await;
        let ax = guard.as_mut().ok_or("ax not initialized")?;
        ToolHandler::call_tool(ax, name, args).await?
    };
    Ok(crate::server::record_policy_delivery(engine, result))
}
