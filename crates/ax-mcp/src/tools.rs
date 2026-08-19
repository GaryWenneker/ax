//! MCP tools - ax_explore, ax_search, ax_status, policy tools, etc.

use std::path::PathBuf;

use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;
use ax_context::directory::{find_nearest_ax_root, is_initialized};
use ax_context::{format_context_as_markdown, format_explore_text};
use ax_policy::{
    detect_directive, finalize_proposal, propose_rule_from_prompt, GuardOp, MatchInput, MatchResult,
    PolicyStatus, PolicyStore, RuleFrontmatter,
};
use ax_reasoning::{maybe_synthesize_explore, ExploreOffloadMeta};
use ax_types::{BuildContextOptions, ExploreOptions, Node, SearchOptions, SearchResult, Subgraph, TaskInput};
use serde_json::{json, Value};

pub struct ToolHandler;

impl ToolHandler {
    pub async fn list_tools(project_has_policy: bool) -> Value {
        let mut tools = vec![explore_tool()];
        // ax_preflight + ax_policy_capture are ALWAYS advertised, even when the
        // project has no policy yet. Otherwise directive capture can never
        // bootstrap: the tool that would create the first rule is hidden until
        // a rule already exists (chicken-and-egg). The first capture save
        // creates the policy store. Tools that only make sense with existing
        // policy (rules/skill/guard) stay gated.
        tools.push(preflight_tool());
        tools.push(capture_tool());
        if project_has_policy {
            tools.push(rules_tool());
            tools.push(skill_tool());
            tools.push(guard_tool());
        }
        tools.extend(extra_tools());
        // Lean default: core tools only. Extras via AX_MCP_TOOLS=all|name,name.
        // Unlisted tools remain callable (call_tool is not filtered).
        crate::tool_filter::filter_tools_list(&mut tools);
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
            "ax_diagnostics" => diagnostics(ax, params).await,
            "ax_search" => {
                let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
                let results = ax.search_nodes(query, &SearchOptions { limit: Some(20), ..Default::default() }).await.map_err(|e| e.to_string())?;
                let text = format_search_results_text(&format!("Search: {query}"), &results);
                Ok(json!({ "text": text, "results": results }))
            }
            "ax_status" => status(ax).await,
            "ax_index" => index_tool(ax, params).await,
            "ax_sync" => sync_tool(ax).await,
            "ax_lsp" => lsp_tool(ax, params).await,
            "ax_ship" => ship_tool(ax, params).await,
            "ax_policy_index" => policy_index_tool(ax, params).await,
            "ax_context" => {
                let task = params.get("task").and_then(|v| v.as_str()).unwrap_or("");
                let ctx = ax.build_context(TaskInput::Text(task.to_string()), BuildContextOptions::default()).await.map_err(|e| e.to_string())?;
                let text = format_context_as_markdown(&ctx);
                let mut value = serde_json::to_value(&ctx).map_err(|e| e.to_string())?;
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("text".to_string(), Value::String(text));
                }
                Ok(value)
            }
            "ax_callers" => {
                let sym = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax.search_nodes(sym, &SearchOptions { limit: Some(1), ..Default::default() }).await.map_err(|e| e.to_string())?;
                if let Some(first) = nodes.first() {
                    let callers = ax.get_callers(&first.node.id, 3).await.map_err(|e| e.to_string())?;
                    let text = format_nodes_text(&format!("Callers of '{sym}'"), &callers);
                    Ok(json!({ "text": text, "callers": callers }))
                } else {
                    Ok(json!({ "text": format!("No symbol matching '{sym}'"), "callers": [] }))
                }
            }
            "ax_callees" => {
                let sym = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax.search_nodes(sym, &SearchOptions { limit: Some(1), ..Default::default() }).await.map_err(|e| e.to_string())?;
                if let Some(first) = nodes.first() {
                    let callees = ax.get_callees(&first.node.id, 3).await.map_err(|e| e.to_string())?;
                    let text = format_nodes_text(&format!("Callees of '{sym}'"), &callees);
                    Ok(json!({ "text": text, "callees": callees }))
                } else {
                    Ok(json!({ "text": format!("No symbol matching '{sym}'"), "callees": [] }))
                }
            }
            "ax_impact" => {
                let sym = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax.search_nodes(sym, &SearchOptions { limit: Some(1), ..Default::default() }).await.map_err(|e| e.to_string())?;
                if let Some(first) = nodes.first() {
                    let sg = ax.get_impact_radius(&first.node.id, 3).await.map_err(|e| e.to_string())?;
                    let text = format_subgraph_text(sym, &sg);
                    let mut value = serde_json::to_value(&sg).map_err(|e| e.to_string())?;
                    if let Some(obj) = value.as_object_mut() {
                        obj.insert("text".to_string(), Value::String(text));
                    }
                    Ok(value)
                } else {
                    Ok(json!({ "text": format!("No symbol matching '{sym}'") }))
                }
            }
            "ax_cycles" => {
                let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
                let cycles = ax.find_cycles(limit).await.map_err(|e| e.to_string())?;
                let text = if cycles.is_empty() {
                    "No call-graph cycles found.".to_string()
                } else {
                    let mut s = format!("## Call-graph cycles ({})\n\n", cycles.len());
                    for (i, c) in cycles.iter().enumerate() {
                        let mut names = Vec::with_capacity(c.nodes.len());
                        for id in &c.nodes {
                            let name = ax
                                .get_node(id)
                                .await
                                .ok()
                                .flatten()
                                .map(|n| n.qualified_name)
                                .unwrap_or_else(|| id.clone());
                            names.push(name);
                        }
                        s.push_str(&format!("{}. {}\n", i + 1, names.join(" → ")));
                    }
                    s
                };
                Ok(json!({ "text": text, "cycles": cycles }))
            }
            "ax_path" => {
                let from = params.get("from").and_then(|v| v.as_str()).unwrap_or("");
                let to = params.get("to").and_then(|v| v.as_str()).unwrap_or("");
                if from.is_empty() || to.is_empty() {
                    return Err("from and to are required".into());
                }
                let from_hits = ax
                    .search_nodes(from, &SearchOptions { limit: Some(1), ..Default::default() })
                    .await
                    .map_err(|e| e.to_string())?;
                let to_hits = ax
                    .search_nodes(to, &SearchOptions { limit: Some(1), ..Default::default() })
                    .await
                    .map_err(|e| e.to_string())?;
                let (Some(f), Some(t)) = (from_hits.first(), to_hits.first()) else {
                    return Ok(json!({
                        "text": format!("Could not resolve symbols '{from}' / '{to}'"),
                        "path": Value::Null
                    }));
                };
                let path = ax
                    .find_path(&f.node.id, &t.node.id)
                    .await
                    .map_err(|e| e.to_string())?;
                let text = match &path {
                    Some(ids) if !ids.is_empty() => format!("Path: {}", ids.join(" → ")),
                    _ => format!(
                        "No Calls/References path from '{}' to '{}'.",
                        f.node.qualified_name, t.node.qualified_name
                    ),
                };
                Ok(json!({
                    "text": text,
                    "from": f.node.qualified_name,
                    "to": t.node.qualified_name,
                    "path": path
                }))
            }
            "ax_api" => {
                let module = params.get("module").and_then(|v| v.as_str()).unwrap_or("");
                if module.is_empty() {
                    return Err("module is required".into());
                }
                let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
                let nodes = ax.module_api(module, limit).await.map_err(|e| e.to_string())?;
                let text = if nodes.is_empty() {
                    format!("No exported symbols matching module '{module}'.")
                } else {
                    let mut s = format!("## API surface: {module} ({} symbols)\n\n", nodes.len());
                    for n in &nodes {
                        s.push_str(&format!("- {} — {}\n", n.qualified_name, n.file_path));
                    }
                    s
                };
                Ok(json!({ "text": text, "module": module, "symbols": nodes }))
            }
            "ax_files" => {
                let files = ax.queries().get_all_files().await.map_err(|e| e.to_string())?;
                Ok(json!({ "files": files }))
            }
            "ax_node" => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax.search_nodes(name, &SearchOptions { limit: Some(5), ..Default::default() }).await.map_err(|e| e.to_string())?;
                let text = format_search_results_text(&format!("Symbol(s) for '{name}'"), &nodes);
                Ok(json!({ "text": text, "nodes": nodes }))
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
            "ax_insights" => insights(ax, params).await,
            "ax_report" => report(ax, params).await,
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
    let text = ax_core::stats_format::format_status_text(&stats, last, &pending);
    let mut out = json!({
        "text": text,
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

async fn sync_tool(ax: &mut Ax) -> Result<Value, String> {
    let root = ax.project_root().to_path_buf();
    ax_usage::log_workspace(Some(&root), "sync start via=mcp");
    let result = match ax.sync(IndexOptions::default(), None).await {
        Ok(r) => r,
        Err(e) => {
            ax_usage::log_workspace(Some(&root), "sync fail via=mcp");
            return Err(e.to_string());
        }
    };
    ax_usage::log_workspace(
        Some(&root),
        format!(
            "sync ok files={} duration_ms={} via=mcp",
            result.files_indexed, result.duration_ms
        ),
    );
    Ok(json!({
        "text": format!(
            "Synced {} file(s) in {}ms",
            result.files_indexed, result.duration_ms
        ),
        "filesIndexed": result.files_indexed,
        "durationMs": result.duration_ms,
    }))
}

async fn index_tool(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let force = params.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
    let root = ax.project_root().to_path_buf();
    if !force {
        return sync_tool(ax).await;
    }
    ax_usage::log_workspace(Some(&root), "index start force=1 via=mcp");
    if let Err(e) = ax.clear().await {
        ax_usage::log_workspace(Some(&root), "index fail stage=clear via=mcp");
        return Err(e.to_string());
    }
    let opts = IndexOptions {
        force: true,
        quiet: true,
        ..IndexOptions::default()
    };
    let result = match ax.index_all(opts, None).await {
        Ok(r) => r,
        Err(e) => {
            ax_usage::log_workspace(Some(&root), "index fail stage=index via=mcp");
            return Err(e.to_string());
        }
    };
    ax_usage::log_workspace(
        Some(&root),
        format!(
            "index ok files={} duration_ms={} force=1 via=mcp",
            result.files_indexed, result.duration_ms
        ),
    );
    Ok(json!({
        "text": format!(
            "Indexed {} files in {}ms (force)",
            result.files_indexed, result.duration_ms
        ),
        "filesIndexed": result.files_indexed,
        "durationMs": result.duration_ms,
        "force": true,
    }))
}

async fn lsp_tool(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let action = params
        .get("action")
        .and_then(|v| v.as_str())
        .unwrap_or("status");
    let root = ax.project_root().to_path_buf();
    match action {
        "status" => {
            let servers = ax_lsp::discover_servers();
            let text = {
                let mut lines = vec!["LSP servers:".to_string()];
                for s in &servers {
                    let mark = if s.available { "ok" } else { "--" };
                    let path = s.path.as_deref().unwrap_or("(not found)");
                    lines.push(format!("  [{mark}] {} {path}", s.id));
                }
                lines.join("\n")
            };
            Ok(json!({ "text": text, "servers": servers, "action": "status" }))
        }
        "enrich" => {
            let limit = params
                .get("limit")
                .and_then(|v| v.as_u64())
                .unwrap_or(200)
                .min(5_000) as usize;
            ax_usage::log_lsp(Some(&root), format!("enrich start limit={limit} via=mcp"));
            let report = match ax_lsp::enrich_project(&root, ax.queries(), limit).await {
                Ok(r) => r,
                Err(e) => {
                    ax_usage::log_lsp(Some(&root), format!("enrich fail via=mcp"));
                    return Err(e.to_string());
                }
            };
            ax_usage::log_lsp(
                Some(&root),
                format!(
                    "enrich examined={} resolved={} no_server={} no_def={} errors={} via=mcp",
                    report.examined,
                    report.resolved,
                    report.skipped_no_server,
                    report.skipped_no_definition,
                    report.errors.len()
                ),
            );
            Ok(json!({
                "text": format!(
                    "LSP enrich: examined={} resolved={} errors={}",
                    report.examined,
                    report.resolved,
                    report.errors.len()
                ),
                "action": "enrich",
                "report": report,
            }))
        }
        other => Err(format!("ax_lsp action must be status or enrich, got {other}")),
    }
}

async fn ship_tool(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let mode = params
        .get("mode")
        .and_then(|v| v.as_str())
        .unwrap_or("evaluate");
    if mode != "ci" && mode != "evaluate" {
        return Err(format!("ax_ship mode must be ci or evaluate, got {mode}"));
    }
    let root = ax.project_root().to_path_buf();
    if mode == "ci" {
        ax_usage::log_ship_ci(Some(&root), "start via=mcp");
    } else {
        ax_usage::log_ship(Some(&root), "start mode=evaluate via=mcp");
    }
    let report = match ax_ship::evaluate_project(root.clone()).await {
        Ok(r) => r,
        Err(e) => {
            if mode == "ci" {
                ax_usage::log_ship_ci(Some(&root), "fail stage=evaluate via=mcp");
            } else {
                ax_usage::log_ship(Some(&root), "fail mode=evaluate via=mcp");
            }
            return Err(e);
        }
    };
    let passed = report.quality_gate.passed;
    let status = if passed { "passed" } else { "failed" };
    if mode == "ci" {
        ax_usage::log_ship_ci(
            Some(&root),
            format!(
                "status={status} steps={} via=mcp",
                report.quality_gate.steps.len()
            ),
        );
    } else {
        ax_usage::log_ship(
            Some(&root),
            format!(
                "ok mode=evaluate passed={} via=mcp",
                if passed { "1" } else { "0" }
            ),
        );
    }
    let text = format!(
        "ax ship --{mode}: status={status} steps={}",
        report.quality_gate.steps.len()
    );
    // Soft error for CI gate failure — never process::exit from MCP.
    let is_error = mode == "ci" && !passed;
    Ok(json!({
        "text": text,
        "mode": mode,
        "passed": passed,
        "isError": is_error,
        "report": report,
    }))
}

async fn policy_index_tool(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let force = params.get("force").and_then(|v| v.as_bool()).unwrap_or(false);
    let root = ax.project_root().to_path_buf();
    ax_usage::log_policy(
        Some(&root),
        format!(
            "index start force={} via=mcp",
            if force { "1" } else { "0" }
        ),
    );
    let result = match ax.index_policy(force).await {
        Ok(r) => r,
        Err(e) => {
            ax_usage::log_policy(Some(&root), "index fail via=mcp");
            return Err(e.to_string());
        }
    };
    ax_usage::log_policy(
        Some(&root),
        format!(
            "index ok rules={} skills={} via=mcp",
            result.rules_indexed, result.skills_indexed
        ),
    );
    Ok(json!({
        "text": format!(
            "Policy indexed: {} rules, {} skills (force={force})",
            result.rules_indexed, result.skills_indexed
        ),
        "rulesIndexed": result.rules_indexed,
        "skillsIndexed": result.skills_indexed,
        "force": force,
    }))
}

fn resolve_preflight_cwd(ax: &Ax, params: &Value) -> PathBuf {
    let override_path = params
        .get("projectPath")
        .or_else(|| params.get("project_path"))
        .and_then(|v| v.as_str())
        .map(PathBuf::from);
    if let Some(p) = override_path {
        if let Some(root) = find_nearest_ax_root(&p) {
            return root;
        }
        if is_initialized(&p) {
            return p;
        }
    }
    ax.project_root().to_path_buf()
}

async fn preflight(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let prompt = params.get("prompt").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let files = string_array(params.get("files"));
    let cwd = resolve_preflight_cwd(ax, &params);
    let input = MatchInput {
        prompt: prompt.clone(),
        cwd,
        open_files: files.iter().map(PathBuf::from).collect(),
        changed_files: vec![],
    };
    // Preflight must always return a payload. Policy match/status failures
    // degrade to an empty inject + error note — never MCP isError.
    let mut policy_error: Option<String> = None;
    let result = match ax.match_policy(input).await {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            policy_error = Some(msg.clone());
            crate::verbose::push_line(format!("enrich policy match failed: {msg}"));
            MatchResult {
                rules: vec![],
                skills: vec![],
                inject: format!(
                    "<ax_policy note=\"preflight degraded: policy match failed — {msg}\">\n</ax_policy>\n"
                ),
            }
        }
    };
    let status = match ax.policy_status().await {
        Ok(s) => s,
        Err(e) => {
            let msg = e.to_string();
            crate::verbose::push_line(format!("enrich policy status failed: {msg}"));
            if policy_error.is_none() {
                policy_error = Some(msg);
            }
            PolicyStatus {
                indexed: false,
                rules: 0,
                skills: 0,
                mode: "degraded".into(),
            }
        }
    };
    let meta = ax_policy::build_preflight_meta(&status, &result);
    crate::verbose::push_line(format!(
        "enrich policy matched_rules={} matched_skills={} inject_chars={} mode={}",
        meta.matched_rules,
        meta.matched_skills,
        result.inject.len(),
        meta.mode
    ));

    // Index snapshot rides along with policy inject — failures must never
    // break preflight; stats are additive context.
    let index_stats = ax.get_stats().await.ok();
    let pending = ax.get_pending_files().await;

    // Durable memories ride along with the policy inject. Failures here must
    // never break preflight — memories are additive context.
    let memories = ax_memory::recall_for_prompt(ax.db_pool(), &prompt, 3)
        .await
        .unwrap_or_default();
    let mut inject = result.inject.clone();
    if let Some(ref stats) = index_stats {
        let block = ax_core::stats_format::format_index_inject_block(stats, &pending);
        if !block.is_empty() {
            if !inject.is_empty() {
                inject.push('\n');
            }
            inject.push_str(&block);
            crate::verbose::push_line(format!(
                "enrich index block_chars={} pending_files={}",
                block.len(),
                pending.len()
            ));
        } else {
            crate::verbose::push_line("enrich index skipped (empty block)");
        }
    } else {
        crate::verbose::push_line("enrich index skipped (stats unavailable)");
    }
    if !memories.is_empty() {
        let block = ax_memory::format_memories_inject_block(&memories, 3_000);
        if !block.is_empty() {
            if !inject.is_empty() {
                inject.push('\n');
            }
            inject.push_str(&block);
            crate::verbose::push_line(format!(
                "enrich memories count={} block_chars={}",
                memories.len(),
                block.len()
            ));
        }
    } else {
        crate::verbose::push_line("enrich memories none");
    }

    let has_directive = detect_directive(&prompt);

    // When the prompt carries a durable directive, build the rule proposal
    // right here so the agent has it in hand (no extra round-trip) and can go
    // straight to the confirm-then-save flow.
    let capture_proposal = if has_directive {
        let proposal = propose_rule_from_prompt(&prompt, &files);
        let existing: Vec<String> = ax_policy::list_rules(ax.db_pool())
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.id)
            .collect();
        Some(finalize_proposal(proposal, &existing))
    } else {
        None
    };

    if has_directive && !inject.is_empty() {
        inject.push('\n');
    }
    if has_directive {
        inject.push_str("<ax_capture_hint>Directive detected — a ready rule proposal is in captureProposal. Ask the user captureProposal.questions, then call ax_policy_capture({ action: \"save\", rule }) after they confirm.</ax_capture_hint>");
        crate::verbose::push_line("enrich directive capture_hint appended");
    } else {
        crate::verbose::push_line("enrich directive none");
    }

    let mut instruction = "Apply CRITICAL rules before editing. If a skill matched, follow its workflow.".to_string();
    if has_directive {
        instruction.push_str(" DIRECTIVE DETECTED — captureProposal holds a ready rule. Ask the user the questions in captureProposal.questions, then call ax_policy_capture(action=\"save\", rule) after they say yes. This persists even in a project with no prior policy.");
    }

    crate::verbose::push_line(format!(
        "enrich done final_inject_chars={} directive={}",
        inject.len(),
        has_directive
    ));

    let mut out = json!({
        "policyStatus": meta.policy_status,
        "matchedRules": meta.matched_rules,
        "matchedSkills": meta.matched_skills,
        "matchedMemories": memories.len(),
        "guardRequired": meta.guard_required,
        "mode": meta.mode,
        "directiveDetected": has_directive,
        "captureProposal": capture_proposal,
        "rules": result.rules,
        "skills": result.skills,
        "memories": memories,
        "indexStats": index_stats,
        "pendingFiles": pending,
        "inject": inject,
        "instruction": instruction,
    });
    if let Some(err) = policy_error {
        if let Some(obj) = out.as_object_mut() {
            obj.insert("policyError".to_string(), Value::String(err));
        }
    }
    Ok(out)
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

async fn insights(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let resolution = params
        .get("resolution")
        .and_then(|v| v.as_f64())
        .filter(|r| *r > 0.0)
        .unwrap_or(1.0);
    let god_limit = params
        .get("godLimit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .clamp(1, 200) as usize;
    let surprising_limit = params
        .get("surprisingLimit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .clamp(1, 200) as usize;
    let insights = ax
        .insights(resolution, god_limit, surprising_limit)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!(insights))
}

async fn report(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let resolution = params
        .get("resolution")
        .and_then(|v| v.as_f64())
        .filter(|r| *r > 0.0)
        .unwrap_or(1.0);
    let markdown = ax
        .architecture_report(resolution)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({ "markdown": markdown }))
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
    let paths = guard_paths_from_params(&params)?;
    let op = guard_op_from_params(&params);
    if paths.len() == 1 {
        let path_str = &paths[0];
        let path = ax.project_root().join(path_str);
        let content = guard_content_from_params(&params).or_else(|| std::fs::read(&path).ok());
        let result = ax
            .guard_operation(&path, op, content.as_ref().map(|v| v.as_slice()))
            .await
            .map_err(|e| e.to_string())?;
        return Ok(json!(result));
    }
    let mut all_allowed = true;
    let mut violations = Vec::new();
    let mut results = Vec::new();
    for path_str in &paths {
        let path = ax.project_root().join(path_str);
        let content = guard_content_from_params(&params).or_else(|| std::fs::read(&path).ok());
        let result = ax
            .guard_operation(&path, op, content.as_ref().map(|v| v.as_slice()))
            .await
            .map_err(|e| e.to_string())?;
        if !result.allowed {
            all_allowed = false;
        }
        for v in &result.violations {
            violations.push(json!({
                "path": path_str,
                "ruleId": v.rule_id,
                "message": v.message,
            }));
        }
        results.push(json!({
            "path": path_str,
            "allowed": result.allowed,
            "violations": result.violations,
        }));
    }
    Ok(json!({
        "allowed": all_allowed,
        "violations": violations,
        "results": results,
    }))
}

/// Diagnostics bridge — the agent gathers LSP/linter/compiler diagnostics from its
/// own host (Cursor's Problems panel, `tsc`, `ruff`, …) and feeds them in here.
/// ax has no way to read editor/IDE state itself, so this tool exists to receive
/// that state and correlate it with graph data ax *does* own: which CRITICAL-guarded
/// paths are affected, and which tests the graph says are impacted by those files.
async fn diagnostics(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let entries = diagnostics_from_params(&params);
    if entries.is_empty() {
        return Ok(json!({
            "text": "No diagnostics provided — pass diagnostics: [{ path, line?, severity?, message, source? }].",
            "files": [],
        }));
    }

    let mut by_path: std::collections::BTreeMap<String, Vec<&DiagnosticEntry>> = std::collections::BTreeMap::new();
    for d in &entries {
        by_path.entry(d.path.clone()).or_default().push(d);
    }

    let error_count = entries.iter().filter(|d| d.severity == "error").count();
    let warning_count = entries.iter().filter(|d| d.severity == "warning").count();

    let paths: Vec<String> = by_path.keys().cloned().collect();
    let affected = ax.get_affected_files(&paths).await.unwrap_or_default();

    let mut guarded_paths = Vec::new();
    for path_str in &paths {
        let abs = ax.project_root().join(path_str);
        let content = std::fs::read(&abs).ok();
        if let Ok(result) = ax
            .guard_operation(&abs, GuardOp::Write, content.as_deref())
            .await
        {
            if !result.allowed {
                guarded_paths.push(json!({
                    "path": path_str,
                    "violations": result.violations,
                }));
            }
        }
    }

    let mut lines = vec![format!(
        "# Diagnostics: {} error(s), {} warning(s) across {} file(s)",
        error_count,
        warning_count,
        by_path.len()
    )];
    for (path, ds) in &by_path {
        lines.push(format!("\n## {path}"));
        for d in ds.iter() {
            let loc = match d.line {
                Some(l) => format!(":{l}"),
                None => String::new(),
            };
            let src = d
                .source
                .as_ref()
                .map(|s| format!(" ({s})"))
                .unwrap_or_default();
            lines.push(format!("- [{}]{loc} {}{src}", d.severity, d.message));
        }
    }
    if !guarded_paths.is_empty() {
        lines.push("\n## CRITICAL policy overlap".to_string());
        lines.push("These diagnostic files are also guarded by CRITICAL rules — check ax_guard before writing a fix:".to_string());
        for g in &guarded_paths {
            lines.push(format!("- {}", g["path"]));
        }
    }
    if !affected.is_empty() {
        lines.push("\n## Affected tests (run after fixing)".to_string());
        for t in &affected {
            lines.push(format!("- {t}"));
        }
    }

    Ok(json!({
        "text": lines.join("\n"),
        "errorCount": error_count,
        "warningCount": warning_count,
        "files": paths,
        "guardedPaths": guarded_paths,
        "affectedTests": affected,
    }))
}

struct DiagnosticEntry {
    path: String,
    line: Option<u64>,
    severity: String,
    message: String,
    source: Option<String>,
}

fn diagnostics_from_params(params: &Value) -> Vec<DiagnosticEntry> {
    params
        .get("diagnostics")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|d| {
                    let path = d
                        .get("path")
                        .or_else(|| d.get("file"))
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())?
                        .to_string();
                    let message = d
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("(no message)")
                        .to_string();
                    let severity = d
                        .get("severity")
                        .and_then(|v| v.as_str())
                        .unwrap_or("error")
                        .to_ascii_lowercase();
                    let line = d.get("line").and_then(|v| v.as_u64());
                    let source = d
                        .get("source")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    Some(DiagnosticEntry {
                        path,
                        line,
                        severity,
                        message,
                        source,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Accept `path`, `file`/`filepath`, or a non-empty `paths` array (common agent mistakes).
fn guard_paths_from_params(params: &Value) -> Result<Vec<String>, String> {
    if let Some(p) = params
        .get("path")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(vec![p.to_string()]);
    }
    for key in ["file", "filepath", "filePath"] {
        if let Some(p) = params
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Ok(vec![p.to_string()]);
        }
    }
    if let Some(arr) = params.get("paths").and_then(|v| v.as_array()) {
        let paths: Vec<String> = arr
            .iter()
            .filter_map(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
        if !paths.is_empty() {
            return Ok(paths);
        }
    }
    Err("path required".into())
}

fn guard_op_from_params(params: &Value) -> GuardOp {
    let op = params
        .get("operation")
        .or_else(|| params.get("action"))
        .and_then(|v| v.as_str())
        .unwrap_or("write")
        .trim()
        .to_ascii_lowercase();
    match op.as_str() {
        "delete" | "unlink" | "remove" => GuardOp::Delete,
        _ => GuardOp::Write, // write / edit / create / update / …
    }
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

/// One compact line for a node: `qualifiedName — file:start-end (Kind)`.
fn node_line(n: &Node) -> String {
    format!(
        "- {} — {}:{}-{} ({:?})",
        n.qualified_name, n.file_path, n.start_line, n.end_line, n.kind
    )
}

/// Compact node list — far cheaper than serializing full `Node` JSON per hit.
fn format_nodes_text(header: &str, nodes: &[Node]) -> String {
    if nodes.is_empty() {
        return format!("{header}\n(none)");
    }
    let mut out = format!("{header} ({})\n", nodes.len());
    for n in nodes {
        out.push_str(&node_line(n));
        out.push('\n');
    }
    out
}

/// Compact search-result list (node + relevance score).
fn format_search_results_text(header: &str, results: &[SearchResult]) -> String {
    if results.is_empty() {
        return format!("{header}\n(no matches)");
    }
    let mut out = format!("{header} ({})\n", results.len());
    for r in results {
        let n = &r.node;
        out.push_str(&format!(
            "- {} — {}:{}-{} ({:?}) [score {:.2}]\n",
            n.qualified_name, n.file_path, n.start_line, n.end_line, n.kind, r.score
        ));
    }
    out
}

/// Compact impact summary: counts plus a stable (path-sorted) node list.
fn format_subgraph_text(sym: &str, sg: &Subgraph) -> String {
    let mut out = format!(
        "Impact radius for '{sym}': {} node(s), {} edge(s)\n",
        sg.nodes.len(),
        sg.edges.len()
    );
    let mut nodes: Vec<&Node> = sg.nodes.values().collect();
    nodes.sort_by(|a, b| {
        a.file_path
            .cmp(&b.file_path)
            .then(a.start_line.cmp(&b.start_line))
    });
    for n in nodes {
        out.push_str(&node_line(n));
        out.push('\n');
    }
    out
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
        "description": "MANDATORY first tool call each turn — never skip. Returns matched rules, skills, memories, and inject block. Detects durable directives (je moet/altijd/nooit/always/never/must/@rule): sets directiveDetected=true and returns a ready captureProposal (rule + interview questions) — ask the questions, then ax_policy_capture(action=save) after the user confirms. Works even with no existing policy. Pass the full user prompt.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "prompt": { "type": "string" },
                "files": { "type": "array", "items": { "type": "string" } },
                "projectPath": {
                    "type": "string",
                    "description": "Optional project root when cwd differs from the MCP --path index (monorepos). Resolves to the nearest ax root."
                }
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
        "description": "Pre-write guard for CRITICAL policy rules. Prefer path+operation; also accepts paths[] and action=edit|write|delete. Checks built-in encoding/secrets rules, plus any CRITICAL rule whose body contains a `guard: forbid-path: \"<glob>\"`, `guard: forbid-content: \"<substring or /regex/>\"`, `guard: require-content: \"<substring or /regex/>\"` (scoped by that rule's globs), or `guard: require-skill: \"<name>\"` (skill must exist, be approved, and have alwaysApply) directive line.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Relative file path (preferred)" },
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Alternate to path — guard each path"
                },
                "file": { "type": "string", "description": "Alias for path" },
                "operation": { "type": "string", "enum": ["write", "delete"] },
                "action": {
                    "type": "string",
                    "description": "Alias for operation (edit/write → write, delete → delete)"
                },
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
        json!({
            "name": "ax_index",
            "description": "Re-index the project. Default is incremental sync (same as ax_sync). Pass force=true to clear and full-index.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "force": { "type": "boolean", "description": "When true, clear the index and rebuild from scratch" }
                }
            }
        }),
        json!({
            "name": "ax_sync",
            "description": "Incremental index sync (changed files only). Prefer this after local edits; use ax_index with force=true for a full rebuild.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "ax_lsp",
            "description": "Language server bridge: action=status lists discovered LSP servers; action=enrich resolves Exact edges via language servers (same as ax lsp enrich).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["status", "enrich"], "description": "Default status" },
                    "limit": { "type": "number", "description": "Max unresolved refs to enrich (enrich only, default 200)" }
                }
            }
        }),
        json!({
            "name": "ax_ship",
            "description": "Run the quality-gate pipeline (same as ax ship --evaluate / --ci). Returns the ShipReport JSON. For mode=ci, isError is set when the gate fails — never exits the MCP process.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "mode": { "type": "string", "enum": ["evaluate", "ci"], "description": "Default evaluate" }
                }
            }
        }),
        json!({
            "name": "ax_policy_index",
            "description": "Re-index / import policy rules and skills from .ax/policy/ into the project store (same as ax policy index). Always available so empty DB projects can bootstrap after files exist.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "force": { "type": "boolean", "description": "Force re-import from filesystem into database mode" }
                }
            }
        }),
        json!({ "name": "ax_files", "description": "Project file listing", "inputSchema": { "type": "object", "properties": {} } }),
        json!({ "name": "ax_context", "description": "Build task context", "inputSchema": { "type": "object", "properties": { "task": { "type": "string" } }, "required": ["task"] } }),
        json!({ "name": "ax_callers", "description": "Find callers", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } }, "required": ["symbol"] } }),
        json!({ "name": "ax_callees", "description": "Find callees", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } }, "required": ["symbol"] } }),
        json!({ "name": "ax_impact", "description": "Impact radius", "inputSchema": { "type": "object", "properties": { "symbol": { "type": "string" } }, "required": ["symbol"] } }),
        json!({
            "name": "ax_cycles",
            "description": "List call-graph cycles (non-trivial SCCs on Calls/References edges).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "number", "description": "Max cycles to return (default 50)" }
                }
            }
        }),
        json!({
            "name": "ax_path",
            "description": "Shortest Calls/References path between two symbols.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string" },
                    "to": { "type": "string" }
                },
                "required": ["from", "to"]
            }
        }),
        json!({
            "name": "ax_api",
            "description": "Public API surface for a module or path prefix (exported symbols).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "module": { "type": "string", "description": "Module name or path prefix, e.g. ax-mcp or crates/ax-mcp" },
                    "limit": { "type": "number", "description": "Max symbols (default 200)" }
                },
                "required": ["module"]
            }
        }),
        json!({
            "name": "ax_insights",
            "description": "Whole-graph insights: Leiden communities (subsystems), god nodes (most-connected concepts), and surprising cross-community connections. Use to understand overall architecture before diving in.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "resolution": { "type": "number", "description": "Cluster granularity; higher = more, smaller communities (default 1.0)" },
                    "godLimit": { "type": "number", "description": "Max god nodes to return (default 20)" },
                    "surprisingLimit": { "type": "number", "description": "Max surprising connections to return (default 20)" }
                }
            }
        }),
        json!({
            "name": "ax_report",
            "description": "Generate a full Markdown architecture report (god nodes, communities, surprising connections, dead code, unresolved refs, suggested questions). Returns { markdown }.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "resolution": { "type": "number", "description": "Cluster granularity (default 1.0)" }
                }
            }
        }),
        json!({ "name": "ax_affected", "description": "Affected test files", "inputSchema": { "type": "object", "properties": { "files": { "type": "array", "items": { "type": "string" } } } } }),
        json!({
            "name": "ax_diagnostics",
            "description": "Diagnostics bridge: feed in LSP/linter/compiler diagnostics gathered by the agent (e.g. Cursor's Problems panel / ReadLints, tsc, ruff) and get back graph-correlated context — which of those files intersect CRITICAL-guarded paths, and which tests the graph says are impacted. ax cannot read editor state itself; call this after gathering diagnostics client-side.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "diagnostics": {
                        "type": "array",
                        "description": "One entry per diagnostic",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "description": "Relative file path" },
                                "line": { "type": "number" },
                                "severity": { "type": "string", "enum": ["error", "warning", "info"] },
                                "message": { "type": "string" },
                                "source": { "type": "string", "description": "Origin, e.g. tsc, eslint, ruff, rustc" }
                            },
                            "required": ["path", "message"]
                        }
                    }
                },
                "required": ["diagnostics"]
            }
        }),
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
    // Always shipped — works even in a project with no policy yet, so the
    // first durable directive can bootstrap the policy store.
    s.push_str(
        "Turn start: call ax_preflight with the user prompt and open/changed files. If you have not called it this turn, call it now before other work.\n\
         Directive capture (IMPORTANT): whenever the user states a durable rule — phrases like 'je moet', 'altijd', 'nooit', 'voortaan', 'always', 'never', 'you must', or '@rule' — treat it as a rule to persist. preflight returns directiveDetected + a ready captureProposal; ask the questions it lists, then call ax_policy_capture(action=\"save\", rule) after the user confirms. This works even if the project has no policy yet — the first save bootstraps it. Do not silently ignore such directives.\n\n",
    );
    if has_policy {
        s.push_str(
            "This project has team policy: apply CRITICAL rules before editing; do not Read or Grep .ax/policy/ on disk (policy is delivered in ax_preflight inject only); call ax_guard before Write/Delete when CRITICAL rules exist.\n\n",
        );
    }
    s.push_str(
        "For structural questions — how code works, call paths, impact, dependencies, architecture — call ax_explore FIRST with the user's question or symbol names. Treat returned numbered source as already read; do not re-grep the same symbols.\n\n\
         Use ax_search for quick symbol lookup. Use ax_node for one symbol's file context. Use ax_callers / ax_callees / ax_impact for focused graph queries.\n\n\
         Whole-graph understanding: call ax_insights for Leiden communities (subsystems), god nodes (most-connected concepts), and surprising cross-community connections. Call ax_report for a full Markdown architecture report. Edges carry a confidence tag (extracted / inferred / ambiguous) and Markdown docs are indexed as Doc nodes linked to the code they reference.\n\n\
         Memory vault: when you make a durable decision, fix a tricky bug, or establish a convention, store it with ax_remember. Use ax_recall to search past decisions before re-deriving them. Relevant memories are auto-injected via ax_preflight.\n\n\
         Ops (prefer MCP — do NOT shell ax CLI when MCP is connected):\n\
         - ax_sync after local edits that should refresh the graph\n\
         - ax_index with force=true for a full rebuild\n\
         - ax_lsp action=status|enrich for language-server Exact edges\n\
         - ax_ship mode=evaluate|ci for the quality gate (never process::exit)\n\
         - ax_policy_index to refresh rules/skills from .ax/policy/\n\
         Shell CLI only when MCP is unreachable (DEGRADED) or for install/upgrade/web/share/ship --watch.\n\n\
         Pass projectPath when cwd is not the indexed project root (monorepos). Prefer ax over grep/read for code structure.",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool_names(v: &Value) -> Vec<String> {
        v["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }

    #[tokio::test]
    async fn preflight_and_capture_always_advertised_even_without_policy() {
        let names = tool_names(&ToolHandler::list_tools(false).await);
        assert!(names.contains(&"ax_preflight".to_string()));
        assert!(names.contains(&"ax_policy_capture".to_string()));
        // Tools that need existing policy stay gated.
        assert!(!names.contains(&"ax_guard".to_string()));
        assert!(!names.contains(&"ax_rules".to_string()));
        assert!(!names.contains(&"ax_skill".to_string()));
    }

    #[tokio::test]
    async fn policy_tools_appear_when_policy_present() {
        let names = tool_names(&ToolHandler::list_tools(true).await);
        for expected in ["ax_preflight", "ax_policy_capture", "ax_guard", "ax_rules", "ax_skill"] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn preflight_tool_schema_includes_project_path() {
        let schema = preflight_tool()["inputSchema"]["properties"].clone();
        assert!(schema.get("projectPath").is_some());
        assert!(schema.get("prompt").is_some());
    }

    #[test]
    fn server_instructions_prefer_mcp_ops_over_shell() {
        let s = server_instructions(true);
        assert!(s.contains("ax_sync"));
        assert!(s.contains("ax_lsp"));
        assert!(s.contains("ax_ship"));
        assert!(s.contains("ax_policy_index"));
        assert!(s.contains("do NOT shell") || s.contains("Do NOT shell") || s.contains("Shell CLI only"));
    }

    #[test]
    fn server_instructions_always_mention_directive_capture() {
        // Even with no policy, agents must be told to capture directives.
        let s = server_instructions(false);
        assert!(s.contains("ax_preflight"));
        assert!(s.contains("Directive capture"));
        assert!(s.contains("ax_policy_capture"));
    }

    #[test]
    fn lean_default_hides_extras_including_diagnostics() {
        let mut tools = extra_tools();
        tools.insert(0, explore_tool());
        tools.insert(1, preflight_tool());
        tools.insert(2, capture_tool());
        crate::tool_filter::filter_tools_list_with(&mut tools, None);
        let names: Vec<String> = tools
            .iter()
            .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
            .collect();
        assert!(names.contains(&"ax_explore".to_string()));
        assert!(names.contains(&"ax_preflight".to_string()));
        assert!(names.contains(&"ax_policy_capture".to_string()));
        assert!(!names.contains(&"ax_diagnostics".to_string()));
        assert!(!names.contains(&"ax_sync".to_string()));
        assert!(!names.contains(&"ax_search".to_string()));
    }

    #[test]
    fn ax_mcp_tools_all_advertises_ops_and_diagnostics() {
        let mut tools = extra_tools();
        tools.insert(0, explore_tool());
        tools.insert(1, preflight_tool());
        crate::tool_filter::filter_tools_list_with(
            &mut tools,
            crate::tool_filter::resolve_tool_allowlist_from(Some("all")),
        );
        let names: Vec<String> = tools
            .iter()
            .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
            .collect();
        for expected in [
            "ax_diagnostics",
            "ax_sync",
            "ax_lsp",
            "ax_ship",
            "ax_policy_index",
            "ax_index",
            "ax_search",
            "ax_explore",
            "ax_preflight",
        ] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn ax_mcp_tools_allowlist_adds_short_names() {
        let mut tools = extra_tools();
        tools.insert(0, explore_tool());
        crate::tool_filter::filter_tools_list_with(
            &mut tools,
            crate::tool_filter::resolve_tool_allowlist_from(Some("node,status")),
        );
        let names: Vec<String> = tools
            .iter()
            .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
            .collect();
        assert!(names.contains(&"ax_node".to_string()));
        assert!(names.contains(&"ax_status".to_string()));
        assert!(!names.contains(&"ax_callers".to_string()));
        assert!(names.contains(&"ax_explore".to_string()));
    }

    #[test]
    fn diagnostics_from_params_parses_entries_and_defaults_severity() {
        let params = json!({
            "diagnostics": [
                { "path": "src/lib.rs", "line": 42, "severity": "warning", "message": "unused import", "source": "rustc" },
                { "file": "src/main.rs", "message": "missing semicolon" },
                { "message": "no path — should be dropped" }
            ]
        });
        let entries = diagnostics_from_params(&params);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "src/lib.rs");
        assert_eq!(entries[0].severity, "warning");
        assert_eq!(entries[0].line, Some(42));
        assert_eq!(entries[1].path, "src/main.rs");
        assert_eq!(entries[1].severity, "error"); // default
    }

    #[test]
    fn diagnostics_from_params_empty_without_diagnostics_key() {
        assert!(diagnostics_from_params(&json!({})).is_empty());
    }
}
