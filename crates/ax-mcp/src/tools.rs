//! MCP tools - ax_explore, ax_search, ax_status, policy tools, etc.

use std::path::{Path, PathBuf};

use ax_context::directory::find_nearest_ax_root;
use ax_context::{format_context_as_markdown, format_explore_text};
use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;
use ax_policy::{
    detect_directive, finalize_proposal, propose_rule_from_prompt, GuardOp, MatchInput,
    MatchResult, PolicyStatus, PolicyStore, RuleFrontmatter,
};
use ax_reasoning::{maybe_synthesize_explore, ExploreOffloadMeta};
use ax_types::{
    BuildContextOptions, ExploreOptions, Node, SearchOptions, SearchResult, Subgraph, TaskInput,
};
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
        advertise_fresh(&mut tools);
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
                let results = ax
                    .search_nodes(
                        query,
                        &SearchOptions {
                            limit: Some(20),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                let text = format_search_results_text(&format!("Search: {query}"), &results);
                Ok(json!({ "text": text, "results": results }))
            }
            "ax_status" => status(ax).await,
            "ax_budget" => budget_tool(ax).await,
            "ax_index" => index_tool(ax, params).await,
            "ax_sync" => sync_tool(ax).await,
            "ax_lsp" => lsp_tool(ax, params).await,
            "ax_ship" => ship_tool(ax, params).await,
            "ax_policy_index" => policy_index_tool(ax, params).await,
            "ax_session" => {
                let conversation = chat_of(&params);
                let fingerprint = match indexed_hashes(ax.db_pool()).await {
                    Ok(index) => ax_usage::index_fingerprint(&index),
                    Err(_) => String::new(),
                };
                let action = params
                    .get("action")
                    .and_then(Value::as_str)
                    .unwrap_or("get");
                if matches!(action, "fork" | "handoff") {
                    let child = crate::chat_session::mint_session();
                    let text = if action == "fork" {
                        ax_usage::fork_working_context(
                            ax.project_root(),
                            &conversation,
                            &child,
                            &fingerprint,
                        )
                        .await?
                    } else {
                        ax_usage::handoff_working_context(
                            ax.project_root(),
                            &conversation,
                            &child,
                            &params,
                            &fingerprint,
                        )
                        .await?
                    };
                    return Ok(json!({ "text": text, "session": child }));
                }
                let text = ax_usage::working_context_apply(
                    ax.project_root(),
                    &conversation,
                    &params,
                    &fingerprint,
                )
                .await?;
                Ok(json!({ "text": text }))
            }
            "ax_durable" => {
                let conversation = chat_of(&params);
                let text =
                    ax_usage::durable_project_apply(ax.project_root(), &conversation, &params)
                        .await?;
                Ok(json!({ "text": text }))
            }
            "ax_tool_economics" => pi_economics_tool(ax, params).await,
            "ax_optimization_advice" => pi_optimization_tool(ax, params).await,
            "ax_cost" => pi_cost_tool(ax, params).await,
            "ax_context" => {
                let task = params.get("task").and_then(|v| v.as_str()).unwrap_or("");
                let ctx = ax
                    .build_context(
                        TaskInput::Text(task.to_string()),
                        BuildContextOptions::default(),
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                let text = format_context_as_markdown(&ctx);
                let mut value = serde_json::to_value(&ctx).map_err(|e| e.to_string())?;
                if let Some(obj) = value.as_object_mut() {
                    obj.insert("text".to_string(), Value::String(text));
                    match crate::pi_knowledge::pi_context_for_task(ax, task).await {
                        Ok(pi) => {
                            if let Ok(encoded) = serde_json::to_value(&pi) {
                                obj.insert("piContext".to_string(), encoded);
                            }
                        }
                        Err(err) => {
                            tracing::warn!("AX_PI_INTEGRATION_ERROR {err}");
                            obj.insert("piContext".to_string(), json!({ "degraded": true }));
                        }
                    }
                }
                Ok(value)
            }
            "ax_callers" => {
                let sym = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax
                    .search_nodes(
                        sym,
                        &SearchOptions {
                            limit: Some(1),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(first) = nodes.first() {
                    let callers = ax
                        .get_callers(&first.node.id, 3)
                        .await
                        .map_err(|e| e.to_string())?;
                    let text = format_nodes_text(&format!("Callers of '{sym}'"), &callers);
                    Ok(json!({ "text": text, "callers": callers }))
                } else {
                    Ok(json!({ "text": format!("No symbol matching '{sym}'"), "callers": [] }))
                }
            }
            "ax_callees" => {
                let sym = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax
                    .search_nodes(
                        sym,
                        &SearchOptions {
                            limit: Some(1),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(first) = nodes.first() {
                    let callees = ax
                        .get_callees(&first.node.id, 3)
                        .await
                        .map_err(|e| e.to_string())?;
                    let text = format_nodes_text(&format!("Callees of '{sym}'"), &callees);
                    Ok(json!({ "text": text, "callees": callees }))
                } else {
                    Ok(json!({ "text": format!("No symbol matching '{sym}'"), "callees": [] }))
                }
            }
            "ax_impact" => {
                let sym = params.get("symbol").and_then(|v| v.as_str()).unwrap_or("");
                let nodes = ax
                    .search_nodes(
                        sym,
                        &SearchOptions {
                            limit: Some(1),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                if let Some(first) = nodes.first() {
                    let sg = ax
                        .get_impact_radius(&first.node.id, 3)
                        .await
                        .map_err(|e| e.to_string())?;
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
                    .search_nodes(
                        from,
                        &SearchOptions {
                            limit: Some(1),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                let to_hits = ax
                    .search_nodes(
                        to,
                        &SearchOptions {
                            limit: Some(1),
                            ..Default::default()
                        },
                    )
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
                    Some(ids) if !ids.is_empty() => {
                        let mut out = format!("Path ({} hops):\n", ids.len().saturating_sub(1));
                        for id in ids {
                            match ax.get_node(id).await.map_err(|e| e.to_string())? {
                                Some(n) => out.push_str(&format!(
                                    "- {} — {}:{}-{}\n",
                                    n.qualified_name, n.file_path, n.start_line, n.end_line
                                )),
                                None => out.push_str(&format!("- {id}\n")),
                            }
                        }
                        out
                    }
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
                let nodes = ax
                    .module_api(module, limit)
                    .await
                    .map_err(|e| e.to_string())?;
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
                let files = ax
                    .queries()
                    .get_all_files()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(json!({ "files": files }))
            }
            "ax_node" => node(ax, params).await,
            "ax_affected" => {
                let files: Vec<String> = params
                    .get("files")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                let affected = ax
                    .get_affected_files(&files)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(json!({ "affected": affected }))
            }
            "ax_remember" => remember(ax, params).await,
            "ax_recall" => recall(ax, params).await,
            "ax_history" => history_tool(ax, params).await,
            "ax_expand" => expand_tool(ax, params).await,
            "ax_stash" => stash_tool(ax, params).await,
            "ax_cache_status" => cache_status_tool(params).await,
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

async fn pi_economics_tool(ax: &Ax, params: Value) -> Result<Value, String> {
    let db = ax.project_root().join(".ax").join("ax.db");
    let session = params.get("sessionId").and_then(|v| v.as_str());
    let settings = ax_usage::load_settings(Some(ax.project_root()));
    let currency = match settings.currency {
        ax_usage::Currency::Eur => "EUR",
        ax_usage::Currency::Usd => "USD",
    };
    match ax_pi::read_economics(&db, session, currency, settings.usd_per_eur).await {
        Ok((text, value)) => Ok(json!({ "text": text, "report": value, "degraded": false })),
        Err(err) => {
            tracing::warn!("AX_PI_INTEGRATION_ERROR {err}");
            Ok(json!({ "text": "AX_PI_INTEGRATION_ERROR", "degraded": true, "error": err }))
        }
    }
}

async fn pi_optimization_tool(ax: &Ax, _params: Value) -> Result<Value, String> {
    let db = ax.project_root().join(".ax").join("ax.db");
    match ax_pi::read_optimization(&db).await {
        Ok((text, value)) => Ok(json!({ "text": text, "report": value, "degraded": false })),
        Err(err) => {
            tracing::warn!("AX_PI_INTEGRATION_ERROR {err}");
            Ok(json!({ "text": "AX_PI_INTEGRATION_ERROR", "degraded": true, "error": err }))
        }
    }
}

async fn pi_cost_tool(ax: &Ax, params: Value) -> Result<Value, String> {
    pi_economics_tool(ax, params).await
}

async fn budget_tool(ax: &Ax) -> Result<Value, String> {
    let (decision, text) = ax_usage::budget_check(Some(ax.project_root())).await?;
    Ok(json!({
        "text": text,
        "decision": decision,
        "enforced": false,
        "note": "Ax did not block a provider API call. deny is advice for an integration that asked."
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
    let force = params
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
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
            let servers = ax_lsp::discover_servers_in(Some(&root));
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
                    ax_usage::log_lsp(Some(&root), "enrich fail via=mcp");
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
        other => Err(format!(
            "ax_lsp action must be status or enrich, got {other}"
        )),
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
    let force = params
        .get("force")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
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

fn resolve_preflight_cwd(ax: &Ax, params: &Value) -> Result<PathBuf, String> {
    if let Some(path) = params
        .get("projectPath")
        .or_else(|| params.get("project_path"))
        .and_then(Value::as_str)
    {
        let root = find_nearest_ax_root(&PathBuf::from(path))
            .ok_or("projectPath does not identify an initialized project; connect MCP to the intended project")?;
        if ax_usage::project_scope(&root) != ax_usage::project_scope(ax.project_root()) {
            return Err("projectPath differs from this MCP server's project; connect a separate server to that project".into());
        }
    }
    Ok(ax.project_root().to_path_buf())
}

async fn preflight(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let files = string_array(params.get("files"));
    let cwd = resolve_preflight_cwd(ax, &params)?;
    let input = MatchInput {
        prompt: prompt.clone(),
        cwd,
        open_files: files.iter().map(PathBuf::from).collect(),
        changed_files: vec![],
    };
    // Preflight must always return a payload. Policy match/status failures
    // degrade to an empty inject + error note — never MCP isError.
    let mut policy_error: Option<String> = None;
    let mut result = match ax.match_policy(input).await {
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
    // Durable memories ride along with the policy inject. Failures here must
    // never break preflight — memories are additive context.
    let mut memories = ax_memory::recall_for_prompt(ax.db_pool(), &prompt, 3)
        .await
        .unwrap_or_default();
    if crate::links::has_links(&result, &memories) {
        let rows = ax.policy_rows().await;
        let memory_rows = ax_memory::list(ax.db_pool(), 100_000, 0).await;
        if let (Ok((rules, skills)), Ok((memory_rows, _))) = (rows, memory_rows) {
            let added = crate::links::expand_links(
                &mut result,
                &mut memories,
                &rules,
                &skills,
                &memory_rows,
            );
            crate::verbose::push_line(format!("enrich links added={added}"));
        }
    }
    let meta = ax_policy::build_preflight_meta(&status, &result);
    let session_delivered: Option<std::collections::HashMap<String, u64>> = params
        .get(crate::server::SESSION_ARG)
        .and_then(|s| s.get("delivered"))
        .and_then(|d| serde_json::from_value(d.clone()).ok());
    let (policy_inject, mut delivered) = match params.get(crate::server::SESSION_ARG) {
        Some(session) if policy_error.is_none() => session_policy_inject(ax, session, &result),
        _ => (result.inject.clone(), Vec::new()),
    };
    crate::verbose::push_line(format!(
        "enrich policy matched_rules={} matched_skills={} inject_chars={} mode={}",
        meta.matched_rules,
        meta.matched_skills,
        policy_inject.len(),
        meta.mode
    ));

    // Index snapshot rides along with policy inject — failures must never
    // break preflight; stats are additive context.
    let index_stats = ax.get_stats().await.ok();
    let pending = ax.get_pending_files().await;

    let mut optional_blocks: Vec<ax_usage::ContextBlock> = Vec::new();
    let mut inject = format!(
        "# Ax context\n\nProject: `{}`\n\n{}",
        ax_usage::project_scope(ax.project_root()),
        crate::smart_output::policy_markdown(&policy_inject)
    );
    if let Some(ref stats) = index_stats {
        let block = crate::smart_output::index_context(stats, &pending);
        if !block.is_empty() {
            let mut index_text = String::new();
            push_once(
                &mut index_text,
                "block:index",
                &block,
                session_delivered.as_ref(),
                &mut delivered,
            );
            if !index_text.is_empty() {
                optional_blocks.push(ax_usage::ContextBlock {
                    id: "block:index".into(),
                    class: if pending.is_empty()
                        && ax_core::stats_format::source_store_warning(stats).is_none()
                    {
                        ax_usage::ContextClass::Optional
                    } else {
                        ax_usage::ContextClass::HardRequired
                    },
                    text: index_text,
                });
            }
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
        let block = ax_memory::format_memory_match_titles(&memories);
        if !block.is_empty() {
            optional_blocks.push(ax_usage::ContextBlock {
                id: "memories".into(),
                class: ax_usage::ContextClass::HighValue,
                text: crate::smart_output::generated_markdown(
                    &block,
                    "ax_memories",
                    "Relevant memories — ax_recall by ID for full content",
                ),
            });
            crate::verbose::push_line(format!(
                "enrich memories count={} block_chars={}",
                memories.len(),
                block.len()
            ));
        }
    } else {
        crate::verbose::push_line("enrich memories none");
    }
    let open_files = project_relative_files(ax.project_root(), &files).await;
    let turns = ax_memory::related_turns(ax.db_pool(), &prompt, &open_files, 3)
        .await
        .unwrap_or_default();
    let turn_block = ax_memory::format_turn_history_block(&turns, 1_200);
    if !turn_block.is_empty() {
        optional_blocks.push(ax_usage::ContextBlock {
            id: "history".into(),
            class: ax_usage::ContextClass::Optional,
            text: turn_block.clone(),
        });
        crate::verbose::push_line(format!(
            "enrich turn history count={} block_chars={}",
            turns.len(),
            turn_block.len()
        ));
    }
    if ax_memory::is_history_question(&prompt) {
        if !inject.is_empty() {
            inject.push('\n');
        }
        inject.push_str("<ax_history_hint>This asks when something changed: call ax_history with the file, symbol, or topic. It lists dated agent turns (prompt, outcome, files, commits) and git commits.</ax_history_hint>");
    }

    match ax_core::review_language::resolve_for_project(ax.project_root()) {
        Ok(language) => {
            if !inject.is_empty() {
                inject.push('\n');
            }
            inject.push_str(&ax_core::review_language::review_language_line(language));
        }
        Err(error) => {
            if !inject.is_empty() {
                inject.push('\n');
            }
            inject.push_str(&error);
        }
    }

    if ax_usage::cache_enabled() {
        if !inject.is_empty() {
            inject.push('\n');
        }
        push_once(
            &mut inject,
            "block:cache_help",
            CONTEXT_CACHE_LINE,
            session_delivered.as_ref(),
            &mut delivered,
        );
        let conversation = chat_of(&params);
        let index = indexed_hashes(ax.db_pool()).await;
        let graph = index.as_ref().ok().map(ax_usage::index_fingerprint);
        inject.push('\n');
        inject.push_str(&chat_line(&conversation, graph.as_deref()));
        if let Ok(index) = index.as_ref() {
            let unseen: Vec<_> =
                ax_usage::reuse_session_entries(ax.project_root(), &conversation, index)
                    .await
                    .into_iter()
                    .filter(|e| {
                        !session_delivered
                            .as_ref()
                            .is_some_and(|m| m.contains_key(&format!("reuse:{}", e.id)))
                    })
                    .collect();
            let known = ax_usage::format_session_context(&unseen, ax_usage::SESSION_CONTEXT_TOKENS);
            if !known.is_empty() {
                if session_delivered.is_some() {
                    delivered.extend(
                        unseen
                            .iter()
                            .filter(|e| known.contains(&e.id))
                            .map(|e| (format!("reuse:{}", e.id), 1)),
                    );
                }
                optional_blocks.push(ax_usage::ContextBlock {
                    id: "reuse".into(),
                    class: ax_usage::ContextClass::Optional,
                    text: known,
                });
            }
            let fingerprint = ax_usage::index_fingerprint(index);
            let known = params.get("known_context").and_then(Value::as_str);
            let working = ax_usage::working_context_block(
                ax.project_root(),
                &conversation,
                &fingerprint,
                known,
            )
            .await;
            if !working.is_empty() {
                inject.push('\n');
                inject.push_str(&ax_section("session", &working));
            }
            let turns = params.get(TURNS_ARG).and_then(Value::as_u64).unwrap_or(0) as u32;
            let stale = working
                .lines()
                .next()
                .is_some_and(|header| header.contains("stale=true"));
            if let Some(nudge) = ax_usage::session_nudge(turns, stale) {
                inject.push('\n');
                inject.push_str(&ax_section("nudge", &nudge));
            }
        }
    }

    if let Ok((decision, text)) = ax_usage::budget_check(Some(ax.project_root())).await {
        if decision != "allow" && !text.is_empty() {
            if !inject.is_empty() {
                inject.push('\n');
            }
            inject.push_str(&text);
        }
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

    let mut instruction =
        "Apply CRITICAL rules before editing. If a skill matched, follow its workflow.".to_string();
    if has_directive {
        instruction.push_str(" DIRECTIVE DETECTED — captureProposal holds a ready rule. Ask the user the questions in captureProposal.questions, then call ax_policy_capture(action=\"save\", rule) after they say yes. This persists even in a project with no prior policy.");
    }

    crate::verbose::push_line(format!(
        "enrich done final_inject_chars={} directive={}",
        inject.len(),
        has_directive
    ));

    if let Some(proposal) = capture_proposal.as_ref() {
        inject.push_str("\n\n## Directive proposal\n\n");
        inject.push_str(&proposal.preview);
        inject.push_str("\n\n");
        inject.push_str(&proposal.interview_instruction);
        for question in &proposal.questions {
            inject.push_str(&format!(
                "\n- {} (current: {}; options: {})",
                question.question,
                question.current,
                question.options.join(", ")
            ));
        }
    }
    let budget = ax_usage::load_settings(Some(ax.project_root())).context_budget_tokens;
    let mut blocks = vec![ax_usage::ContextBlock {
        id: "required".into(),
        class: ax_usage::ContextClass::HardRequired,
        text: inject,
    }];
    blocks.extend(optional_blocks);
    let selected = ax_usage::select_context(&blocks, budget);
    let included: Vec<_> = selected.iter().map(|b| b.id.as_str()).collect();
    let omitted: Vec<_> = blocks
        .iter()
        .filter(|b| !included.contains(&b.id.as_str()))
        .map(|b| b.id.as_str())
        .collect();
    // Do not mark a block as delivered if the budget omitted it.
    delivered.retain(|(key, _)| match key.as_str() {
        "block:index" => included.contains(&"block:index"),
        key if key.starts_with("reuse:") => included.contains(&"reuse"),
        _ => true,
    });
    let mut inject = selected
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("\n\n");
    if !omitted.is_empty() {
        inject.push_str("\n\nOptional context omitted to fit the budget. Retrieve index details with `ax_status`, decisions with `ax_recall`, history with `ax_history`, or rerun graph queries with `fresh: true`.");
    }
    let over_budget = budget.is_some_and(|limit| ax_usage::count_tokens(&inject) > limit as usize);
    if over_budget {
        inject.push_str(
            "\n\nContext budget exceeded: required policy and working context are preserved.",
        );
    }
    let mut out = json!({
        "project": { "path": ax_usage::project_scope(ax.project_root()) },
        "session": { "id": chat_of(&params) },
        "contextBudget": { "included": included, "omitted": omitted, "limit": budget, "overBudget": over_budget, "tokens": ax_usage::count_tokens(&inject), "measurement": if ax_usage::tokenizer_available() { "o200k_base" } else { "estimate" } },
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
    if !delivered.is_empty() {
        out[crate::server::DELIVERED_KEY] = json!(delivered);
    }
    Ok(out)
}

/// Append `block` unless this session already received the identical text.
fn push_once(
    inject: &mut String,
    key: &str,
    block: &str,
    session: Option<&std::collections::HashMap<String, u64>>,
    delivered: &mut Vec<(String, u64)>,
) {
    let hash = ax_policy::format::content_hash(block);
    if session.and_then(|m| m.get(key)) == Some(&hash) {
        return;
    }
    if session.is_some() {
        delivered.push((key.to_string(), hash));
    }
    if !inject.is_empty() {
        inject.push('\n');
    }
    inject.push_str(block);
}

fn skill_inline_chars() -> usize {
    let tokens = std::env::var("AX_POLICY_SKILL_INLINE_TOKENS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1_500);
    tokens.saturating_mul(4)
}

/// Private args key: the chat the server resolved for this call.
pub(crate) const CHAT_ARG: &str = "__axChat";
/// Private args key: preflight calls in this chat since its last `ax_session` write.
pub(crate) const TURNS_ARG: &str = "__axTurns";

fn chat_of(params: &Value) -> String {
    match params.get(CHAT_ARG).and_then(Value::as_str) {
        Some(chat) => chat.to_string(),
        None => ax_usage::conversation_key(ax_usage::read_active_cursor_session()),
    }
}

fn ax_section(name: &str, body: &str) -> String {
    format!("<ax_section name=\"{name}\">\n{body}\n</ax_section>")
}

fn chat_line(chat: &str, fingerprint: Option<&str>) -> String {
    let graph = fingerprint
        .map(|f| format!(" graph={}", &f[..f.len().min(16)]))
        .unwrap_or_default();
    format!(
        "<ax_chat session={chat}{graph}>Pass \"session\": \"{chat}\" to ax_preflight and to every ax tool call in this chat. A preflight without it starts a new chat.</ax_chat>"
    )
}

/// Policy inject for one MCP connection: skip bodies the agent already holds.
fn session_policy_inject(
    ax: &Ax,
    session: &Value,
    result: &MatchResult,
) -> (String, Vec<(String, u64)>) {
    let delivered: std::collections::HashMap<String, u64> = session
        .get("delivered")
        .and_then(|d| serde_json::from_value(d.clone()).ok())
        .unwrap_or_default();
    let client = session.get("client").and_then(|v| v.as_str()).unwrap_or("");
    let client_loaded = if client.to_ascii_lowercase().contains("cursor") {
        let mut roots = vec![ax.project_root().to_path_buf()];
        roots.extend(ax_utils::paths::home_dir());
        ax_policy::ide_loaded::ide_loaded_keys(&roots, &result.rules, &result.skills)
    } else {
        std::collections::HashSet::new()
    };
    let out = ax_policy::format::format_inject_block_with(
        &result.rules,
        &result.skills,
        ax_policy::matcher::max_inject_chars(),
        ax_policy::format::InjectOptions {
            delivered: Some(&delivered),
            client_loaded: Some(&client_loaded),
            skill_inline_chars: Some(skill_inline_chars()),
            rule_inline_chars: None,
        },
    );
    (out.text, out.delivered)
}

async fn remember(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let body = params
        .get("body")
        .or_else(|| params.get("text"))
        .and_then(|v| v.as_str())
        .ok_or("body required")?
        .to_string();
    let input = ax_memory::RememberInput {
        title: params
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        body,
        kind: params
            .get("kind")
            .and_then(|v| v.as_str())
            .map(String::from),
        tags: string_array(params.get("tags")),
        files: string_array(params.get("files")),
        source: Some("mcp".into()),
    };
    let row = ax_memory::remember(ax.db_pool(), input)
        .await
        .map_err(|e| e.to_string())?;
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
        "Memory saved. It will surface in future ax_preflight and ax_recall calls when relevant."
            .to_string()
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
    if let Some(id) = params.get("id").and_then(Value::as_str) {
        let memory = ax_memory::get(ax.db_pool(), id)
            .await
            .map_err(|e| e.to_string())?
            .filter(|m| m.enabled)
            .ok_or("memory ID not found in this project")?;
        return Ok(
            json!({ "text": format!("## {} [{}]\n\n{}\n\nFiles: {}", memory.title, memory.id, memory.body, memory.files.join(", ")), "matches": [{ "memory": memory }] }),
        );
    }
    let query = params
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or("query required")?;
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(5)
        .min(25) as usize;
    let matches = ax_memory::recall(ax.db_pool(), query, limit)
        .await
        .map_err(|e| e.to_string())?;
    let text = if matches.is_empty() {
        format!("No memories match '{query}'.")
    } else {
        crate::smart_output::memory_results(&matches)
    };
    Ok(json!({
        "matches": matches,
        "inject": text,
    }))
}

const HISTORY_OUTCOME_CHARS: usize = 600;

async fn history_tool(ax: &mut Ax, params: Value) -> Result<Value, String> {
    if let Some(id) = params.get("id").and_then(|v| v.as_str()) {
        let entry = ax_memory::history_entry(ax.db_pool(), id)
            .await
            .map_err(|e| e.to_string())?;
        let text = match entry {
            Some(entry) => ax_memory::format_history(&[entry], usize::MAX),
            None => format!("No turn memory with id {id}."),
        };
        return Ok(json!({ "text": text }));
    }
    let query = params
        .get("query")
        .and_then(|v| v.as_str())
        .filter(|q| !q.trim().is_empty())
        .ok_or("query or id required")?;
    let since_ms = match params.get("since").and_then(|v| v.as_str()) {
        Some(date) => Some(ax_memory::parse_since(date).ok_or("since must be YYYY-MM-DD")?),
        None => None,
    };
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .clamp(1, 50) as usize;
    let history_query = ax_memory::HistoryQuery {
        query: query.to_string(),
        since_ms,
        limit,
    };
    let entries = ax_memory::history(ax.db_pool(), ax.project_root(), &history_query)
        .await
        .map_err(|e| e.to_string())?;
    Ok(json!({
        "text": ax_memory::format_history(&entries, HISTORY_OUTCOME_CHARS),
        "count": entries.len(),
    }))
}

/// Preflight `files` as project-relative `/` paths, the form turn memories store.
async fn project_relative_files(root: &Path, files: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(files.len());
    for file in files {
        let path = Path::new(file);
        let relative = match path.strip_prefix(root) {
            Ok(relative) => relative.to_path_buf(),
            Err(_) => match tokio::fs::canonicalize(path).await {
                Ok(canonical) => canonical
                    .strip_prefix(root)
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|_| path.to_path_buf()),
                Err(_) => path.to_path_buf(),
            },
        };
        let text = relative.to_string_lossy().replace('\\', "/");
        out.push(text.trim_start_matches("./").to_string());
    }
    out
}

async fn expand_tool(ax: &Ax, params: Value) -> Result<Value, String> {
    let id = params
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("id required")?;
    let offset = params.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize);
    match ax_usage::expand_project_cached(ax.project_root(), id, offset, limit).await {
        Ok(page) => Ok(json!({
            "text": page.text,
            "id": id,
            "offset": offset,
            "nextOffset": page.next_offset,
        })),
        Err(msg) => Ok(json!({ "text": msg, "isError": true })),
    }
}

async fn cache_status_tool(params: Value) -> Result<Value, String> {
    let requested = params
        .get("session")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let fallback = crate::verbose::active_session_id();
    let session = requested.as_deref().or(fallback.as_deref());
    let status = ax_usage::load_cache_status(session).await;
    let lines = ax_usage::format_cache_status_lines(session, &status);
    for line in &lines {
        crate::verbose::push_line(line);
    }
    Ok(json!({
        "text": lines.join("\n"),
        "group": ax_usage::cache_group_key(session),
        "context": {
            "enabled": status.enabled,
            "threshold": status.threshold,
            "rows": status.live_rows,
            "storedTokens": status.stored_tokens,
            "expired": status.expired_rows,
            "sessionRows": status.session_rows,
            "sessionStoredTokens": status.session_stored_tokens,
            "sessionInlineTokens": status.session_inline_tokens,
            "error": status.context_error,
        },
        "tokenCache": {
            "entries": status.tokens.entries,
            "capacity": status.tokens.capacity,
            "hits": status.tokens.hits,
            "misses": status.tokens.misses,
            "evictions": status.tokens.evictions,
            "tokenizer": status.tokens.tokenizer,
            "lockOk": status.tokens.lock_ok,
        },
    }))
}

async fn stash_tool(ax: &Ax, params: Value) -> Result<Value, String> {
    let text = params
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or("text required")?;
    let label = params.get("label").and_then(|v| v.as_str());
    match ax_usage::stash_project_text(ax.project_root(), label, text).await {
        Ok(receipt) => Ok(json!({
            "text": format!(
                "Stashed {} tokens as {}. Call ax_expand with that id. The body is not repeated here.",
                receipt.original_tokens, receipt.id
            ),
            "id": receipt.id,
            "originalTokens": receipt.original_tokens,
        })),
        Err(msg) => Ok(json!({ "text": msg, "isError": true })),
    }
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
    let mut value = serde_json::to_value(&insights).map_err(|e| e.to_string())?;
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "suggestedQuestions".to_string(),
            json!(ax_core::report::suggested_questions(&insights)),
        );
    }
    Ok(value)
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
    let prompt = params
        .get("prompt")
        .and_then(|v| v.as_str())
        .map(String::from);
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
        let all = ax_policy::list_rules(ax.db_pool())
            .await
            .map_err(|e| e.to_string())?;
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
        let doc = store
            .save_rule(fm.clone(), body)
            .await
            .map_err(|e| e.error)?;
        let storage_label = match storage {
            ax_policy::PolicyStorage::Database => "database",
            ax_policy::PolicyStorage::Files => "files",
        };
        return Ok(json!({
            "ok": true,
            "action": "save",
            "id": doc.frontmatter.id,
            "storage": storage_label,
            "path": format!(".agents/rules/{}.mdc", doc.frontmatter.id),
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
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("name required")?;
    let row = ax
        .get_policy_skill(name)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("skill not found: {name}"))?;
    let key = format!("skill_full:{name}");
    let hash = ax_policy::format::delivered_body_hash(&row.body, &row.properties);
    let session = params.get(crate::server::SESSION_ARG);
    let seen = session
        .and_then(|s| s.get("delivered"))
        .and_then(|d| d.get(&key))
        .and_then(Value::as_u64);
    if seen == Some(hash) {
        return Ok(json!({
            "text": format!("Skill '{name}' is unchanged since you loaded it earlier in this session; use the copy in your context."),
            "name": name,
            "unchanged": true,
        }));
    }
    let mut value = json!(row);
    if session.is_some() {
        value[crate::server::DELIVERED_KEY] = json!([[key, hash]]);
    }
    Ok(value)
}

async fn guard(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let paths = guard_paths_from_params(&params)?;
    let op = guard_op_from_params(&params);
    if paths.len() == 1 {
        let path_str = &paths[0];
        let path = ax.project_root().join(path_str);
        let content = guard_content_from_params(&params).or_else(|| std::fs::read(&path).ok());
        let result = ax
            .guard_operation(&path, op, content.as_deref())
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
            .guard_operation(&path, op, content.as_deref())
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

    let mut by_path: std::collections::BTreeMap<String, Vec<&DiagnosticEntry>> =
        std::collections::BTreeMap::new();
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
                    let source = d.get("source").and_then(|v| v.as_str()).map(String::from);
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
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
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
    match v {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => vec![],
    }
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
                "files": {
                    "anyOf": [
                        { "type": "array", "items": { "type": "string" } },
                        { "type": "string" }
                    ]
                },
                "projectPath": {
                    "type": "string",
                    "description": "Optional project root when cwd differs from the MCP --path index (monorepos). Resolves to the nearest ax root."
                },
                "known_context": {
                    "type": "string",
                    "description": "The <ax_working_context> hash you already have; when it is still current, preflight sends one unchanged line instead of the notes"
                }
            }
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
        "description": "Pre-write guard for CRITICAL policy rules. Prefer path+operation; also accepts paths[] and action=edit|write|delete. Checks built-in encoding/secrets rules, plus any CRITICAL rule whose body contains a `guard: forbid-path: \"<glob>\"`, `guard: forbid-content: \"<substring or /regex/>\"` (scoped by that rule's globs when it has any), `guard: require-content: \"<substring or /regex/>\"` (scoped by that rule's globs), or `guard: require-skill: \"<name>\"` (skill must exist, be approved, and have alwaysApply) directive line.",
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

/// Indexed `content_hash` per path from the project `files` table; all files when `paths` is `None`.
pub(crate) async fn indexed_hashes(
    pool: &sqlx::SqlitePool,
) -> Result<ax_usage::IndexHashes, String> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT path, content_hash FROM files")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().collect())
}

fn advertise_fresh(tools: &mut [Value]) {
    for tool in tools.iter_mut() {
        let cacheable = tool["name"].as_str().is_some_and(ax_usage::reuse_cacheable);
        if let (true, Some(props)) = (cacheable, tool["inputSchema"]["properties"].as_object_mut())
        {
            props.insert(
                "fresh".to_string(),
                json!({ "type": "boolean", "description": "Skip the per-conversation cache and rerun the query" }),
            );
        }
        let name = tool["name"].as_str().unwrap_or_default();
        let chat_state = cacheable || matches!(name, "ax_preflight" | "ax_session" | "ax_durable");
        if let (true, Some(props)) = (
            chat_state,
            tool["inputSchema"]["properties"].as_object_mut(),
        ) {
            props.insert(
                "session".to_string(),
                json!({ "type": "string", "description": "The session id preflight printed in <ax_chat>; keeps this chat's cache and notes" }),
            );
        }
    }
}

fn extra_tools() -> Vec<Value> {
    vec![
        json!({
            "name": "ax_node",
            "description": "Open a symbol from the index: full numbered source (up to 400 lines) plus direct callers and callees for the best matches. Use this instead of Read for a function, class, or method.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Symbol name or qualified name" },
                    "limit": { "type": "number", "description": "Max matches (default 3)" },
                    "maxLinesPerSnippet": { "type": "number", "description": "Source lines per match (default 400)" },
                    "mode": { "type": "string", "enum": ["signature"], "description": "signature: path, lines, and declaration only" },
                    "maxSourceChars": { "type": "number", "description": "Source chars per match (default 24000)" }
                },
                "required": ["name"]
            }
        }),
        json!({ "name": "ax_search", "description": "FTS symbol search", "inputSchema": { "type": "object", "properties": { "query": { "type": "string" } }, "required": ["query"] } }),
        json!({ "name": "ax_status", "description": "Index stats and staleness", "inputSchema": { "type": "object", "properties": {} } }),
        json!({
            "name": "ax_budget",
            "description": "Local budget decision (allow, warn, or deny). Does not block a provider API call. deny is advice for an integration that asked.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
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
        json!({
            "name": "ax_durable",
            "description": "Durable transcript, documents, and tasks for this chat. append/read/search store the conversation. compact writes a summary and hides older entries from read; search still finds them. fork reads the parent up to an entry and copies documents. handoff starts a new transcript from a note. doc_put/doc_get store JSON state. task_start, task_checkpoint, task_resume, and task_finish keep a step that survives a restart. hook records a name that is written into the transcript when that event runs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["append", "read", "search", "compact", "fork", "handoff", "doc_put", "doc_get", "task_start", "task_checkpoint", "task_resume", "task_finish", "hook"] },
                    "kind": { "type": "string" },
                    "body": {},
                    "query": { "type": "string" },
                    "summary": { "type": "string" },
                    "first_kept": { "type": "number" },
                    "at": { "type": "number" },
                    "note": { "type": "string" },
                    "task": { "type": "string" },
                    "phase": { "type": "string" },
                    "checkpoint": { "type": "object" },
                    "input": { "type": "object" },
                    "result": {},
                    "event": { "type": "string" },
                    "name": { "type": "string" }
                }
            }
        }),
        json!({
            "name": "ax_session",
            "description": "Read or update this conversation's working context: the small snapshot of objective, facts, files, symbols, decisions, and open questions. Preflight shows it every turn, or one unchanged line when you pass its hash as known_context. Raw tool results stay in the conversation cache. Actions: get (default), add, update, compact, clear, fork, handoff. fork copies the notes to a new session. handoff stores the note you send as a new session. The old session stays readable and the graph cache is not copied.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["get", "add", "update", "compact", "clear", "fork", "handoff"], "description": "Default get. compact replaces the whole snapshot and requires every section. add appends. update replaces the sections you send. fork copies the notes. handoff starts a new session from the note you send." },
                    "objective": { "type": "string" },
                    "facts": { "type": "array", "items": { "type": "string" } },
                    "files": { "type": "array", "items": { "type": "string" } },
                    "symbols": { "type": "array", "items": { "type": "string" } },
                    "decisions": { "type": "array", "items": { "type": "string" } },
                    "open_questions": { "type": "array", "items": { "type": "string" } }
                }
            }
        }),
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
            "name": "ax_expand",
            "description": "Read a cached oversized MCP reply by id. offset and limit are character indexes. Default limit is 8000 characters, clamped to 12000.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Cache id from an [ax context cache] stub (cc_…)" },
                    "offset": { "type": "number", "description": "Character offset into the stored body (default 0)" },
                    "limit": { "type": "number", "description": "Max characters to return (default 8000, max 12000)" }
                },
                "required": ["id"]
            }
        }),
        json!({
            "name": "ax_cache_status",
            "description": "Context-cache and file-token-cache status. Safe to call at any time. Returns counts only — no stored bodies, paths, or file contents. Also writes the two lines to the MCP verbose log so Logging can group them.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session": { "type": "string", "description": "Session id. Omit to use the active Cursor session." }
                }
            }
        }),
        json!({
            "name": "ax_stash",
            "description": "Store arbitrary text (a chat slice or another tool result) in the context cache. Returns an id. The body is not echoed. Read it later with ax_expand.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "label": { "type": "string", "description": "Short label stored as the tool name (default stash)" }
                },
                "required": ["text"]
            }
        }),
        json!({
            "name": "ax_history",
            "description": "When did I change X? Dated agent turns (prompt, outcome, files, commits) and git commits that touched a file, symbol, or topic, newest first. Pass id for one turn's full outcome.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "File path, symbol name, or topic" },
                    "since": { "type": "string", "description": "YYYY-MM-DD" },
                    "limit": { "type": "number" },
                    "id": { "type": "string", "description": "Turn memory id from an earlier listing" }
                }
            }
        }),
        json!({
            "name": "ax_tool_economics",
            "description": "Pi session tool and model economics recorded by the Ax adapter. Estimates only. Does not include prompts or tool output.",
            "inputSchema": { "type": "object", "properties": { "sessionId": { "type": "string" } } }
        }),
        json!({
            "name": "ax_optimization_advice",
            "description": "Estimated avoidable context from recorded Pi tool calls. Advisory. Ax does not replace the tool.",
            "inputSchema": { "type": "object", "properties": {} }
        }),
        json!({
            "name": "ax_cost",
            "description": "Estimated model cost for a recorded Pi session, using the existing Ax price catalog. Unknown rates stay unknown.",
            "inputSchema": { "type": "object", "properties": { "sessionId": { "type": "string" } } }
        }),
        json!({
            "name": "ax_recall",
            "description": "Search durable project memories by free text; returns ranked IDs and summaries. Use id to retrieve the complete memory. Fresh memories outrank stale ones via confidence decay.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string" },
                    "id": { "type": "string", "description": "Stable memory ID for a complete body" },
                    "limit": { "type": "number" }
                },
                "anyOf": [{ "required": ["query"] }, { "required": ["id"] }]
            }
        }),
    ]
}

