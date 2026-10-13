//! Opt-in writing style: public contract, source preservation and delivery state.
use ax_core::Ax;
use ax_mcp::{server::handle_request, McpEngine, ToolHandler};
use serde_json::{json, Value};

const POLICY: &str = "If role is Reader, deny writes unless an explicit grant exists.\nThe finding is unverified. Do not change `reader_id` or `/src/auth.rs`.\n```rust\nlet example = \"<ax_policy>quoted markup</ax_policy>\";\n```\nUnicode: café, Δ, 🪓.";
const GUIDANCE: &str = "Write explanations in clear technical English.";

async fn fixture() -> (tempfile::TempDir, Ax) {
    let dir = tempfile::tempdir().unwrap();
    let ax = Ax::init(dir.path()).await.unwrap();
    let rules = dir.path().join(".agents/rules");
    std::fs::create_dir_all(&rules).unwrap();
    std::fs::write(
        rules.join("protected.mdc"),
        format!("---\nid: protected\nlevel: CRITICAL\nalwaysApply: true\n---\n{POLICY}\n"),
    )
    .unwrap();
    ax_policy::index_policy(ax.db_pool(), dir.path(), true)
        .await
        .unwrap();
    (dir, ax)
}

async fn raw(ax: &mut Ax, mut args: Value) -> Value {
    args["session"] = json!("style-contract");
    ToolHandler::call_tool(ax, "ax_preflight", args)
        .await
        .unwrap()
}

async fn wire(engine: &mut McpEngine, args: Value) -> Value {
    handle_request(
        engine,
        "tools/call",
        json!({"name":"ax_preflight", "arguments":args}),
    )
    .await
    .result
    .unwrap()
}

fn text(reply: &Value) -> &str {
    reply["content"][0]["text"].as_str().unwrap()
}

fn stable(mut value: Value) -> Value {
    // The index's wall-clock timestamp is unrelated to presentation.
    if let Some(stats) = value["indexStats"].as_object_mut() {
        stats.remove("lastUpdated");
    }
    value
}

#[tokio::test]
async fn s01_legacy_default_is_unchanged() {
    let (_dir, mut ax) = fixture().await;
    let out = raw(&mut ax, json!({})).await;
    assert!(out.get("presentation").is_none());
    assert_eq!(
        out["instruction"],
        "Apply CRITICAL rules before editing. If a skill matched, follow its workflow."
    );
    assert!(out["inject"]
        .as_str()
        .unwrap()
        .contains("Apply matched rules and skills before editing.\n\n"));
    assert!(!out["inject"].as_str().unwrap().contains(GUIDANCE));
    assert!(out["inject"].as_str().unwrap().contains(POLICY));
}

#[tokio::test]
async fn s02_explicit_standard_matches_legacy() {
    let (_dir, mut ax) = fixture().await;
    let omitted = raw(&mut ax, json!({})).await;
    let explicit = raw(&mut ax, json!({"textStyle":"standard"})).await;
    assert_eq!(stable(omitted), stable(explicit));
}

#[tokio::test]
async fn s03_style_is_independent_of_transport() {
    let (dir, mut ax) = fixture().await;
    let out = raw(&mut ax, json!({"textStyle":"ste-inspired"})).await;
    assert_eq!(out["presentation"]["textStyle"], "ste-inspired");
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    let envelope = wire(&mut engine, json!({"textStyle":"ste-inspired"})).await;
    for rendered in [
        ax_mcp::format_tool_result(&out),
        text(&envelope).to_string(),
    ] {
        assert!(rendered.contains(GUIDANCE));
        assert!(rendered.contains("deny writes unless an explicit grant exists"));
        assert!(rendered.contains("The finding is unverified."));
    }
}

#[tokio::test]
async fn s04_invalid_style_has_no_delivery_effect() {
    let (dir, _ax) = fixture().await;
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    let args = json!({"session":"invalid-style", "textStyle":"ste-inspired"});
    let first = wire(&mut engine, args.clone()).await;
    assert!(text(&first).contains(POLICY));
    for invalid in [json!("unknown"), json!(null), json!(2), json!([])] {
        let outcome = handle_request(
            &mut engine,
            "tools/call",
            json!({
                "name":"ax_preflight", "arguments": {
                    "session":"invalid-style", "textStyle":invalid, "context_reset":true
                }
            }),
        )
        .await;
        assert!(outcome.result.is_err(), "accepted {invalid}");
        assert!(outcome.result.unwrap_err().contains("textStyle"));
    }
    let next = wire(&mut engine, args).await;
    assert!(
        !text(&next).contains(POLICY),
        "invalid request changed delivery state"
    );
}

