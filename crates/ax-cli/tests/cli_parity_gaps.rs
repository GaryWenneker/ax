//! Remaining acceptance scenarios: parallel sessions, crash recovery, MCP/CLI preflight parity.

use std::fs;
use std::process::Command;
use std::sync::{Mutex, MutexGuard};

use serde_json::Value;
use tempfile::tempdir;

static ENV_LOCK: Mutex<()> = Mutex::new(());

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

fn prepare_project() -> (tempfile::TempDir, MutexGuard<'static, ()>) {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("lib.rs"), "pub fn ping() -> i32 { 1 }\n").unwrap();
    let _ = Command::new("git").args(["init"]).current_dir(root).status();
    let _ = Command::new("git")
        .args(["config", "user.email", "ax@test"])
        .current_dir(root)
        .status();
    let _ = Command::new("git")
        .args(["config", "user.name", "ax"])
        .current_dir(root)
        .status();
    let _ = Command::new("git").args(["add", "."]).current_dir(root).status();
    let _ = Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(root)
        .status();
    std::env::set_var("AX_USAGE_DB", root.join("usage.db"));
    let (code, out, err) = run_ax(root, &["init", &root.display().to_string()], None);
    assert!(
        code == 0 || err.contains("already") || out.contains("Initialized"),
        "init failed: {code} {out} {err}"
    );
    // Ensure graph fingerprint exists for ax_session add.
    let (code, out, err) = run_ax(root, &["sync", "--quiet"], None);
    assert!(
        code == 0,
        "sync failed: code={code} out={out} err={err}"
    );
    (dir, guard)
}

#[test]
fn parallel_sessions_do_not_mix_working_context() {
    let (dir, _guard) = prepare_project();
    let root = dir.path();

    let (_, out_a, err) = run_ax(root, &["session", "start", "--json"], None);
    assert!(err.is_empty() || out_a.contains("session"), "{err}");
    let a: Value = serde_json::from_str(&out_a).unwrap();
    let sess_a = a["session"].as_str().unwrap().to_string();

    let (_, out_b, _) = run_ax(root, &["session", "start", "--json"], None);
    let b: Value = serde_json::from_str(&out_b).unwrap();
    let sess_b = b["session"].as_str().unwrap().to_string();
    assert_ne!(sess_a, sess_b);

    let (code, _, err) = run_ax(
        root,
        &["session", "add", "--session", &sess_a, "--input-stdin", "--json"],
        Some(r#"{"facts":["only-session-a"]}"#),
    );
    assert_eq!(code, 0, "{err}");
    let (code, _, err) = run_ax(
        root,
        &["session", "add", "--session", &sess_b, "--input-stdin", "--json"],
        Some(r#"{"facts":["only-session-b"]}"#),
    );
    assert_eq!(code, 0, "{err}");

    let (_, out_a, _) = run_ax(root, &["session", "get", "--session", &sess_a, "--json"], None);
    let (_, out_b, _) = run_ax(root, &["session", "get", "--session", &sess_b, "--json"], None);
    assert!(out_a.contains("only-session-a"), "{out_a}");
    assert!(!out_a.contains("only-session-b"), "{out_a}");
    assert!(out_b.contains("only-session-b"), "{out_b}");
    assert!(!out_b.contains("only-session-a"), "{out_b}");
}

#[test]
fn unfinished_turn_stays_incomplete_after_restart() {
    let (dir, _guard) = prepare_project();
    let root = dir.path();
    let (_, out, _) = run_ax(root, &["session", "start", "--json"], None);
    let sess = serde_json::from_str::<Value>(&out).unwrap()["session"]
        .as_str()
        .unwrap()
        .to_string();
    let (code, out, err) = run_ax(
        root,
        &[
            "turn",
            "begin",
            "--session",
            &sess,
            "--prompt-stdin",
            "--json",
        ],
        Some("interrupted research"),
    );
    assert_eq!(code, 0, "{err}\n{out}");
    let turn = serde_json::from_str::<Value>(&out).unwrap()["turnId"]
        .as_str()
        .unwrap()
        .to_string();

    // Simulate process end without finish: status must still show open.
    let (code, out, err) = run_ax(
        root,
        &["turn", "status", "--session", &sess, "--json"],
        None,
    );
    assert_eq!(code, 0, "{err}");
    assert!(out.contains(&turn), "{out}");
    assert!(out.contains("open") || out.contains(&turn), "{out}");
    let status: Value = serde_json::from_str(&out).unwrap();
    let open = status["open"].as_array().cloned().unwrap_or_default();
    assert!(
        open.iter().any(|v| v.as_str() == Some(turn.as_str())),
        "expected open turn {turn} in {status}"
    );
}

#[test]
fn cli_and_mcp_preflight_share_matched_policy_ids() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
    let (dir, _guard) = prepare_project();
    let root = dir.path().to_path_buf();
    // Seed a CRITICAL always-apply rule so both paths must surface it.
    let rules = root.join(".agents").join("rules");
    fs::create_dir_all(&rules).unwrap();
    fs::write(
        rules.join("parity-probe.mdc"),
        "---\nid: parity-probe\nlevel: CRITICAL\nalwaysApply: true\n---\nParity probe rule body.\n",
    )
    .unwrap();
    let (code, _, err) = run_ax(root.as_path(), &["policy", "index"], None);
    assert_eq!(code, 0, "policy index: {err}");

    let prompt = "parity probe please";
    let (code, cli_out, err) = run_ax(
        root.as_path(),
        &[
            "preflight",
            "--prompt-stdin",
            "--session",
            "parity-sess",
            "--json",
        ],
        Some(prompt),
    );
    assert_eq!(code, 0, "cli preflight: {err}\n{cli_out}");
    let cli: Value = serde_json::from_str(&cli_out).unwrap_or_else(|_| {
        // Some builds wrap text; fall back to structuredContent if present.
        json_wrap(&cli_out)
    });

    let mut engine = ax_mcp::McpEngine::with_project_root(root.clone());
    let mcp = ax_mcp::call_tool(
        &mut engine,
        "ax_preflight",
        serde_json::json!({
            "prompt": prompt,
            "session": "parity-sess-mcp",
        }),
    )
    .await
    .expect("mcp preflight");

    let cli_text = flatten_text(&cli);
    let mcp_text = flatten_text(&mcp);
    assert!(
        cli_text.contains("parity-probe") || cli_text.contains("Parity probe"),
        "CLI missing rule: {cli_text}"
    );
    assert!(
        mcp_text.contains("parity-probe") || mcp_text.contains("Parity probe"),
        "MCP missing rule: {mcp_text}"
    );
    });
}

fn flatten_text(v: &Value) -> String {
    if let Some(s) = v.as_str() {
        return s.to_string();
    }
    if let Some(s) = v.pointer("/content/0/text").and_then(|x| x.as_str()) {
        return s.to_string();
    }
    if let Some(s) = v.get("inject").and_then(|x| x.as_str()) {
        return s.to_string();
    }
    v.to_string()
}

fn json_wrap(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| serde_json::json!({ "raw": raw }))
}
