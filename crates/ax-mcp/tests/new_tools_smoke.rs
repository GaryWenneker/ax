//! Smoke: new competitive-gap MCP tools against the local ax index.
//! Run: cargo test -p ax-mcp --test new_tools_smoke -- --nocapture

use ax_mcp::tools::ToolHandler;
use serde_json::json;

#[tokio::test]
async fn default_tools_list_shows_graph_surface_and_gates_heavy_ops() {
    std::env::remove_var("AX_MCP_TOOLS");
    let listed = ToolHandler::list_tools(true).await;
    let names: Vec<&str> = listed["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();
    assert!(names.contains(&"ax_explore"));
    assert!(names.contains(&"ax_preflight"));
    // The graph read surface the CRITICAL policy rules tell agents to prefer
    // over Grep/Read must be visible in the default catalog.
    for expected in [
        "ax_search",
        "ax_cycles",
        "ax_node",
        "ax_impact",
        "ax_status",
    ] {
        assert!(
            names.contains(&expected),
            "{expected} must be advertised by default"
        );
    }
    // Heavy ops stay opt-in via AX_MCP_TOOLS.
    for hidden in [
        "ax_ship",
        "ax_lsp",
        "ax_index",
        "ax_diagnostics",
        "ax_policy_index",
    ] {
        assert!(
            !names.contains(&hidden),
            "{hidden} must stay opt-in by default"
        );
    }
}

#[tokio::test]
async fn cycles_api_path_handlers_work() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/server.rs"),
        "pub fn call_tool_and_wrap() -> u64 { estimate_savings() }\n\
         pub fn estimate_savings() -> u64 { 1 }\n\
         pub fn find_call_cycles() -> bool { call_graph_has_cycle() }\n\
         pub fn call_graph_has_cycle() -> bool { false }\n",
    )
    .unwrap();
    let mut ax = ax_core::Ax::init(root).await.expect("init ax project");
    ax.index_all(
        ax_extraction::orchestrator::IndexOptions {
            quiet: true,
            ..Default::default()
        },
        None,
    )
    .await
    .expect("index project");

    let cycles = ToolHandler::call_tool(&mut ax, "ax_cycles", json!({ "limit": 2 }))
        .await
        .expect("ax_cycles");
    let text = cycles["text"].as_str().unwrap_or("");
    assert!(
        text.contains("Call-graph cycles") || text.contains("No call-graph cycles"),
        "unexpected cycles text: {text}"
    );

    let api = ToolHandler::call_tool(&mut ax, "ax_api", json!({ "module": "ax-mcp", "limit": 3 }))
        .await
        .expect("ax_api");
    let api_text = api["text"].as_str().unwrap_or("");
    assert!(
        api_text.contains("API surface") || api_text.contains("No exported"),
        "unexpected api text: {api_text}"
    );

    let path = ToolHandler::call_tool(
        &mut ax,
        "ax_path",
        json!({
            "from": "find_call_cycles",
            "to": "call_graph_has_cycle"
        }),
    )
    .await
    .expect("ax_path");
    assert!(path.get("text").is_some(), "ax_path missing text");

    let hop = ToolHandler::call_tool(
        &mut ax,
        "ax_path",
        json!({ "from": "call_tool_and_wrap", "to": "estimate_savings" }),
    )
    .await
    .expect("ax_path hop");
    let hop_text = hop["text"].as_str().unwrap_or("");
    assert!(hop_text.starts_with("Path (1 hops)"), "{hop_text}");
    assert!(hop_text.contains("call_tool_and_wrap"), "{hop_text}");
    assert!(hop_text.contains("estimate_savings"), "{hop_text}");
    assert!(hop_text.contains("src/server.rs"), "{hop_text}");
}
