//! MCP tools - ax_explore, ax_search, ax_status, policy tools, etc.

use std::path::PathBuf;

use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;
use ax_context::format_explore_text;
use ax_policy::{finalize_proposal, propose_rule_from_prompt, GuardOp, MatchInput, PolicyStore, RuleFrontmatter};
use ax_reasoning::{maybe_synthesize_explore, ExploreOffloadMeta};
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
            tools.push(capture_tool());
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
            "ax_policy_capture" => policy_capture(ax, params).await,
            "ax_guard" => guard(ax, params).await,
            "ax_search" => {
                let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let results = ax.search_nodes(query, &SearchOptions { limit: Some(20), ..Default::default() }).await.map_err(|e| e.to_string())?;
                Ok(json!({ "results": results }))
            }
            "ax_status" => status(ax).await,
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
            "ax_remember" => remember(ax, params).await,
            "ax_recall" => recall(ax, params).await,
            _ => Err(format!("unknown tool: {}", name)),
        }
    }
}

async fn explore(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let opts = explore_opts_from_params(&params);
    let result = ax.explore(query, opts).await.map_err(|e| e.to_string())?;
    let raw = format_explore_text(&result);
    let project = ax
        .project_root()
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string);
    let meta = Some(ExploreOffloadMeta {
        source: "mcp_explore",
        project,
    });
    let text = maybe_synthesize_explore(query, &raw, meta).await;
    Ok(json!({
        "text": text,
        "query": result.query,
        "summary": result.summary,
        "blastRadius": result.blast_radius,
        "entries": result.entries,
    }))
}

async fn status(ax: &mut Ax) -> Result<Value, String> {
    let stats = ax.get_stats().await.map_err(|e| e.to_string())?;
    let last = ax.get_last_indexed_at().await.map_err(|e| e.to_string())?;
    let pending = ax.get_pending_files().await;
    let mut out = json!({
        "stats": stats,
        "lastIndexedAt": last,
        "pendingFiles": pending,
    });
    if ax.policy_exists() {
        let policy = ax.policy_status().await.map_err(|e| e.to_string())?;
        out["policy"] = serde_json::to_value(policy).unwrap_or(Value::Null);
    }
    Ok(out)
}

async fn preflight(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let prompt = params.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let files = string_array(params.get("files"));
    let input = MatchInput {
        prompt: prompt.clone(),
        cwd: ax.project_root().to_path_buf(),
        open_files: files.iter().map(PathBuf::from).collect(),
        changed_files: vec![],
    };
    let result = ax.match_policy(input).await.map_err(|e| e.to_string())?;
    let status = ax.policy_status().await.map_err(|e| e.to_string())?;
    let meta = ax_policy::build_preflight_meta(&status, &result);

    // Durable memories ride along with the policy inject. Failures here must
    // never break preflight — memories are additive context.
    let memories = ax_memory::recall_for_prompt(ax.db_pool(), &prompt, 3)
        .await
        .unwrap_or_default();
    let mut inject = result.inject.clone();
    if !memories.is_empty() {
        let block = ax_memory::format_memories_inject_block(&memories, 6_000);
        if !block.is_empty() {
            if !inject.is_empty() {
                inject.push('\n');
            }
            inject.push_str(&block);
        }
    }

    Ok(json!({
        "policyStatus": meta.policy_status,
        "matchedRules": meta.matched_rules,
        "matchedSkills": meta.matched_skills,
        "matchedMemories": memories.len(),
        "guardRequired": meta.guard_required,
        "mode": meta.mode,
        "rules": result.rules,
        "skills": result.skills,
        "memories": memories,
        "inject": inject,
        "instruction": "Apply CRITICAL rules before editing. If a skill matched, follow its workflow.",
    }))
}

