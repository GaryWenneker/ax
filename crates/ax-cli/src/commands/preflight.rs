//! `ax preflight` — shared context engine (same as `ax_preflight` MCP tool).

use std::io::Read;
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::commands::resolve_path;

pub struct PreflightArgs {
    pub path: Option<String>,
    pub prompt: Option<String>,
    pub prompt_stdin: bool,
    pub files: Vec<String>,
    pub session: Option<String>,
    pub context_mode: Option<String>,
    pub known_context: Option<String>,
    pub context_reset: bool,
    pub json: bool,
}

pub async fn run(args: PreflightArgs) -> Result<(), String> {
    let root = resolve_path(args.path);
    let prompt = if args.prompt_stdin {
        read_stdin()?
    } else {
        args.prompt.unwrap_or_default()
    };
    let started = std::time::Instant::now();
    let mut params = json!({
        "prompt": prompt,
        "files": args.files,
        "context_reset": args.context_reset,
    });
    if let Some(session) = &args.session {
        params["session"] = json!(session);
    }
    if let Some(mode) = &args.context_mode {
        params["contextMode"] = json!(mode);
    }
    if let Some(known) = &args.known_context {
        params["known_context"] = json!(known);
    }

    let result = invoke_shared_tool(&root, "ax_preflight", params).await?;
    let ms = started.elapsed().as_millis() as i64;
    let _ = ax_usage::record_cli_command(
        &root,
        "preflight",
        args.session.as_deref(),
        true,
        Some(ms),
        "cli preflight",
    )
    .await;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?);
    } else {
        println!("{}", ax_mcp::format_tool_result(&result));
    }
    Ok(())
}

fn read_stdin() -> Result<String, String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// In-process shared tool runtime (no MCP stdio transport).
pub async fn invoke_shared_tool(
    root: &PathBuf,
    name: &str,
    args: Value,
) -> Result<Value, String> {
    let mut engine = ax_mcp::McpEngine::with_project_root(root.clone());
    ax_mcp::call_tool(&mut engine, name, args).await
}
