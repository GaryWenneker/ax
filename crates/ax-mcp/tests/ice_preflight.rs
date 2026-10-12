//! ICE preflight acceptance: exact source, policy budget and negative prompts.
use ax_core::Ax;
use ax_extraction::orchestrator::IndexOptions;
use ax_mcp::ToolHandler;
use serde_json::json;

#[tokio::test]
async fn preflight_returns_exact_indexed_manifest_line() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let body = (1..=325).map(|n| format!("\"key{n}\": \"value{n}\","))
        .collect::<Vec<_>>().join("\n");
    std::fs::write(root.join("package.json"), &body).unwrap();
    let mut ax = Ax::init(root).await.unwrap();
    ax.index_all(IndexOptions::default(), None).await.unwrap();
    let result = ToolHandler::call_tool(&mut ax, "ax_preflight", json!({
        "prompt": "Wat staat er in package.json op regel 309?",
        "contextMode": "compact"
    })).await.unwrap();
    assert_eq!(result["sourceContext"]["status"], "matched");
    assert_eq!(result["sourceContext"]["evidence"][0]["path"], "package.json");
    assert_eq!(result["sourceContext"]["evidence"][0]["startLine"], 309);
    assert!(result["inject"].as_str().unwrap().contains("309\t\"key309\": \"value309\","));
}

#[tokio::test]
async fn preflight_keeps_required_policy_when_source_does_not_fit_budget() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("package.json"), "{\n\"scripts\":{}\n}\n").unwrap();
    std::fs::write(root.join("ax.json"), r#"{"context":{"budgetTokens":1}}"#).unwrap();
    let mut ax = Ax::init(root).await.unwrap();
    let rules = root.join(".agents/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(rules.join("critical.mdc"), "---\nid: critical\nlevel: CRITICAL\nalwaysApply: true\n---\nNever bypass project authorization.\n").unwrap();
    ax.index_all(IndexOptions::default(), None).await.unwrap();
    ax_policy::index_policy(ax.db_pool(), root, true).await.unwrap();
    let result = ToolHandler::call_tool(&mut ax, "ax_preflight", json!({
        "prompt": "Wat staat er in package.json op regel 2?",
        "contextMode": "compact"
    })).await.unwrap();
    assert!(result["inject"].as_str().unwrap().contains("Never bypass project authorization."));
    assert!(!result["inject"].as_str().unwrap().contains("2\t\"scripts\""));
    assert_eq!(result["contextBudget"]["overBudget"], true);
}

#[tokio::test]
async fn irrelevant_prompt_never_injects_source() {
    let tmp = tempfile::tempdir().unwrap();
    let mut ax = Ax::init(tmp.path()).await.unwrap();
    let result = ToolHandler::call_tool(&mut ax, "ax_preflight", json!({
        "prompt": "Wat is het weer morgen?", "contextMode": "compact"
    })).await.unwrap();
    assert_eq!(result["sourceContext"]["status"], "disabled");
}

#[tokio::test]
async fn approved_business_rule_supplies_search_terms_without_model() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    std::fs::write(root.join("auth.ts"),
        "export function refreshToken(): boolean {\n    return true;\n}\n").unwrap();
    let mut ax = Ax::init(root).await.unwrap();
    let rules = root.join(".agents/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(rules.join("auth-context.mdc"),
        "---\nid: auth-context\nlevel: INFO\ntriggers: [inloggen]\ncontextTerms: [refreshToken]\n---\nPrefer the indexed authentication implementation.\n").unwrap();
    ax.index_all(IndexOptions::default(), None).await.unwrap();
    ax_policy::index_policy(ax.db_pool(), root, true).await.unwrap();
    let result = ToolHandler::call_tool(&mut ax, "ax_preflight", json!({
        "prompt": "Waarom kan ik niet inloggen?",
        "contextMode": "compact"
    })).await.unwrap();
    assert_eq!(result["sourceContext"]["status"], "matched");
    assert_eq!(result["sourceContext"]["evidence"][0]["path"], "auth.ts");
    assert!(result["sourceContext"]["evidence"][0]["reason"].as_str().unwrap()
        .contains("configured policy term"));
}
