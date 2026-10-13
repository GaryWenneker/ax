//! The withdrawn writing-style experiment cannot change the standard MCP contract.
use ax_core::Ax;
use ax_mcp::{server::handle_request, McpEngine, ToolHandler};
use serde_json::{json, Value};

const POLICY: &str = "If role is Reader, deny writes unless an explicit grant exists.\nThe finding is unverified. Preserve `reader_id`. Unicode: café, Δ, 🪓.";
const EXPERIMENT_GUIDANCE: &str = "Write explanations in clear technical English.";

async fn fixture() -> (tempfile::TempDir, Ax) {
    let dir = tempfile::tempdir().unwrap();
    let ax = Ax::init(dir.path()).await.unwrap();
    let rules = dir.path().join(".agents/rules");
    tokio::fs::create_dir_all(&rules).await.unwrap();
    tokio::fs::write(
        rules.join("standard-contract.mdc"),
        format!("---\nid: standard-contract\nlevel: CRITICAL\nalwaysApply: true\n---\n{POLICY}\n"),
    )
    .await
    .unwrap();
    ax_policy::index_policy(ax.db_pool(), dir.path(), true)
        .await
        .unwrap();
    (dir, ax)
}

fn assert_standard(out: &Value, text: &str) {
    assert!(out.get("presentation").is_none());
    assert!(!text.contains(EXPERIMENT_GUIDANCE));
    assert!(text.contains("Apply matched rules and skills before editing."));
}

#[tokio::test]
async fn preflight_schema_has_no_writing_style_selector() {
    let tools = ToolHandler::list_tools(false).await;
    let tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "ax_preflight")
        .unwrap();
    assert!(tool["inputSchema"]["properties"].get("textStyle").is_none());
}

#[tokio::test]
async fn raw_preflight_keeps_standard_output_with_obsolete_arguments() {
    let (_dir, mut ax) = fixture().await;
    for value in [
        json!("standard"),
        json!("ste-inspired"),
        json!("unknown"),
        json!(null),
        json!(42),
    ] {
        let out = ToolHandler::call_tool(
            &mut ax,
            "ax_preflight",
            json!({"session":"raw-standard", "textStyle":value, "context_reset":true}),
        )
        .await
        .unwrap();
        let text = out["inject"].as_str().unwrap();
        assert_standard(&out, text);
        assert!(text.contains(POLICY));
        assert_eq!(
            out["instruction"],
            "Apply CRITICAL rules before editing. If a skill matched, follow its workflow."
        );
    }
}

#[tokio::test]
async fn wire_preflight_preserves_cold_warm_reset_delivery() {
    let (dir, _ax) = fixture().await;
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    for (reset, expected_source, value) in [
        (false, true, json!("ste-inspired")),
        (false, false, json!(null)),
        (true, true, json!("unknown")),
    ] {
        let out = handle_request(
            &mut engine,
            "tools/call",
            json!({"name":"ax_preflight", "arguments": {
                "session":"wire-standard", "textStyle":value, "context_reset":reset
            }}),
        )
        .await
        .result
        .unwrap();
        assert_eq!(out["isError"], false);
        let text = out["content"][0]["text"].as_str().unwrap();
        assert_standard(&out["structuredContent"], text);
        assert_eq!(text.contains(POLICY), expected_source);
    }
}

#[tokio::test]
async fn embedded_preflight_ignores_obsolete_style_validation() {
    let (dir, _ax) = fixture().await;
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    for value in [json!("ste-inspired"), json!(null), json!("unknown")] {
        let out = ax_mcp::call_tool(
            &mut engine,
            "ax_preflight",
            json!({"session":"embedded-standard", "textStyle":value, "context_reset":true}),
        )
        .await
        .unwrap();
        let text = out["inject"].as_str().unwrap();
        assert_standard(&out, text);
        assert!(text.contains(POLICY));
    }
}
