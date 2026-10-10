//! Hook attribution must be explicit and project-bound.
use serde_json::json;
use std::io::Write;
use std::process::{Command, Stdio};

#[tokio::test]
async fn nested_hook_cwd_is_bound_to_the_initialized_project_and_window() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let ax = ax_core::Ax::init(root.path()).await.unwrap();
    drop(ax);
    let nested = root.path().join("nested");
    std::fs::create_dir_all(&nested).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ax"))
        .arg("session-hook")
        .current_dir(&nested)
        .env("AX_HOME_DIR", home.path())
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("AX_USAGE_DB", home.path().join("usage.db"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let input = json!({"session_id":"hook-chat","cwd":nested,"window_id":"window-a","hook_event_name":"sessionStart"});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = std::fs::read_to_string(home.path().join(".ax/active-cursor-session")).unwrap();
    let record: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(record["session_id"], "hook-chat");
    assert_eq!(record["window_id"], "window-a");
    assert_eq!(record["project"], ax_usage::project_scope(root.path()));
}

#[tokio::test]
async fn prompt_and_stop_hooks_never_store_unscoped_transcript_bodies() {
    let root = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let ax = ax_core::Ax::init(root.path()).await.unwrap();
    drop(ax);
    let slug = root.path().to_string_lossy().replace(['/', '\\', ':'], "-");
    let transcript = home
        .path()
        .join(".claude/projects")
        .join(slug)
        .join("chat.jsonl");
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    let body = "Project-owned transcript content. ".repeat(100);
    std::fs::write(
        transcript,
        json!({"type":"tool_result","name":"Read","content":body}).to_string(),
    )
    .unwrap();
    let usage = home.path().join("usage.db");
    for hook in ["prompt-hook", "stop-hook", "session-hook"] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ax"))
            .arg(hook)
            .current_dir(root.path())
            .env("AX_HOME_DIR", home.path())
            .env("AX_NO_UPDATE_CHECK", "1")
            .env("AX_OFFLOAD_DISABLE", "1")
            .env("AX_TELEMETRY", "0")
            .env("AX_USAGE_DB", &usage)
            .env("AX_CONTEXT_CACHE_TOKENS", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = json!({"session_id":"hook-chat","cwd":root.path(),"window_id":"window-a","prompt":"Review this project", "hook_event_name":"beforeSubmitPrompt"});
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{hook}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&usage)
        .read_only(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .unwrap();
    let unscoped: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM mcp_context_cache WHERE project IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unscoped, 0);
    let scoped: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM mcp_context_cache WHERE project = ?")
            .bind(ax_usage::project_scope(root.path()))
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        scoped > 0,
        "stop hook must preserve project-owned tool results"
    );
}
