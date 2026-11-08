//! CLI-only turn lifecycle integration (no MCP transport).

use std::fs;
use std::process::Command;

use tempfile::tempdir;

fn ax_bin() -> std::path::PathBuf {
    std::env::var_os("CARGO_BIN_EXE_ax")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let mut p = std::env::current_exe().unwrap();
            p.pop();
            if p.ends_with("deps") {
                p.pop();
            }
            p.join("ax")
        })
}

fn run_ax(root: &std::path::Path, args: &[&str], stdin: Option<&str>) -> (i32, String, String) {
    let mut cmd = Command::new(ax_bin());
    cmd.current_dir(root).args(args);
    if stdin.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }
    let mut child = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn ax");
    if let Some(input) = stdin {
        use std::io::Write;
        let mut sin = child.stdin.take().unwrap();
        sin.write_all(input.as_bytes()).unwrap();
    }
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn research_turn_without_file_changes_is_persisted() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("main.rs"), "fn main() {}\n").unwrap();
    // Minimal git repo so snapshots work.
    let _ = Command::new("git").args(["init"]).current_dir(root).status();
    let _ = Command::new("git")
        .args(["config", "user.email", "ax@test"])
        .current_dir(root)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.name", "ax"])
        .current_dir(root)
        .status();
    let _ = Command::new("git")
        .args(["add", "."])
        .current_dir(root)
        .status();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .status();

    // Isolate usage.db for this test.
    std::env::set_var("AX_USAGE_DB", root.join("usage.db"));

    let (code, out, err) = run_ax(root, &["init", &root.display().to_string()], None);
    assert!(
        code == 0 || out.contains("Initialized") || err.contains("already"),
        "init failed code={code} out={out} err={err}"
    );

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
        Some(r#"{"kind":"finding","body":"Cookie SameSite mismatch on soft reload","decisions":["Use Lax"]}"#),
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
        Some(r#"{"outcome":"Root cause confirmed without code changes.","decisions":["Use Lax SameSite"]}"#),
    );
    assert_eq!(code, 0, "turn finish: {err}\n{out}");
    let receipt: serde_json::Value = serde_json::from_str(&out).expect(&out);
    assert!(receipt["memory_id"].as_str().is_some() || receipt["memoryId"].as_str().is_some());
    let files = receipt
        .get("files")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    // Research turn: no requirement for file changes.
    assert!(files.is_empty() || files.iter().all(|f| f.as_str().is_some()));

    let (code, out, err) = run_ax(
        root,
        &["turn", "status", "--session", &session, "--json"],
        None,
    );
    assert_eq!(code, 0, "turn status: {err}");
    assert!(out.contains(&turn) || out.contains("finished"));
}
