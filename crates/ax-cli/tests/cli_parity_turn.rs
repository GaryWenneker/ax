//! CLI-only turn lifecycle integration (no MCP transport).

mod common;

use std::fs;

use common::{git, git_init_commit, run_ax_env};
use tempfile::tempdir;

#[test]
fn research_turn_without_file_changes_is_persisted() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    // The usage database lives outside the repository so it is not a changed file.
    let usage = tempdir().unwrap();
    let usage_db = usage.path().join("usage.db");
    let envs = [("AX_USAGE_DB", usage_db.as_os_str())];
    let run_ax = |root: &std::path::Path, args: &[&str], stdin: Option<&str>| {
        run_ax_env(root, args, stdin, &envs)
    };
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    // Minimal git repo so snapshots work.
    git_init_commit(root);

    let (code, out, err) = run_ax(root, &["init", &root.display().to_string()], None);
    assert_eq!(code, 0, "init failed: out={out} err={err}");
    // Commit what init wrote, so the turn starts from a clean tree.
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "ax init"]);

    let (code, out, err) = run_ax(root, &["session", "start", "--json"], None);
    assert_eq!(code, 0, "session start: {err}");
    let session_json: serde_json::Value = serde_json::from_str(&out).expect(&out);
    let session = session_json["session"]
        .as_str()
        .expect("session id")
        .to_string();

    let (code, out, err) = run_ax(
        root,
        &[
            "turn",
            "begin",
            "--session",
            &session,
            "--prompt-stdin",
            "--json",
        ],
        Some("Why does authentication fail after refresh?"),
    );
    assert_eq!(code, 0, "turn begin: {err}\n{out}");
    let begin: serde_json::Value = serde_json::from_str(&out).expect(&out);
    let turn = begin["turnId"].as_str().expect("turnId").to_string();

    let (code, _out, err) = run_ax(
        root,
        &[
            "turn",
            "event",
            "--session",
            &session,
            "--turn",
            &turn,
            "--input-stdin",
        ],
        Some(
            r#"{"kind":"finding","body":"Cookie SameSite mismatch on soft reload","decisions":["Use Lax"]}"#,
        ),
    );
    assert_eq!(code, 0, "turn event: {err}");

    let (code, out, err) = run_ax(
        root,
        &[
            "turn",
            "finish",
            "--session",
            &session,
            "--turn",
            &turn,
            "--outcome-stdin",
            "--json",
        ],
        Some(
            r#"{"outcome":"Root cause confirmed without code changes.","decisions":["Use Lax SameSite"]}"#,
        ),
    );
    assert_eq!(code, 0, "turn finish: {err}\n{out}");
    let receipt: serde_json::Value = serde_json::from_str(&out).expect(&out);
    assert_eq!(receipt["state"], "finished", "{receipt}");
    assert_eq!(
        receipt["turn_id"].as_str(),
        Some(turn.as_str()),
        "{receipt}"
    );
    // Research turn: nothing in the tree changed.
    assert_eq!(receipt["files"], serde_json::json!([]), "{receipt}");
    let memory_id = receipt["memory_id"]
        .as_str()
        .expect("memory_id in the receipt")
        .to_string();

    let (code, out, err) = run_ax(root, &["recall", "--id", &memory_id, "--json"], None);
    assert_eq!(code, 0, "recall: {err}\n{out}");
    let recalled: serde_json::Value = serde_json::from_str(&out).expect(&out);
    let matches = recalled["matches"].as_array().expect("matches");
    assert_eq!(matches.len(), 1, "{recalled}");
    let body = matches[0]["body"].as_str().expect("memory body");
    assert!(
        body.contains("Root cause confirmed without code changes."),
        "{body}"
    );
    assert!(body.contains("Use Lax SameSite"), "{body}");

    let (code, out, err) = run_ax(
        root,
        &["turn", "status", "--session", &session, "--json"],
        None,
    );
    assert_eq!(code, 0, "turn status: {err}");
    let status: serde_json::Value = serde_json::from_str(&out).expect(&out);
    assert_eq!(status["finished"], serde_json::json!([turn]), "{status}");
    assert_eq!(status["open"], serde_json::json!([]), "{status}");
}
