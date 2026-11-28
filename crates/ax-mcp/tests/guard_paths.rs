//! ax_guard and ax_diagnostics read file content only inside the project.

use ax_mcp::tools::ToolHandler;
use serde_json::{json, Value};

async fn project_with_marker_rule() -> (tempfile::TempDir, ax_core::Ax) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    std::fs::create_dir_all(root.join("src")).unwrap();
    let mut ax = ax_core::Ax::init(&root).await.expect("init ax project");
    let saved = ToolHandler::call_tool(
        &mut ax,
        "ax_policy_capture",
        json!({"action":"save","rule":{
            "frontmatter":{"id":"no-marker","level":"CRITICAL","alwaysApply":true},
            "body":"Never write the marker.\n\nguard: forbid-content: \"GUARD_MARKER_42\"\n"}}),
    )
    .await
    .expect("save rule");
    assert_eq!(saved["ok"], true, "{saved}");
    (dir, ax)
}

/// True when the marker rule fired, i.e. the guarded file's content was read.
/// A path outside the project is refused with `outside-project` (#329) before
/// any read, which is not a content violation.
fn violated(result: &Value) -> bool {
    rule_ids(result).iter().any(|id| id == "no-marker")
}

fn rule_ids(result: &Value) -> Vec<String> {
    result["violations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v["rule_id"].as_str().map(str::to_string))
        .collect()
}

#[tokio::test]
async fn guard_reads_project_files_but_not_files_outside_the_project() {
    let (dir, mut ax) = project_with_marker_rule().await;
    let root = ax.project_root().to_path_buf();
    std::fs::write(root.join("src/inside.rs"), "GUARD_MARKER_42\n").unwrap();
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, "GUARD_MARKER_42\n").unwrap();

    // Positive control: the rule sees content of a file inside the project,
    // by relative or absolute path.
    for path in [
        "src/inside.rs".to_string(),
        root.join("src/inside.rs").to_string_lossy().into_owned(),
    ] {
        let inside = ToolHandler::call_tool(&mut ax, "ax_guard", json!({ "path": path }))
            .await
            .expect("guard inside");
        assert!(violated(&inside), "{path}: {inside}");
    }

    // A path outside the project is never read, so its content cannot leak
    // through the violations.
    for path in [
        outside.to_string_lossy().into_owned(),
        "../outside.txt".to_string(),
    ] {
        let result = ToolHandler::call_tool(&mut ax, "ax_guard", json!({ "path": path }))
            .await
            .expect("guard outside");
        assert!(!violated(&result), "{path} was read: {result}");
        let multi = ToolHandler::call_tool(
            &mut ax,
            "ax_guard",
            json!({ "paths": [path.clone(), "src/none.rs"] }),
        )
        .await
        .expect("guard outside multi");
        assert!(!violated(&multi), "{path} was read: {multi}");
    }
}

#[cfg(unix)]
#[tokio::test]
async fn guard_does_not_follow_a_symlink_out_of_the_project() {
    let (dir, mut ax) = project_with_marker_rule().await;
    let root = ax.project_root().to_path_buf();
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, "GUARD_MARKER_42\n").unwrap();
    std::os::unix::fs::symlink(&outside, root.join("src/link.rs")).unwrap();
    let result = ToolHandler::call_tool(&mut ax, "ax_guard", json!({ "path": "src/link.rs" }))
        .await
        .expect("guard link");
    assert!(!violated(&result), "symlink target was read: {result}");
}

#[tokio::test]
async fn guard_does_not_read_oversized_files() {
    let (_dir, mut ax) = project_with_marker_rule().await;
    let root = ax.project_root().to_path_buf();
    let mut big = "GUARD_MARKER_42\n".to_string();
    big.push_str(&"x".repeat(3 * 1024 * 1024));
    std::fs::write(root.join("src/big.rs"), big).unwrap();
    let result = ToolHandler::call_tool(&mut ax, "ax_guard", json!({ "path": "src/big.rs" }))
        .await
        .expect("guard big");
    assert!(!violated(&result), "oversized file was read: {result}");
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn guard_does_not_block_on_a_fifo() {
    let (_dir, mut ax) = project_with_marker_rule().await;
    let root = ax.project_root().to_path_buf();
    let fifo = root.join("src/pipe.rs");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .unwrap();
    assert!(status.success());
    let task = tokio::spawn(async move {
        ToolHandler::call_tool(&mut ax, "ax_guard", json!({ "path": "src/pipe.rs" })).await
    });
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(10), task).await;
    if outcome.is_err() {
        // Release the blocked reader so the runtime can shut down.
        drop(std::fs::OpenOptions::new().write(true).open(&fifo));
        panic!("ax_guard blocked on a FIFO");
    }
    let result = outcome.unwrap().unwrap().expect("guard fifo");
    assert!(!violated(&result), "{result}");
}

#[tokio::test]
async fn diagnostics_does_not_read_files_outside_the_project() {
    let (dir, mut ax) = project_with_marker_rule().await;
    let root = ax.project_root().to_path_buf();
    std::fs::write(root.join("src/inside.rs"), "GUARD_MARKER_42\n").unwrap();
    let outside = dir.path().join("outside.txt");
    std::fs::write(&outside, "GUARD_MARKER_42\n").unwrap();
    let result = ToolHandler::call_tool(
        &mut ax,
        "ax_diagnostics",
        json!({ "diagnostics": [
            {"path": "src/inside.rs", "message": "m"},
            {"path": outside.to_string_lossy(), "message": "m"},
            {"path": "../outside.txt", "message": "m"},
        ]}),
    )
    .await
    .expect("diagnostics");
    let text = result.to_string();
    assert!(text.contains("src/inside.rs"), "{text}");
    // Only the file inside the project was read: it alone trips the content rule.
    // The two outside paths may be listed, refused as outside-project.
    let entries = result["guardedPaths"]
        .as_array()
        .or_else(|| result["guarded"].as_array())
        .cloned()
        .unwrap_or_default();
    let read: Vec<String> = entries
        .iter()
        .filter(|g| violated(g))
        .filter_map(|g| g["path"].as_str().map(str::to_string))
        .collect();
    assert_eq!(read, vec!["src/inside.rs".to_string()], "{result}");
    for entry in entries.iter().filter(|g| g["path"] != "src/inside.rs") {
        assert_eq!(
            rule_ids(entry),
            vec!["outside-project".to_string()],
            "{result}"
        );
    }
}
