//! MCP tools - ax_explore, ax_search, ax_status, etc.

use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;
use ax_context::format_explore_text;
use ax_reasoning::maybe_synthesize_explore;
use ax_types::{BuildContextOptions, ExploreOptions, SearchOptions, TaskInput};
use serde_json::{json, Value};

pub struct ToolHandler;

impl ToolHandler {
    pub async fn list_tools() -> Value {
        json!({
            "tools": [
                {
                    "name": "ax_explore",
                    "description": "Semantic search + graph traversal with numbered source and call spine",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": {
                                "type": "string",
                                "description": "Symbol names, filters (kind:, path:, lang:), or short search terms"
                            },
                            "limit": { "type": "number", "description": "Max entry points (default 5)" },
                            "depth": { "type": "number", "description": "Caller/callee traversal depth (default 2)" },
                            "includeCode": { "type": "boolean", "description": "Include numbered source snippets (default true)" },
                            "maxLinesPerSnippet": { "type": "number", "description": "Max lines per source snippet (default 80)" },
                            "maxSourceChars": { "type": "number", "description": "Max chars per source snippet (default 4000)" }
                        },
                        "required": ["query"]
                    }
                },
                { "name": "ax_node", "description": "Get symbol or file details", "inputSchema": { "type": "object", "properties": { "name": { "type": "string" } } } },
                { "name": "ax_search", "description": "FTS symbol search", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] } },
                { "name": "ax_status", "description": "Index stats and staleness", "inputSchema": { "type": "object", "properties": {} } },
                { "name": "ax_index", "description": "Trigger re-index", "inputSchema": { "type": "object", "properties": {} } },
                { "name": "ax_files", "description": "Project file listing", "inputSchema": { "type": "object", "properties": {} } },
                { "name": "ax_context", "description": "Build task context", "inputSchema": { "type": "object", "properties": { "task": { "type": "string" } }, "required": ["task"] } },
                { "name": "ax_callers", "description": "Find callers", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } }, "required": ["symbol"] } },
                { "name": "ax_callees", "description": "Find callees", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } }, "required": ["symbol"] } },
                { "name": "ax_impact", "description": "Impact radius", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } }, "required": ["symbol"] } },
                { "name": "ax_affected", "description": "Affected test files", "inputSchema": { "type": "object", "properties": { "files": { "type": "array", "items": { "type": "string" } } } } },
            ]
        })
    }

    pub async fn call_tool(ax: &mut Ax, name: &str, params: Value) -> Result<Value, String> {
        match name {
            "ax_explore" => {
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
                let result = ax.sync(IndexOptions::default()).await.map_err(|e| e.to_string())?;
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

pub fn server_instructions() -> String {
    r#"You have access to ax code intelligence tools (MCP).

For structural questions — how code works, call paths, impact, dependencies, architecture — call ax_explore FIRST with the user's question or symbol names. Treat returned numbered source as already read; do not re-grep the same symbols.

Use ax_search for quick symbol lookup. Use ax_node for one symbol's file context. Use ax_callers / ax_callees / ax_impact for focused graph queries.

Pass projectPath when cwd is not the indexed project root (monorepos). Prefer ax over grep/read for code structure."#
        .to_string()
}