//! Smoke: new competitive-gap MCP tools against the local ax index.
//! Run: cargo test -p ax-mcp --test new_tools_smoke -- --nocapture

use ax_mcp::tools::ToolHandler;
use serde_json::json;

#[tokio::test]
async fn lean_tools_list_hides_extras() {
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
    assert!(!names.contains(&"ax_cycles"), "cycles must be lean-hidden by default");
    assert!(!names.contains(&"ax_search"), "search must be lean-hidden by default");
}

#[tokio::test]
async fn cycles_api_path_handlers_work() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().expect("repo root");
    let mut ax = ax_core::Ax::open(&root).await.expect("open ax project");

    let cycles = ToolHandler::call_tool(&mut ax, "ax_cycles", json!({ "limit": 2 }))
        .await
        .expect("ax_cycles");
    let text = cycles["text"].as_str().unwrap_or("");
    assert!(
        text.contains("Call-graph cycles") || text.contains("No call-graph cycles"),
        "unexpected cycles text: {text}"
    );

    let api = ToolHandler::call_tool(
        &mut ax,
        "ax_api",
        json!({ "module": "ax-mcp", "limit": 3 }),
    )
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
}
