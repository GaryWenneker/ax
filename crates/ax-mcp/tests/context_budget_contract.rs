use ax_core::Ax;
use ax_mcp::ToolHandler;
use serde_json::json;

#[tokio::test]
async fn budget_counts_footers_and_structured_metadata_and_reports_required_minimum() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let mut ax = Ax::init(root).await.unwrap();
    let rules = root.join(".agents/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(rules.join("required.mdc"), "---\nid: required\nlevel: CRITICAL\nalwaysApply: true\n---\nNever disclose another project's context.\n").unwrap();
    std::fs::write(
        rules.join("extra.mdc"),
        format!(
            "---\nid: extra\nlevel: INFO\ntriggers: [parser]\n---\n{}",
            "optional parser detail ".repeat(2000)
        ),
    )
    .unwrap();
    ax_policy::index_policy(ax.db_pool(), root, true)
        .await
        .unwrap();
    std::fs::write(root.join("ax.json"), r#"{"context":{"budgetTokens":1}}"#).unwrap();
    let out = ToolHandler::call_tool(&mut ax, "ax_preflight", json!({"prompt":"fix parser"}))
        .await
        .unwrap();
    assert!(out["inject"]
        .as_str()
        .unwrap()
        .contains("Never disclose another project's context."));
    assert!(!out["inject"]
        .as_str()
        .unwrap()
        .contains("optional parser detail"));
    assert!(out["contextBudget"]["requiredMinimum"].as_u64().unwrap() > 1);
    assert_eq!(out["contextBudget"]["overBudget"], true);
    assert!(out["contextBudget"]["included"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v != "rule:extra"));
}

#[test]
fn budget_projection_is_exact_across_limits_and_optional_block_sizes() {
    use ax_usage::{ContextBlock, ContextClass};
    for limit in [1, 100, 300, 500, 1000, 3000] {
        for optional_size in [0, 1, 50, 500] {
            let blocks = vec![
                ContextBlock {
                    id: "required".into(),
                    class: ContextClass::HardRequired,
                    text: "Keep all required constraints complete.".into(),
                },
                ContextBlock {
                    id: "detail".into(),
                    class: ContextClass::HighValue,
                    text: "optional detail ".repeat(optional_size),
                },
            ];
            let out = ax_mcp::context_budget::finish(
                json!({"project":{"path":"a"}, "session":{"id":"s"}}),
                &blocks,
                Some(limit),
            )
            .unwrap();
            let measured = ax_mcp::context_budget::projection_tokens(&out);
            assert_eq!(out["contextBudget"]["tokens"], measured);
            assert!(out["inject"]
                .as_str()
                .unwrap()
                .contains("Keep all required constraints complete."));
            if out["contextBudget"]["overBudget"] == false {
                assert!(measured <= limit as usize);
            } else {
                assert!(out["contextBudget"]["requiredMinimum"].as_u64().unwrap() > limit as u64);
            }
        }
    }
}

#[tokio::test]
async fn omitted_optional_rule_remains_eligible_after_budget_increases() {
    let dir = tempfile::tempdir().unwrap();
    let _ax = ax_core::Ax::init(dir.path()).await.unwrap();
    let rules = dir.path().join(".agents/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(rules.join("optional.mdc"), "---\nid: optional\nlevel: INFO\ntriggers: [architecture]\n---\nOptional eligible content.\n").unwrap();
    std::fs::write(
        dir.path().join("ax.json"),
        r#"{"context":{"budgetTokens":1}}"#,
    )
    .unwrap();
    let mut engine = ax_mcp::McpEngine::with_project_root(dir.path().to_path_buf());
    let args = serde_json::json!({"session":"budget-test", "prompt":"architecture"});
    let first = ax_mcp::server::handle_request(
        &mut engine,
        "tools/call",
        serde_json::json!({"name":"ax_preflight", "arguments":args}),
    )
    .await
    .result
    .unwrap();
    assert!(!first["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Optional eligible content."));
    std::fs::write(
        dir.path().join("ax.json"),
        r#"{"context":{"budgetTokens":10000}}"#,
    )
    .unwrap();
    let second = ax_mcp::server::handle_request(
        &mut engine,
        "tools/call",
        serde_json::json!({"name":"ax_preflight", "arguments":args}),
    )
    .await
    .result
    .unwrap();
    assert!(second["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("Optional eligible content."));
}
