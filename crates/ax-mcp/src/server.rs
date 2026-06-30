//! MCP stdio server loop.

use serde_json::{json, Value};

use ax_context::directory::find_nearest_ax_root;

use crate::engine::McpEngine;
use crate::liveness_watchdog::install_main_thread_watchdog;
use crate::ppid_watchdog::spawn_ppid_watchdog;
use crate::proxy::attach_or_spawn;
use crate::tools::{server_instructions, ToolHandler};
use crate::transport::{is_notification, StdioTransport, PARSE_ERROR, METHOD_NOT_FOUND};
use ax_telemetry::telemetry;

pub async fn run_stdio_server() -> Result<(), Box<dyn std::error::Error>> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    if let Some(root) = find_nearest_ax_root(&cwd) {
        if attach_or_spawn(&root).await.is_ok() {
            return Ok(());
        }
    }

    spawn_ppid_watchdog(|| std::process::exit(0));
    let _liveness = install_main_thread_watchdog();

    let mut engine = McpEngine::new();
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

pub async fn handle_request(engine: &mut McpEngine, method: &str, params: Value) -> Result<Value, String> {
    let has_policy = engine
        .project_root()
        .map(|p| ax_policy::policy_exists(p.as_path()))
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
            engine.ensure_initialized().await?;
            engine.reopen_if_replaced().await?;
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(Value::Null);
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
            result
        }
        "notifications/initialized" => Ok(Value::Null),
        _ => Err(format!("method not found: {}", method)),
    }
}