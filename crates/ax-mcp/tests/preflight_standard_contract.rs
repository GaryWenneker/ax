//! The withdrawn writing-style experiment cannot change the standard MCP contract.
use ax_core::Ax;
use ax_mcp::{server::handle_request, McpEngine, ToolHandler};
use serde_json::{json, Value};

const POLICY: &str = "If role is Reader, deny writes unless an explicit grant exists.\nThe finding is unverified. Preserve `reader_id`. Unicode: café, Δ, 🪓.";
const EXPERIMENT_GUIDANCE: &str = "Write explanations in clear technical English.";

/// The tests in this binary run in parallel threads of one process. One of
/// them changes `AX_HOME_DIR` and `AX_CONTEXT_CACHE`, so it holds this lock
/// alone; the others read the environment and share it.
static ENV: tokio::sync::RwLock<()> = tokio::sync::RwLock::const_new(());
static BASE: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Point the home and usage database at a temporary directory once, so no
/// test touches the developer's real data. Called with the write lock held.
fn isolate_process() {
    BASE.get_or_init(|| {
        let base =
            std::env::temp_dir().join(format!("ax-preflight-contract-{}", std::process::id()));
        std::fs::create_dir_all(base.join("home")).unwrap();
        std::env::set_var("AX_HOME_DIR", base.join("home"));
        std::env::set_var("AX_USAGE_DB", base.join("usage.db"));
        base
    });
}

async fn env_shared() -> tokio::sync::RwLockReadGuard<'static, ()> {
    if BASE.get().is_none() {
        let _write = ENV.write().await;
        isolate_process();
    }
    ENV.read().await
}

async fn env_exclusive() -> tokio::sync::RwLockWriteGuard<'static, ()> {
    let guard = ENV.write().await;
    isolate_process();
    guard
}

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
    let _env = env_shared().await;
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
    let _env = env_shared().await;
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
    let _env = env_shared().await;
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
    let _env = env_shared().await;
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

struct RestoreEnv(Vec<(&'static str, Option<std::ffi::OsString>)>);

impl Drop for RestoreEnv {
    fn drop(&mut self) {
        for (key, previous) in self.0.drain(..) {
            match previous {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

async fn preflight_inject(ax: &mut Ax) -> String {
    let out = ToolHandler::call_tool(
        ax,
        "ax_preflight",
        json!({"session":"axs-other", "prompt":"show the plan", "context_reset":true}),
    )
    .await
    .unwrap();
    out["inject"].as_str().unwrap().to_string()
}

fn assert_live_plan(text: &str) {
    assert!(text.contains("<ax_section name=\"spec\">"), "{text}");
    assert!(text.contains("spec: queue-plan"), "{text}");
    assert!(text.contains("approved: none"), "{text}");
    assert!(text.contains("pointer: ax spec show queue-plan"), "{text}");
    assert!(!text.contains("Keep the first job"), "{text}");
}

#[tokio::test]
async fn a_live_plan_is_in_preflight_without_cache_or_index() {
    let _env = env_exclusive().await;
    let home = tempfile::tempdir().unwrap();
    let _restore = RestoreEnv(vec![
        ("AX_HOME_DIR", std::env::var_os("AX_HOME_DIR")),
        ("AX_CONTEXT_CACHE", std::env::var_os("AX_CONTEXT_CACHE")),
    ]);
    std::env::set_var("AX_HOME_DIR", home.path());
    let plans = home.path().join(".cursor").join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    std::fs::write(
        plans.join("queue.plan.md"),
        "# Queue\n\nKeep the first job.\n",
    )
    .unwrap();
    let (dir, mut ax) = fixture().await;
    let turns = dir.path().join(".ax").join("turns");
    std::fs::create_dir_all(&turns).unwrap();
    std::fs::write(
        turns.join("current.json"),
        "{\"session_id\":\"cursor-live\"}\n",
    )
    .unwrap();

    assert_live_plan(&preflight_inject(&mut ax).await);

    sqlx::query("DROP TABLE files")
        .execute(ax.db_pool())
        .await
        .unwrap();
    assert_live_plan(&preflight_inject(&mut ax).await);

    std::env::set_var("AX_CONTEXT_CACHE", "off");
    assert_live_plan(&preflight_inject(&mut ax).await);
}
