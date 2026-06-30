//! MCP tools - ax_explore, ax_search, ax_status, policy tools, etc.

use std::path::PathBuf;

use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;
use ax_context::format_explore_text;
use ax_policy::{GuardOp, MatchInput};
use ax_reasoning::maybe_synthesize_explore;
use ax_types::{BuildContextOptions, ExploreOptions, SearchOptions, TaskInput};
use serde_json::{json, Value};

pub struct ToolHandler;

impl ToolHandler {
    pub async fn list_tools(project_has_policy: bool) -> Value {
        let mut tools = vec![explore_tool()];
        if project_has_policy {
            tools.push(preflight_tool());
            tools.push(rules_tool());
            tools.push(skill_tool());
            tools.push(guard_tool());
        }
        tools.extend(extra_tools());
        json!({ "tools": tools })
    }

    pub async fn call_tool(ax: &mut Ax, name: &str, params: Value) -> Result<Value, String> {
        match name {
            "ax_explore" => explore(ax, params).await,
            "ax_preflight" => preflight(ax, params).await,
            "ax_rules" => rules(ax, params).await,
            "ax_skill" => skill(ax, params).await,
            "ax_guard" => guard(ax, params).await,
            "ax_search" => {
                let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let results = ax.search_nodes(query, &SearchOptions { limit: Some(20), ..Default::default() }).await.map_err(|e| e.to_string())?;
                Ok(json!({ "results": results }))
            }
            "ax_status" => {
                let stats = ax.get_stats().await.map_err(|e| e.to_string())?;
                let last = ax.get_last_indexed_at().await.map_err(|e| e.to_string())?;
                let pending = ax.get_pending_files().await;
                Ok(json!({ "stats": stats, "lastIndexedAt": last, "pendingFiles": pending }))
            }
            "ax_index" => {
                let result = ax.sync(IndexOptions::default(), None).await.map_err(|e| e.to_string())?;
                Ok(json!({ "filesIndexed": result.files_indexed, "durationMs": result.duration_ms }))
            }
            "ax_context" => {
                let task = params.get("task").and_then(|v| v.as_str()).unwrap_or("");
                let ctx = ax.build_context(TaskInput::Text(task.to_string()), BuildContextOptions::default()).await.map_err(|e| e.to_string())?;
                Ok(json!(ctx))
            }
            "ax_callers" => {
                let sym = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax.search_nodes(sym, &SearchOptions { limit: Some(1), ..Default::default() }).await.map_err(|e| e.to_string())?;
                if let Some(first) = nodes.first() {
                    let callers = ax.get_callers(&first.node.id, 3).await.map_err(|e| e.to_string())?;
                    Ok(json!({ "callers": callers }))
                } else {
                    Ok(json!({ "callers": [] }))
                }
            }
            "ax_callees" => {
                let sym = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax.search_nodes(sym, &SearchOptions { limit: Some(1), ..Default::default() }).await.map_err(|e| e.to_string())?;
                if let Some(first) = nodes.first() {
                    let callees = ax.get_callees(&first.node.id, 3).await.map_err(|e| e.to_string())?;
                    Ok(json!({ "callees": callees }))
                } else {
                    Ok(json!({ "callees": [] }))
                }
            }
            "ax_impact" => {
                let sym = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax.search_nodes(sym, &SearchOptions { limit: Some(1), ..Default::default() }).await.map_err(|e| e.to_string())?;
                if let Some(first) = nodes.first() {
                    let sg = ax.get_impact_radius(&first.node.id, 3).await.map_err(|e| e.to_string())?;
                    Ok(json!(sg))
                } else {
                    Ok(json!({}))
                }
            }
            "ax_files" => {
                let files = ax.queries().get_all_files().await.map_err(|e| e.to_string())?;
                Ok(json!({ "files": files }))
            }
            "ax_node" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax.search_nodes(name, &SearchOptions { limit: Some(5), ..Default::default() }).await.map_err(|e| e.to_string())?;
                Ok(json!({ "nodes": nodes }))
            }
            "ax_affected" => {
                let files: Vec<String> = params
                    .get("files")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let affected = ax.get_affected_files(&files).await.map_err(|e| e.to_string())?;
                Ok(json!({ "affected": affected }))
            }
            _ => Err(format!("unknown tool: {}", name)),
        }
    }
}

async fn explore(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let opts = explore_opts_from_params(&params);
    let result = ax.explore(query, opts).await.map_err(|e| e.to_string())?;
    let raw = format_explore_text(&result);
    let text = maybe_synthesize_explore(query, &raw).await;
    Ok(json!({
        "text": text,
        "query": result.query,
        "summary": result.summary,
        "blastRadius": result.blast_radius,
        "entries": result.entries,
    }))
}

async fn preflight(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let prompt = params.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let files = string_array(params.get("files"));
    let input = MatchInput {
        prompt,
        cwd: ax.project_root().to_path_buf(),
        open_files: files.iter().map(PathBuf::from).collect(),
        changed_files: vec![],
    };
    let result = ax.match_policy(input).await.map_err(|e| e.to_string())?;
    Ok(json!({
        "rules": result.rules,
        "skills": result.skills,
        "inject": result.inject,
        "instruction": "Apply CRITICAL rules before editing. If a skill matched, follow its workflow.",
    }))
}

async fn rules(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let prompt = params.get("prompt").and_then(|v| v.as_str()).map(String::from);
    if let Some(p) = prompt {
        let files = string_array(params.get("files"));
        let input = MatchInput {
            prompt: p,
            cwd: ax.project_root().to_path_buf(),
            open_files: files.iter().map(PathBuf::from).collect(),
            changed_files: vec![],
        };
        let result = ax.match_policy(input).await.map_err(|e| e.to_string())?;
        Ok(json!({ "rules": result.rules }))
    } else {
        let all = ax_policy::list_rules(ax.db_pool()).await.map_err(|e| e.to_string())?;
        Ok(json!({ "rules": all }))
    }
}