#[tokio::test]
async fn s05_protected_source_is_exact() {
    let (_dir, mut ax) = fixture().await;
    for style in ["standard", "ste-inspired"] {
        let out = raw(&mut ax, json!({"textStyle":style})).await;
        assert!(out["inject"].as_str().unwrap().contains(POLICY));
        let rule = out["rules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "protected")
            .unwrap();
        assert_eq!(rule["body"], POLICY);
        assert_eq!(
            ax_policy::get_rule(ax.db_pool(), "protected")
                .await
                .unwrap()
                .unwrap()
                .body,
            POLICY
        );
    }
}

#[tokio::test]
async fn s06_conditions_and_uncertainty_survive() {
    let (_dir, mut ax) = fixture().await;
    let legacy = raw(
        &mut ax,
        json!({"prompt":"You must preserve project scope."}),
    )
    .await;
    let styled = raw(
        &mut ax,
        json!({"prompt":"You must preserve project scope.", "textStyle":"ste-inspired"}),
    )
    .await;
    assert_eq!(legacy["captureProposal"], styled["captureProposal"]);
    let inject = styled["inject"].as_str().unwrap();
    assert!(inject.contains("unless an explicit grant exists"));
    assert!(inject.contains("The finding is unverified."));
    assert!(styled["instruction"]
        .as_str()
        .unwrap()
        .contains("After the user confirms"));
    assert!(inject.contains(GUIDANCE));
}

#[tokio::test]
async fn s07_required_overflow_is_honest() {
    let (dir, mut ax) = fixture().await;
    std::fs::write(
        dir.path().join("ax.json"),
        r#"{"context":{"budgetTokens":1}}"#,
    )
    .unwrap();
    let out = raw(&mut ax, json!({"textStyle":"ste-inspired"})).await;
    assert!(out["inject"].as_str().unwrap().contains(POLICY));
    assert!(out["inject"]
        .as_str()
        .unwrap()
        .contains("The context exceeds the token budget."));
    assert_eq!(out["contextBudget"]["overBudget"], true);
    assert!(out["contextBudget"]["requiredMinimum"].as_u64().unwrap() > 1);
}

#[tokio::test]
async fn s08_expansion_returns_full_source() {
    let (dir, ax) = fixture().await;
    let body = format!(
        "First step.\n{}\nLast step, unless the result is unverified.",
        "Exact skill source. ".repeat(1000)
    );
    let path = dir.path().join(".agents/skills/large-workflow/SKILL.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("---\nname: large-workflow\ndescription: Architecture workflow\ntriggers: [architecture]\n---\n{body}\n")).unwrap();
    ax_policy::index_policy(ax.db_pool(), dir.path(), true)
        .await
        .unwrap();
    std::fs::write(
        dir.path().join("ax.json"),
        r#"{"context":{"budgetTokens":1}}"#,
    )
    .unwrap();
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    let args =
        json!({"session":"skill-source", "prompt":"architecture", "textStyle":"ste-inspired"});
    let first = wire(&mut engine, args).await;
    assert!(!text(&first).contains(&body));
    let full = ax_mcp::call_tool(
        &mut engine,
        "ax_skill",
        json!({"session":"skill-source", "name":"large-workflow"}),
    )
    .await
    .unwrap();
    assert_eq!(full["body"], body);
    assert_eq!(
        ax_policy::get_skill(ax.db_pool(), "large-workflow")
            .await
            .unwrap()
            .unwrap()
            .body,
        body
    );
}

#[tokio::test]
async fn s09_profile_change_invalidates_presentation() {
    let (dir, _ax) = fixture().await;
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    for (style, expected) in [
        ("standard", false),
        ("ste-inspired", true),
        ("standard", false),
    ] {
        let out = wire(
            &mut engine,
            json!({"session":"style-switch", "textStyle":style}),
        )
        .await;
        assert_eq!(text(&out).contains(GUIDANCE), expected);
        assert_eq!(
            out["structuredContent"].get("presentation").is_some(),
            expected
        );
    }
}

#[tokio::test]
async fn s10_reset_restores_required_context() {
    let (dir, _ax) = fixture().await;
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    let args = json!({"session":"style-reset", "textStyle":"ste-inspired"});
    assert!(text(&wire(&mut engine, args.clone()).await).contains(POLICY));
    assert!(!text(&wire(&mut engine, args.clone()).await).contains(POLICY));
    let reset = wire(
        &mut engine,
        json!({"session":"style-reset", "textStyle":"ste-inspired", "context_reset":true}),
    )
    .await;
    assert!(text(&reset).contains(POLICY));
    assert_eq!(
        reset["structuredContent"]["presentation"]["profile"],
        "ax-ste-inspired-1"
    );
}

#[tokio::test]
async fn s11_project_boundary_is_preserved() {
    let (_a, mut ax) = fixture().await;
    let (b, mut other) = fixture().await;
    let foreign =
        ToolHandler::call_tool(&mut other, "ax_stash", json!({"text":"PROJECT_B_SECRET"}))
            .await
            .unwrap();
    assert!(
        foreign["id"].as_str().is_some(),
        "stash did not return a usable ID"
    );
    let owned = ToolHandler::call_tool(&mut other, "ax_expand", json!({"id":foreign["id"]}))
        .await
        .unwrap();
    assert!(owned.to_string().contains("PROJECT_B_SECRET"));
    let expansion = ToolHandler::call_tool(&mut ax, "ax_expand", json!({"id":foreign["id"]})).await;
    if let Ok(reply) = expansion {
        assert!(!reply.to_string().contains("PROJECT_B_SECRET"));
    }
    let denied = ToolHandler::call_tool(
        &mut ax,
        "ax_preflight",
        json!({"projectPath":b.path(), "textStyle":"ste-inspired"}),
    )
    .await
    .unwrap_err();
    assert!(denied.contains("projectPath"));
}

#[tokio::test]
async fn s12_final_projection_is_measured() {
    let (_dir, mut ax) = fixture().await;
    let out = raw(&mut ax, json!({"textStyle":"ste-inspired"})).await;
    assert_eq!(
        out["contextBudget"]["finalTokens"],
        ax_mcp::context_budget::projection_tokens(&out)
    );
    let mut without_metadata = out.clone();
    without_metadata
        .as_object_mut()
        .unwrap()
        .remove("presentation");
    assert!(
        ax_mcp::context_budget::projection_tokens(&out)
            > ax_mcp::context_budget::projection_tokens(&without_metadata)
    );
}

#[tokio::test]
async fn s13_metadata_does_not_duplicate_bodies() {
    let (dir, _ax) = fixture().await;
    let mut engine = McpEngine::with_project_root(dir.path().to_path_buf());
    let out = wire(&mut engine, json!({"textStyle":"ste-inspired"})).await;
    assert_eq!(
        out["structuredContent"]["presentation"],
        json!({"textStyle":"ste-inspired", "profile":"ax-ste-inspired-1"})
    );
    assert!(out["structuredContent"].get("rules").is_none());
    assert!(!out["structuredContent"]
        .to_string()
        .contains("deny writes unless an explicit grant exists"));
    assert!(text(&out).contains(POLICY));
}

#[tokio::test]
async fn s14_rendering_is_deterministic() {
    let (_dir, mut ax) = fixture().await;
    let args = json!({"textStyle":"ste-inspired"});
    let first = raw(&mut ax, args.clone()).await;
    assert!(first["inject"].as_str().unwrap().contains(GUIDANCE));
    for _ in 0..3 {
        let repeated = raw(&mut ax, args.clone()).await;
        assert_eq!(stable(first.clone()), stable(repeated));
    }
}

#[tokio::test]
async fn s15_schema_exposes_only_optional_supported_styles() {
    let tools = ToolHandler::list_tools(false).await;
    let tool = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "ax_preflight")
        .unwrap();
    let schema = &tool["inputSchema"];
    assert_eq!(schema["properties"]["textStyle"]["type"], "string");
    assert_eq!(
        schema["properties"]["textStyle"]["enum"],
        json!(["standard", "ste-inspired"])
    );
    assert!(schema.get("required").is_none());
}

#[tokio::test]
async fn history_hint_keeps_the_available_fields() {
    let (_dir, mut ax) = fixture().await;
    for (style, detail) in [
        ("standard", "prompt, outcome, files, commits"),
        (
            "ste-inspired",
            "Each turn includes its prompt, outcome, files and commits.",
        ),
    ] {
        let out = raw(
            &mut ax,
            json!({"prompt":"wanneer heb ik de review-taal aangepast?", "textStyle":style}),
        )
        .await;
        let inject = out["inject"].as_str().unwrap();
        assert!(inject.contains("<ax_history_hint>"));
        assert!(inject.contains(detail));
    }
}

#[tokio::test]
async fn matched_skill_workflow_is_required_before_editing() {
    let (_dir, mut ax) = fixture().await;
    let out = raw(&mut ax, json!({"textStyle":"ste-inspired"})).await;
    assert!(out["inject"]
        .as_str()
        .unwrap()
        .contains("Follow each matched skill's workflow before you edit files."));
}
