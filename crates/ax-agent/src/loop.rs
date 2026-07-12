//! Built-in agent turn: preflight + MCP tools + LLM synthesis.

use std::path::Path;

use ax_mcp::{call_tool, format_tool_result, McpEngine};
use serde_json::json;

use crate::chat::{chunk_text, ChatRunner};

pub struct ToolEvent {
    pub name: String,
    pub output: String,
}

pub struct AgentTurnResult {
    pub answer: String,
    pub tools: Vec<ToolEvent>,
    pub preflight_inject: Option<String>,
}

fn plan_tools(prompt: &str) -> Vec<(&'static str, serde_json::Value)> {
    let lower = prompt.to_lowercase();
    let mut plan = vec![(
        "ax_preflight",
        json!({ "prompt": prompt, "open_files": [], "changed_files": [] }),
    )];

    if lower.contains("search") || lower.starts_with("find ") {
        plan.push(("ax_search", json!({ "query": prompt })));
    } else if lower.contains("status") || lower.contains("indexed") {
        plan.push(("ax_status", json!({})));
    } else {
        plan.push(("ax_explore", json!({ "query": prompt })));
    }
    plan
}

/// Run one agent turn with MCP tools, then synthesize a reply.
pub async fn run_agent_turn(project_root: &Path, prompt: &str) -> Result<AgentTurnResult, String> {
    let mut engine = McpEngine::with_project_root(project_root.to_path_buf());
    let mut tools = Vec::new();
    let mut preflight_inject = None;
    let mut context_parts = Vec::new();

    for (name, args) in plan_tools(prompt) {
        let result = call_tool(&mut engine, name, args).await?;
        let text = format_tool_result(&result);
        if name == "ax_preflight" {
            if let Some(inject) = result.get("inject").and_then(|v| v.as_str()) {
                preflight_inject = Some(inject.to_string());
            }
        }
        tools.push(ToolEvent {
            name: name.to_string(),
            output: text.clone(),
        });
        context_parts.push(format!("## Tool: {name}\n{text}"));
    }

    let context = context_parts.join("\n\n");
    let runner = ChatRunner::new("turn", "builtin");
    let answer = runner.stream_reply(prompt, &context).await?;

    Ok(AgentTurnResult {
        answer,
        tools,
        preflight_inject,
    })
}

pub fn stream_answer_chunks(text: &str) -> Vec<String> {
    chunk_text(text, 48)
}