async fn remember(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let body = params
        .get("body")
        .or_else(|| params.get("text"))
        .and_then(|v| v.as_str())
        .ok_or("body required")?
        .to_string();
    let input = ax_memory::RememberInput {
        title: params.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        body,
        kind: params.get("kind").and_then(|v| v.as_str()).map(String::from),
        tags: string_array(params.get("tags")),
        files: string_array(params.get("files")),
        source: Some("mcp".into()),
    };
    let row = ax_memory::remember(ax.db_pool(), input).await.map_err(|e| e.to_string())?;
    let similar = ax_memory::find_similar(
        ax.db_pool(),
        &format!("{} {}", row.title, row.body),
        Some(&row.id),
        0.80,
        3,
    )
    .await
    .unwrap_or_default();
    let instruction = if similar.is_empty() {
        "Memory saved. It will surface in future ax_preflight and ax_recall calls when relevant.".to_string()
    } else {
        "Memory saved, but very similar memories already exist (see similar[]). If they contradict the new memory, tell the user and consider deleting the stale one.".to_string()
    };
    Ok(json!({
        "ok": true,
        "id": row.id,
        "title": row.title,
        "kind": row.kind,
        "similar": similar,
        "instruction": instruction,
    }))
}

async fn recall(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let query = params.get("query").and_then(|v| v.as_str()).ok_or("query required")?;
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(5).min(25) as usize;
    let matches = ax_memory::recall(ax.db_pool(), query, limit).await.map_err(|e| e.to_string())?;
    let text = ax_memory::format_memories_inject_block(&matches, 12_000);
    Ok(json!({
        "matches": matches,
        "inject": text,
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

async fn policy_capture(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("propose");
    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let files = string_array(params.get("files"));

    if action == "save" {
        let rule = params.get("rule").ok_or("rule required for save action")?;
        let fm: RuleFrontmatter = serde_json::from_value(
            rule.get("frontmatter")
                .cloned()
                .ok_or("rule.frontmatter required")?,
        )
        .map_err(|e| e.to_string())?;
        let body = rule
            .get("body")
            .and_then(|v| v.as_str())
            .ok_or("rule.body required")?
            .to_string();

        let store = PolicyStore::new(ax.db_pool().clone(), ax.project_root().to_path_buf());
        let storage = store.storage();
        let doc = store.save_rule(fm.clone(), body).await.map_err(|e| e.error)?;
        let storage_label = match storage {
            ax_policy::PolicyStorage::Database => "database",
            ax_policy::PolicyStorage::Files => "files",
        };
        return Ok(json!({
            "ok": true,
            "action": "save",
            "id": doc.frontmatter.id,
            "storage": storage_label,
            "path": format!(".ax/policy/rules/{}.mdc", doc.frontmatter.id),
            "instruction": format!(
                "Rule saved to {storage_label}. It will match on future turns via ax_preflight."
            ),
        }));
    }

    let proposal = propose_rule_from_prompt(&prompt, &files);
    if !proposal.detected {
        return Ok(json!({
            "ok": false,
            "action": "propose",
            "detected": false,
            "instruction": "No directive language found. Use explicit markers (@rule, #rule) or phrases like 'je moet', 'always', 'never'.",
        }));
    }

    let existing: Vec<String> = ax_policy::list_rules(ax.db_pool())
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| r.id)
        .collect();
    let proposal = finalize_proposal(proposal, &existing);

    Ok(json!({
        "ok": true,
        "action": "propose",
        "detected": true,
        "proposal": proposal,
        "preview": proposal.preview,
        "questions": proposal.questions,
        "instruction": "Ask the user each question in questions[] before save. Apply answers to rule.frontmatter/body. Save only after explicit yes — ax_policy_capture action=save writes to ax.db in database mode.",
    }))
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
    let content = guard_content_from_params(&params).or_else(|| std::fs::read(&path).ok());
    let result = ax
        .guard_operation(&path, op, content.as_ref().map(|v| v.as_slice()))
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!(result))
}

fn guard_content_from_params(params: &Value) -> Option<Vec<u8>> {
    if let Some(b64) = params.get("contentBase64").and_then(|v| v.as_str()) {
        return base64_decode(b64);
    }
    params
        .get("content")
        .and_then(|v| v.as_str())
        .map(|s| s.as_bytes().to_vec())
}

fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 256] = &{
        let mut t = [255u8; 256];
        let mut i = 0u8;
        while i < 64 {
            let c = match i {
                0..=25 => b'A' + i,
                26..=51 => b'a' + (i - 26),
                52..=61 => b'0' + (i - 52),
                62 => b'+',
                _ => b'/',
            };
            t[c as usize] = i;
            i += 1;
        }
        t[b'=' as usize] = 0;
        t
    };
    let bytes = input.trim().as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in bytes {
        let v = TABLE[b as usize];
        if v == 255 {
            return None;
        }
        buf = (buf << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
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
        "description": "MANDATORY first tool call each turn when policy is indexed. Returns matched rules, skills, and inject block with full bodies.",
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

fn capture_tool() -> Value {
    json!({
        "name": "ax_policy_capture",
        "description": "Propose or save a policy rule from directive language in the user prompt (je moet, always, never, @rule). Propose returns interview questions (level, triggers, globs, alwaysApply, priority). Ask user each question; save only after yes — persisted to ax.db in database mode.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "prompt": { "type": "string" },
                "files": { "type": "array", "items": { "type": "string" } },
                "action": { "type": "string", "enum": ["propose", "save"] },
                "rule": {
                    "type": "object",
                    "properties": {
                        "frontmatter": { "type": "object" },
                        "body": { "type": "string" }
                    }
                }
            },
            "required": ["prompt"]
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
                "operation": { "type": "string", "enum": ["write", "delete"] },
                "content": { "type": "string", "description": "Proposed file content for new files (UTF-8 check)" },
                "contentBase64": { "type": "string", "description": "Base64-encoded proposed content" }
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
        json!({
            "name": "ax_remember",
            "description": "Store a durable project memory (decision, bug fix, architecture choice, convention). Recalled automatically in future ax_preflight calls and searchable via ax_recall.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "body": { "type": "string", "description": "The memory content — what to remember and why" },
                    "title": { "type": "string", "description": "Short title (defaults to first line of body)" },
                    "kind": { "type": "string", "enum": ["decision", "bug_fix", "architecture", "convention", "note"] },
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "files": { "type": "array", "items": { "type": "string" }, "description": "Related project-relative file paths" }
                },
                "required": ["body"]
            }
        }),
        json!({
            "name": "ax_recall",
            "description": "Search durable project memories (decisions, fixes, conventions) by free text. Fresh memories outrank stale ones via confidence decay.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "limit": { "type": "number" }
                },
                "required": ["query"]
            }
        }),
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
             Do not Read or Grep .ax/policy/ on disk — policy is delivered in ax_preflight inject only.\n\
             If you have not called ax_preflight this turn, call it now before any other work.\n\
             Before Write/Delete on project files: call ax_guard when CRITICAL rules exist.\n\
             When the user gives durable directives (je moet, always, never, @rule): call ax_policy_capture(propose), show preview, save only after explicit yes.\n\n",
        );
    }
    s.push_str(
        "For structural questions — how code works, call paths, impact, dependencies, architecture — call ax_explore FIRST with the user's question or symbol names. Treat returned numbered source as already read; do not re-grep the same symbols.\n\n\
         Use ax_search for quick symbol lookup. Use ax_node for one symbol's file context. Use ax_callers / ax_callees / ax_impact for focused graph queries.\n\n\
         Memory vault: when you make a durable decision, fix a tricky bug, or establish a convention, store it with ax_remember. Use ax_recall to search past decisions before re-deriving them. Relevant memories are auto-injected via ax_preflight.\n\n\
         Pass projectPath when cwd is not the indexed project root (monorepos). Prefer ax over grep/read for code structure.",
    );
    s
}