const NODE_MAX_LINES: u32 = 400;
const NODE_MAX_SOURCE_CHARS: u32 = 24_000;

/// `ax_node` is the graph's answer to "open this symbol": few matches, full
/// bodies, direct neighbours only.
fn node_opts_from_params(params: &Value) -> (String, ExploreOptions) {
    let name = ["name", "symbol", "query"]
        .iter()
        .find_map(|k| params.get(*k).and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let mut opts = explore_opts_from_params(params);
    opts.limit = opts.limit.or(Some(3));
    opts.depth = opts.depth.or(Some(1));
    opts.include_code = Some(true);
    opts.max_lines_per_snippet = opts.max_lines_per_snippet.or(Some(NODE_MAX_LINES));
    opts.max_source_chars = opts.max_source_chars.or(Some(NODE_MAX_SOURCE_CHARS));
    (name, opts)
}

async fn node(ax: &mut Ax, params: Value) -> Result<Value, String> {
    let (name, mut opts) = node_opts_from_params(&params);
    let signature_only = params.get("mode").and_then(|v| v.as_str()) == Some("signature");
    if signature_only {
        opts.max_lines_per_snippet = Some(SIGNATURE_LINES);
    }
    let mut result = ax.explore(&name, opts).await.map_err(|e| e.to_string())?;
    keep_exact_entries(&mut result, &name);
    let text = if signature_only {
        format_node_signatures(&result)
    } else {
        format_explore_text(&result).replacen("# Explore:", "# Node:", 1)
    };
    let nodes: Vec<&ax_types::Node> = result.entries.iter().map(|e| &e.node).collect();
    Ok(json!({
        "text": text,
        "query": result.query,
        "summary": result.summary,
        "blastRadius": result.blast_radius,
        "entries": result.entries,
        "nodes": nodes,
    }))
}

/// A fully qualified name or node id asks for one symbol; drop the fuzzy neighbours.
fn keep_exact_entries(result: &mut ax_types::ExploreResult, name: &str) {
    let exact = |n: &Node| n.qualified_name == name || n.id == name;
    if result.entries.iter().any(|e| exact(&e.node)) {
        result.entries.retain(|e| exact(&e.node));
        result.summary = format!("Found {} entry point(s) for '{name}'", result.entries.len());
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

const SIGNATURE_LINES: u32 = 3;

/// Location plus the first declaration lines per match; no body, no neighbours.
fn format_node_signatures(result: &ax_types::ExploreResult) -> String {
    let mut out = format!("# Node (signature): {}\n\n", result.query);
    if result.entries.is_empty() {
        out.push_str("No matching symbols.\n");
        return out;
    }
    for entry in &result.entries {
        out.push_str(&node_line(&entry.node));
        out.push('\n');
        if let Some(sig) = &entry.node.signature {
            out.push_str(&format!("  {sig}\n"));
        } else if let Some(src) = &entry.source {
            for line in src.lines().take(SIGNATURE_LINES as usize) {
                if line.starts_with("...(truncated") {
                    break;
                }
                out.push_str(&format!("  {line}\n"));
            }
        }
    }
    out.push_str("\nCall ax_node without mode for the full source, callers, and callees.\n");
    out
}

const CONTEXT_CACHE_LINE: &str = "## Context tools\n\nUse `ax_expand` with a cache ID and optional character offset to recover large replies; `ax_node` returns one symbol. Use `fresh: true` to rerun graph queries after context loss. Record decisions with `ax_session`; pass `known_context` only if you still hold that snapshot. After compaction or reset, retrieve missing rules with `ax_rules` and skills with `ax_skill`. Never assume an earlier delivery is still in the client context.";

pub fn server_instructions(has_policy: bool) -> String {
    let mut s = String::from("You have access to ax code intelligence tools (MCP).\n\n");
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
         Use ax_search for quick symbol lookup. ax_node returns the full numbered source of a symbol plus its direct callers and callees — use it instead of Read. Use ax_callers / ax_callees / ax_impact for focused graph queries.\n\n\
         Gaps: when a snippet says truncated, call ax_node on that symbol; when a reply ends with an [ax context cache] footer, continue with ax_expand. The index already stores the source: do not Read or Grep a file the graph returned. Read only files the graph does not cover (config, docs, generated output) or a file right before you edit it.\n\n\
         Whole-graph understanding: call ax_insights for Leiden communities (subsystems), god nodes (most-connected concepts), and surprising cross-community connections. Call ax_report for a full Markdown architecture report. Edges carry a confidence tag (extracted / inferred / ambiguous) and Markdown docs are indexed as Doc nodes linked to the code they reference.\n\n\
         Memory vault: when you make a durable decision, fix a tricky bug, or establish a convention, store it with ax_remember. Use ax_recall to search past decisions before re-deriving them. Relevant memories are auto-injected via ax_preflight.\n\n\
         Context cache: an oversized graph reply keeps its head inline and ends with a footer id; other oversized replies become a short stub with an id. Call ax_expand with that id to read the rest. ax_stash stores a chat slice or another tool result the same way. Call ax_cache_status at any time for context-cache and file-token-cache counts (no bodies). Preflight lists recent ids and memory titles, not bodies. This is not a dump of the memory vault.\n\n\
         Conversation cache: A repeated graph call in this conversation returns a short `[ax cache hit]` reference; the answer is already in your context or in ax_expand with its id. Read <ax_session_context> in preflight before searching again; pass fresh: true to force a new query. Editing a cited file invalidates the entry.\n\n\
         Working context: Record a durable fact, file, symbol, decision, or open question with `ax_session` (actions add, update, compact, clear). Preflight shows `<ax_working_context>`; pass its hash as known_context and an unchanged snapshot comes back as one line. Pass the `session` id from `<ax_chat>` to every ax call in this chat; a preflight without it starts a new chat. A changed index marks it stale; compact confirms the notes against the current index.\n\n\
         Ops (prefer MCP — do NOT shell ax CLI when MCP is connected):\n\
         - ax_sync after local edits that should refresh the graph\n\
         - ax_index with force=true for a full rebuild\n\
         - ax_lsp action=status|enrich for language-server Exact edges\n\
         - ax_ship mode=evaluate|ci for the quality gate (never process::exit)\n\
         - ax_policy_index to refresh rules/skills from .ax/policy/\n\
         Shell CLI only when MCP is unreachable (DEGRADED) or for install/upgrade/web/share/ship --watch.\n\n\
         projectPath must identify this MCP server's project; connect a separate server for another monorepo project. Prefer ax over grep/read for code structure.",
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
        for expected in [
            "ax_preflight",
            "ax_policy_capture",
            "ax_guard",
            "ax_rules",
            "ax_skill",
        ] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
    }

    #[test]
    fn preflight_tool_schema_includes_project_path() {
        let schema = preflight_tool()["inputSchema"].clone();
        let props = schema["properties"].clone();
        assert!(props.get("projectPath").is_some());
        assert!(props.get("prompt").is_some());
        assert!(
            schema.get("required").is_none(),
            "session-start clients call ax_preflight with no args; required prompt causes Cursor validation_failed"
        );
    }

    #[test]
    fn string_array_accepts_single_string() {
        assert_eq!(
            string_array(Some(&json!("C:\\\\gary\\\\Contoso\\\\src\\\\a.cs"))),
            vec!["C:\\\\gary\\\\Contoso\\\\src\\\\a.cs"]
        );
        assert_eq!(
            string_array(Some(&json!(["a.rs", "b.rs"]))),
            vec!["a.rs", "b.rs"]
        );
    }

    #[test]
    fn server_instructions_prefer_mcp_ops_over_shell() {
        let s = server_instructions(true);
        assert!(s.contains("ax_sync"));
        assert!(s.contains("ax_lsp"));
        assert!(s.contains("ax_ship"));
        assert!(s.contains("ax_policy_index"));
        assert!(
            s.contains("do NOT shell")
                || s.contains("Do NOT shell")
                || s.contains("Shell CLI only")
        );
    }

    #[test]
    fn server_instructions_always_mention_directive_capture() {
        // Even with no policy, agents must be told to capture directives.
        let s = server_instructions(false);
        assert!(s.contains("ax_preflight"));
        assert!(s.contains("Directive capture"));
        assert!(s.contains("ax_policy_capture"));
    }

    /// The unfiltered catalog, exactly as `list_tools` assembles it.
    fn full_catalog() -> Vec<Value> {
        let mut tools = vec![
            explore_tool(),
            preflight_tool(),
            capture_tool(),
            rules_tool(),
            skill_tool(),
            guard_tool(),
        ];
        tools.extend(extra_tools());
        tools
    }

    #[test]
    fn default_advertises_graph_reads_and_hides_heavy_ops() {
        let mut tools = full_catalog();
        crate::tool_filter::filter_tools_list_with(&mut tools, None);
        let names: Vec<String> = tools
            .iter()
            .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
            .collect();
        for expected in [
            "ax_explore",
            "ax_preflight",
            "ax_policy_capture",
            "ax_search",
            "ax_node",
            "ax_callers",
            "ax_callees",
            "ax_impact",
            "ax_context",
            "ax_affected",
            "ax_insights",
            "ax_report",
            "ax_status",
            "ax_sync",
        ] {
            assert!(names.contains(&expected.to_string()), "missing {expected}");
        }
        // The lean filter must still be doing real work.
        for hidden in crate::tool_filter::GATED_BY_DESIGN {
            assert!(
                !names.contains(&hidden.to_string()),
                "{hidden} must stay opt-in by default"
            );
        }
        assert!(!names.contains(&"ax_files".to_string()));
    }

    /// A typo in `CORE_TOOLS` would silently advertise nothing. Pin every core
    /// name to a tool that actually exists in the catalog.
    #[test]
    fn every_core_tool_exists_in_the_catalog() {
        let catalog: Vec<String> = full_catalog()
            .iter()
            .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
            .collect();
        for name in crate::tool_filter::CORE_TOOLS {
            assert!(
                catalog.contains(&name.to_string()),
                "CORE_TOOLS names {name}, which is not a real tool (typo?)"
            );
        }
        for name in crate::tool_filter::GATED_BY_DESIGN {
            assert!(
                catalog.contains(&name.to_string()),
                "GATED_BY_DESIGN names {name}, which is not a real tool (typo?)"
            );
        }
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
        // Opt in to two gated ops by short name; the third must stay hidden.
        crate::tool_filter::filter_tools_list_with(
            &mut tools,
            crate::tool_filter::resolve_tool_allowlist_from(Some("lsp,ship")),
        );
        let names: Vec<String> = tools
            .iter()
            .filter_map(|t| t["name"].as_str().map(|s| s.to_string()))
            .collect();
        assert!(names.contains(&"ax_lsp".to_string()));
        assert!(names.contains(&"ax_ship".to_string()));
        assert!(!names.contains(&"ax_diagnostics".to_string()));
        // Core is never gated by the allowlist.
        assert!(names.contains(&"ax_explore".to_string()));
        assert!(names.contains(&"ax_search".to_string()));
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
    fn node_opts_default_to_full_source_for_few_symbols() {
        let (name, opts) = node_opts_from_params(&json!({ "name": "choose_stacks_for_init" }));
        assert_eq!(name, "choose_stacks_for_init");
        assert_eq!(opts.limit, Some(3));
        assert_eq!(opts.depth, Some(1));
        assert_eq!(opts.include_code, Some(true));
        assert_eq!(opts.max_lines_per_snippet, Some(NODE_MAX_LINES));
        assert_eq!(opts.max_source_chars, Some(NODE_MAX_SOURCE_CHARS));
        const { assert!(NODE_MAX_LINES >= 400 && NODE_MAX_SOURCE_CHARS >= 20_000) };
    }

    #[test]
    fn node_opts_accept_symbol_alias_and_overrides() {
        let (name, opts) = node_opts_from_params(&json!({
            "symbol": "detect", "limit": 1, "maxLinesPerSnippet": 50, "maxSourceChars": 999
        }));
        assert_eq!(name, "detect");
        assert_eq!(opts.limit, Some(1));
        assert_eq!(opts.max_lines_per_snippet, Some(50));
        assert_eq!(opts.max_source_chars, Some(999));
    }

    #[test]
    fn node_schema_says_it_returns_source() {
        let tool = extra_tools()
            .into_iter()
            .find(|t| t["name"] == "ax_node")
            .unwrap();
        let desc = tool["description"].as_str().unwrap();
        assert!(desc.contains("full numbered source"), "{desc}");
        assert!(tool["inputSchema"]["properties"]
            .get("maxLinesPerSnippet")
            .is_some());
    }

    #[test]
    fn server_instructions_steer_gaps_back_to_the_graph() {
        let s = server_instructions(true);
        assert!(s.contains("ax_node returns the full numbered source"));
        assert!(s.contains("do not Read"));
    }

    #[test]
    fn diagnostics_from_params_empty_without_diagnostics_key() {
        assert!(diagnostics_from_params(&json!({})).is_empty());
    }

    const NOW_MS: i64 = 1_790_000_000_000;

    /// An initialised project with one turn memory that changed `src/a.rs`.
    async fn project_with_turn(outcome: &str) -> (tempfile::TempDir, Ax) {
        let dir = tempfile::tempdir().unwrap();
        let ax = Ax::init(dir.path()).await.unwrap();
        let record = ax_memory::TurnRecord {
            id: "turn-a".into(),
            title: "Fix the dropdown".into(),
            body: format!(
                "Fix the dropdown\n\nFiles: src/a.rs{}{outcome}",
                ax_memory::OUTCOME_MARKER
            ),
            files: vec!["src/a.rs".into()],
        };
        ax_memory::save_turn(ax.db_pool(), &record, NOW_MS)
            .await
            .unwrap();
        (dir, ax)
    }

    async fn preflight_inject(ax: &mut Ax, params: Value) -> String {
        let out = ToolHandler::call_tool(ax, "ax_preflight", params)
            .await
            .unwrap();
        out["inject"].as_str().unwrap_or_default().to_string()
    }

    #[tokio::test]
    async fn r1_preflight_injects_turns_that_changed_an_open_file() {
        let (dir, mut ax) = project_with_turn("Fixed it, 11 languages load.").await;
        let open = ax.project_root().join("src/a.rs");
        let inject = preflight_inject(
            &mut ax,
            json!({ "prompt": "zzz", "files": [open.to_string_lossy()] }),
        )
        .await;
        assert!(inject.contains("<ax_turn_history>"), "{inject}");
        assert!(inject.contains("Fix the dropdown"), "{inject}");
        assert!(inject.contains("Fixed it, 11 languages load."), "{inject}");

        let relative =
            preflight_inject(&mut ax, json!({ "prompt": "zzz", "files": ["src/a.rs"] })).await;
        assert!(relative.contains("<ax_turn_history>"), "{relative}");

        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.rs"), "fn a() {}\n").unwrap();
        let uncanonical = dir.path().join("src/a.rs");
        let inject = preflight_inject(
            &mut ax,
            json!({ "prompt": "zzz", "files": [uncanonical.to_string_lossy()] }),
        )
        .await;
        assert!(inject.contains("<ax_turn_history>"), "{inject}");
    }

    #[tokio::test]
    async fn r2_preflight_without_related_turns_has_no_block() {
        let (_dir, mut ax) = project_with_turn("Done.").await;
        let inject = preflight_inject(
            &mut ax,
            json!({ "prompt": "zzz", "files": ["src/other.rs"] }),
        )
        .await;
        assert!(!inject.contains("<ax_turn_history>"), "{inject}");
        assert!(!inject.contains("ax_history"), "{inject}");
    }

    #[tokio::test]
    async fn h2_history_question_points_the_agent_to_ax_history() {
        let (_dir, mut ax) = project_with_turn("Done.").await;
        let inject = preflight_inject(
            &mut ax,
            json!({ "prompt": "wanneer heb ik de review-taal aangepast?" }),
        )
        .await;
        assert!(inject.contains("<ax_history_hint>"), "{inject}");
        assert!(inject.contains("call ax_history"), "{inject}");
    }

    #[tokio::test]
    async fn ax_history_lists_turns_and_gives_one_in_full_by_id() {
        let outcome = "o".repeat(5_000);
        let (_dir, mut ax) = project_with_turn(&outcome).await;
        let listed = ToolHandler::call_tool(&mut ax, "ax_history", json!({ "query": "src/a.rs" }))
            .await
            .unwrap();
        let text = listed["text"].as_str().unwrap();
        assert!(
            text.contains("turn-a") && text.contains("Fix the dropdown"),
            "{text}"
        );
        assert!(
            text.contains(&"o".repeat(600)) && !text.contains(&"o".repeat(601)),
            "cut at 600"
        );
        assert_eq!(listed["count"], 1);

        let full = ToolHandler::call_tool(&mut ax, "ax_history", json!({ "id": "turn-a" }))
            .await
            .unwrap();
        assert!(full["text"].as_str().unwrap().contains(&outcome));

        let missing = ToolHandler::call_tool(&mut ax, "ax_history", json!({ "id": "nope" }))
            .await
            .unwrap();
        assert!(
            missing["text"].as_str().unwrap().contains("No turn memory"),
            "{missing}"
        );
    }

    #[tokio::test]
    async fn ax_history_since_filters_and_rejects_bad_dates() {
        let (_dir, mut ax) = project_with_turn("Done.").await;
        let later = ToolHandler::call_tool(
            &mut ax,
            "ax_history",
            json!({ "query": "src/a.rs", "since": "2026-09-23" }),
        )
        .await
        .unwrap();
        assert_eq!(later["count"], 0, "{later}");
        let earlier = ToolHandler::call_tool(
            &mut ax,
            "ax_history",
            json!({ "query": "src/a.rs", "since": "2026-09-01" }),
        )
        .await
        .unwrap();
        assert_eq!(earlier["count"], 1, "{earlier}");

        let bad = ToolHandler::call_tool(
            &mut ax,
            "ax_history",
            json!({ "query": "x", "since": "24-09" }),
        )
        .await;
        assert!(bad.unwrap_err().contains("YYYY-MM-DD"));
        let empty = ToolHandler::call_tool(&mut ax, "ax_history", json!({})).await;
        assert!(empty.unwrap_err().contains("query"));
    }

    #[tokio::test]
    async fn ax_history_is_in_the_default_catalog() {
        let names = tool_names(&ToolHandler::list_tools(false).await);
        assert!(names.contains(&"ax_history".to_string()), "{names:?}");
    }

    #[test]
    fn server_text_and_seeds_share_the_conversation_cache_sentence() {
        for has_policy in [true, false] {
            let text = server_instructions(has_policy);
            assert!(
                text.contains(ax_policy::CONVERSATION_CACHE_SENTENCE),
                "instructions({has_policy})"
            );
            assert!(text.contains("<ax_session_context>") && text.contains("fresh: true"));
        }
        assert!(CONTEXT_CACHE_LINE.contains("After compaction or reset"));
        assert!(CONTEXT_CACHE_LINE.contains("ax_expand"));
        assert!(CONTEXT_CACHE_LINE.contains("known_context"));
        assert!(CONTEXT_CACHE_LINE.contains("fresh: true"));
    }

    #[tokio::test]
    async fn cacheable_tools_advertise_fresh_and_others_do_not() {
        let listed = ToolHandler::list_tools(true).await;
        let tools = listed["tools"].as_array().unwrap();
        let schema_has_fresh = |name: &str| {
            tools
                .iter()
                .find(|t| t["name"] == name)
                .map(|t| t["inputSchema"]["properties"].get("fresh").is_some())
        };
        assert_eq!(schema_has_fresh("ax_node"), Some(true));
        assert_eq!(schema_has_fresh("ax_explore"), Some(true));
        assert_eq!(schema_has_fresh("ax_search"), Some(true));
        assert_eq!(schema_has_fresh("ax_preflight"), Some(false));
        assert_eq!(schema_has_fresh("ax_guard"), Some(false));
    }

    #[tokio::test]
    async fn session_is_advertised_where_chat_state_is_read() {
        let listed = ToolHandler::list_tools(true).await;
        let tools = listed["tools"].as_array().unwrap();
        let session_type = |name: &str| {
            tools.iter().find(|t| t["name"] == name).map(|t| {
                t["inputSchema"]["properties"]["session"]["type"].as_str() == Some("string")
            })
        };
        for name in [
            "ax_preflight",
            "ax_session",
            "ax_durable",
            "ax_node",
            "ax_explore",
            "ax_search",
            "ax_callers",
        ] {
            assert_eq!(session_type(name), Some(true), "{name}");
        }
        assert_eq!(session_type("ax_guard"), Some(false));
        assert_eq!(session_type("ax_sync"), Some(false));
        let preflight = tools.iter().find(|t| t["name"] == "ax_preflight").unwrap();
        assert_eq!(
            preflight["inputSchema"]["properties"]["known_context"]["type"],
            "string"
        );
    }
}