async fn skill(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let name = params.get("name").and_then(|v| v.as_str()).ok_or("name required")?;
    let row = ax_policy::get_skill(ax.db_pool(), name)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("skill not found: {name}"))?;
    Ok(json!(row))
}

async fn guard(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let path_str = params.get("path").and_then(|v| v.as_str()).ok_or("path required")?;
    let op = params
        .get("operation")
        .and_then(|v| v.as_str())
        .unwrap_or("write");
    let op = match op {
        "delete" => GuardOp::Delete,
        _ => GuardOp::Write,
    };
    let path = ax.project_root().join(path_str);
    let content = std::fs::read(&path).ok();
    let result = ax
        .guard_operation(&path, op, content.as_ref().map(|v| v.as_slice()))
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!(result))
}

fn string_array(v: Option<&Value>) -> Vec<String> {
    v.and_then(|a| a.as_array())
        .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

fn explore_tool() -> Value {
    json!({
        "name": "ax_explore",
        "description": "Semantic search + graph traversal with numbered source and call spine",
        "inputSchema": {
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "limit": { "type": "number" },
                "depth": { "type": "number" },
                "includeCode": { "type": "boolean" },
                "maxLinesPerSnippet": { "type": "number" },
                "maxSourceChars": { "type": "number" }
            },
            "required": ["query"]
        }
    })
}

fn preflight_tool() -> Value {
    json!({
        "name": "ax_preflight",
        "description": "Turn-start policy gate: matched rules and suggested skills for the user prompt",
        "inputSchema": {
            "type": "object",
            "properties": {
                "prompt": { "type": "string" },
                "files": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["prompt"]
        }
    })
}

fn rules_tool() -> Value {
    json!({
        "name": "ax_rules",
        "description": "List or match policy rules",
        "inputSchema": {
            "type": "object",
            "properties": {
                "prompt": { "type": "string" },
                "files": { "type": "array", "items": { "type": "string" } }
            }
        }
    })
}

fn skill_tool() -> Value {
    json!({
        "name": "ax_skill",
        "description": "Load a named skill workflow by name",
        "inputSchema": {
            "type": "object",
            "properties": { "name": { "type": "string" } },
            "required": ["name"]
        }
    })
}

fn guard_tool() -> Value {
    json!({
        "name": "ax_guard",
        "description": "Pre-write guard for CRITICAL policy rules",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "operation": { "type": "string", "enum": ["write", "delete"] }
            },
            "required": ["path"]
        }
    })
}

fn extra_tools() -> Vec<Value> {
    vec![
        json!({ "name": "ax_node", "description": "Get symbol or file details", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } } } }),
        json!({ "name": "ax_search", "description": "FTS symbol search", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] } }),
        json!({ "name": "ax_status", "description": "Index stats and staleness", "inputSchema": { "type": "object", "properties": {} } }),
        json!({ "name": "ax_index", "description": "Trigger re-index", "inputSchema": { "type": "object", "properties": {} } }),
        json!({ "name": "ax_files", "description": "Project file listing", "inputSchema": { "type": "object", "properties": {} } }),
        json!({ "name": "ax_context", "description": "Build task context", "inputSchema": { "type": "object", "properties": { "task": { "type": "string" } }, "required": ["task"] } }),
        json!({ "name": "ax_callers", "description": "Find callers", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } }, "required": ["symbol"] } }),
        json!({ "name": "ax_callees", "description": "Find callees", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } }, "required": ["symbol"] } }),
        json!({ "name": "ax_impact", "description": "Impact radius", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } }, "required": ["symbol"] } }),
        json!({ "name": "ax_affected", "description": "Affected test files", "inputSchema": { "type": "object", "properties": { "files": { "type": "array", "items": { "type": "string" } } } } }),
    ]
}

fn explore_opts_from_params(params: &Value) -> ExploreOptions {
    let mut opts = ExploreOptions::default();
    if let Some(n) = params.get("limit").and_then(|v| v.as_u64()) {
        opts.limit = Some(n as u32);
    }
    if let Some(n) = params.get("depth").and_then(|v| v.as_u64()) {
        opts.depth = Some(n as u32);
    }
    if let Some(b) = params.get("includeCode").and_then(|v| v.as_bool()) {
        opts.include_code = Some(b);
    }
    if let Some(n) = params.get("maxLinesPerSnippet").and_then(|v| v.as_u64()) {
        opts.max_lines_per_snippet = Some(n as u32);
    }
    if let Some(n) = params.get("maxSourceChars").and_then(|v| v.as_u64()) {
        opts.max_source_chars = Some(n as u32);
    }
    opts
}

pub fn server_instructions(has_policy: bool) -> String {
    let mut s = String::from(
        "You have access to ax code intelligence tools (MCP).\n\n",
    );
    if has_policy {
        s.push_str(
            "Turn start: call ax_preflight with the user prompt and open/changed files. Apply CRITICAL rules before editing.\n\
             Before Write/Delete on project files: call ax_guard when CRITICAL rules exist.\n\n",
        );
    }
    s.push_str(
        "For structural questions — how code works, call paths, impact, dependencies, architecture — call ax_explore FIRST with the user's question or symbol names. Treat returned numbered source as already read; do not re-grep the same symbols.\n\n\
         Use ax_search for quick symbol lookup. Use ax_node for one symbol's file context. Use ax_callers / ax_callees / ax_impact for focused graph queries.\n\n\
         Pass projectPath when cwd is not the indexed project root (monorepos). Prefer ax over grep/read for code structure.",
    );
    s
}
