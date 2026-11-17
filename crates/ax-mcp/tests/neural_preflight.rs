use ax_core::Ax;
use ax_mcp::ToolHandler;
use serde_json::{json, Value};
use std::path::Path;

/// A fixed chat id, so two preflights differ only by what the project configures.
const SESSION: &str = "axs_0123456789abcdef";

async fn preflight(root: &Path, prompt: &str) -> Value {
    let mut ax = Ax::init(root).await.unwrap();
    ToolHandler::call_tool(
        &mut ax,
        "ax_preflight",
        json!({ "prompt": prompt, "session": SESSION }),
    )
    .await
    .unwrap()
}

async fn write_policy(root: &Path) {
    let rules = root.join(".agents/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(
        rules.join("required.mdc"),
        "---\nid: required\nlevel: CRITICAL\nalwaysApply: true\n---\nKeep project context private.\n",
    )
    .unwrap();
    std::fs::write(
        rules.join("parser.mdc"),
        "---\nid: parser\nlevel: INFO\ntriggers: [parser]\n---\nParser errors carry the line number.\n",
    )
    .unwrap();
    let ax = Ax::init(root).await.unwrap();
    ax_policy::index_policy(ax.db_pool(), root, true)
        .await
        .unwrap();
}

#[tokio::test]
async fn absent_and_disabled_preflights_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_policy(root).await;
    let absent = preflight(root, "fix the parser").await;
    std::fs::write(root.join("ax.json"), r#"{"neural":{"enabled":false}}"#).unwrap();
    let disabled = preflight(root, "fix the parser").await;

    assert_eq!(absent["inject"], disabled["inject"]);
    assert_eq!(absent["contextBudget"], disabled["contextBudget"]);
    assert!(absent.get("neuralRank").is_none(), "{absent}");
    assert_eq!(absent["contextBudget"]["neural"]["kept"], "ranked");
    assert_eq!(absent["contextBudget"]["neural"]["omitted"], "over-budget");
}

#[tokio::test]
async fn each_preflight_adds_to_the_rank_totals() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_policy(root).await;
    let mut ax = Ax::init(root).await.unwrap();
    let project = ax_neural::dispatch::project_id(ax.project_root());
    for expected in 1..=2 {
        ToolHandler::call_tool(
            &mut ax,
            "ax_preflight",
            json!({ "prompt": "fix the parser", "session": SESSION }),
        )
        .await
        .unwrap();
        let mut preflights = 0;
        for _ in 0..250 {
            preflights = ax_neural::store::rank_totals(ax.db_pool(), &project)
                .await
                .unwrap()
                .map_or(0, |totals| totals.preflights);
            if preflights == expected {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        assert_eq!(preflights, expected);
    }
}

/// A CLI process ends its runtime as soon as preflight returns.
#[test]
fn rank_totals_are_written_before_preflight_returns() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let runtime = || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
    };
    runtime().block_on(async {
        write_policy(&root).await;
        let mut ax = Ax::init(&root).await.unwrap();
        ToolHandler::call_tool(
            &mut ax,
            "ax_preflight",
            json!({ "prompt": "fix the parser", "session": SESSION }),
        )
        .await
        .unwrap();
    });
    let preflights = runtime().block_on(async {
        let ax = Ax::init(&root).await.unwrap();
        let project = ax_neural::dispatch::project_id(ax.project_root());
        ax_neural::store::rank_totals(ax.db_pool(), &project)
            .await
            .unwrap()
            .map_or(0, |totals| totals.preflights)
    });
    assert_eq!(preflights, 1);
}

/// Two successful parser runs, so prediction is confident about their skill and memory.
async fn record_parser_feedback(root: &Path) {
    let ax = Ax::init(root).await.unwrap();
    for execution in ["e1", "e2"] {
        ax_neural::dispatch::handle(
            ax.project_root(),
            "feedback",
            &json!({
                "executionId": execution,
                "taskCategory": "parser",
                "outcome": "success",
                "skillIds": ["parse-errors"],
                "contextIds": ["memory:parser-lines"]
            }),
            Some(ax.db_pool()),
        )
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn neural_tools_are_answered_through_the_mcp_tool_handler() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("ax.json"),
        r#"{"neural":{"learning":{"feedback_collection":true}}}"#,
    )
    .unwrap();
    let mut ax = Ax::init(root).await.unwrap();

    let stored = ToolHandler::call_tool(
        &mut ax,
        "ax_neural_feedback",
        json!({ "executionId": "e1", "taskCategory": "parser", "outcome": "success" }),
    )
    .await
    .unwrap();
    assert_eq!(stored["stored"], true);
    let status = ToolHandler::call_tool(&mut ax, "ax_neural_status", json!({}))
        .await
        .unwrap();
    assert_eq!(
        status["metrics"]["feedbackCount"],
        json!({ "value": 1, "kind": "measured" })
    );
    let error = ToolHandler::call_tool(&mut ax, "ax_neural_export", json!({}))
        .await
        .unwrap_err();
    assert!(error.contains("unknown neural command"), "{error}");
}

#[tokio::test]
async fn the_budget_can_drop_the_predicted_block() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("ax.json"),
        r#"{"context":{"budgetTokens":1},"neural":{"prediction":{"enabled":true},"learning":{"feedback_collection":true}}}"#,
    )
    .unwrap();
    record_parser_feedback(root).await;

    let reply = preflight(root, "fix the parser").await;
    let omitted = reply["contextBudget"]["omitted"].to_string();
    assert!(omitted.contains("neural:predicted"), "{omitted}");
    assert!(!reply["inject"].as_str().unwrap().contains("## Predicted"));
}

#[tokio::test]
async fn predicted_block_appears_only_when_prediction_is_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("ax.json"),
        r#"{"context":{"budgetTokens":100000},"neural":{"prediction":{"enabled":true},"learning":{"feedback_collection":true}}}"#,
    )
    .unwrap();
    record_parser_feedback(root).await;

    let on = preflight(root, "fix the parser").await;
    let inject = on["inject"].as_str().unwrap();
    assert!(inject.contains("## Predicted"), "{inject}");
    assert!(
        inject.contains("skill parse-errors (confidence 1.00, predicted)"),
        "{inject}"
    );
    assert!(
        inject.contains("memory parser-lines (confidence 1.00, predicted)"),
        "{inject}"
    );
    assert!(on["contextBudget"]["included"]
        .as_array()
        .unwrap()
        .iter()
        .any(|id| id == "neural:predicted"));

    std::fs::write(
        root.join("ax.json"),
        r#"{"context":{"budgetTokens":100000},"neural":{"learning":{"feedback_collection":true}}}"#,
    )
    .unwrap();
    let off = preflight(root, "fix the parser").await;
    assert!(!off["inject"].as_str().unwrap().contains("## Predicted"));
    assert!(!off["contextBudget"]
        .to_string()
        .contains("neural:predicted"));
}

#[tokio::test]
async fn no_predicted_block_when_feedback_predicts_nothing_for_the_prompt() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("ax.json"),
        r#"{"context":{"budgetTokens":100000},"neural":{"prediction":{"enabled":true},"learning":{"feedback_collection":true}}}"#,
    )
    .unwrap();
    record_parser_feedback(root).await;
    let reply = preflight(root, "update the release notes").await;
    assert!(!reply["inject"].as_str().unwrap().contains("## Predicted"));
    assert!(!reply["contextBudget"]
        .to_string()
        .contains("neural:predicted"));
}
