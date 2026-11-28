//! Remaining acceptance scenarios: parallel sessions, crash recovery, MCP/CLI preflight parity.

mod common;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use common::{git_init_commit, run_ax_env};
use serde_json::Value;
use tempfile::tempdir;

/// Serializes the one test that sets `AX_USAGE_DB` in this process (for the
/// in-process MCP engine, which reads it from the environment).
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// A temp project with its own usage database outside the repository.
struct Project {
    dir: tempfile::TempDir,
    _usage: tempfile::TempDir,
    usage_db: PathBuf,
}

impl Project {
    fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Run `ax` in the project with `AX_USAGE_DB` set on the child only.
    fn ax(&self, args: &[&str], stdin: Option<&str>) -> (i32, String, String) {
        run_ax_env(
            self.root(),
            args,
            stdin,
            &[("AX_USAGE_DB", self.usage_db.as_os_str())],
        )
    }
}

/// Sets one variable for the calling test and restores it on drop.
struct ScopedEnv {
    name: &'static str,
    previous: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl ScopedEnv {
    fn set(name: &'static str, value: &Path) -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os(name);
        std::env::set_var(name, value);
        Self {
            name,
            previous,
            _lock: lock,
        }
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(v) => std::env::set_var(self.name, v),
            None => std::env::remove_var(self.name),
        }
    }
}

fn prepare_project() -> Project {
    let dir = tempdir().unwrap();
    let usage = tempdir().unwrap();
    let project = Project {
        usage_db: usage.path().join("usage.db"),
        dir,
        _usage: usage,
    };
    let root = project.root();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src").join("lib.rs"),
        "pub fn ping() -> i32 { 1 }\n",
    )
    .unwrap();
    git_init_commit(root);
    let (code, out, err) = project.ax(&["init", &root.display().to_string()], None);
    assert_eq!(code, 0, "init failed: out={out} err={err}");
    // Ensure graph fingerprint exists for ax_session add.
    let (code, out, err) = project.ax(&["sync", "--quiet"], None);
    assert_eq!(code, 0, "sync failed: out={out} err={err}");
    project
}

#[test]
fn parallel_sessions_do_not_mix_working_context() {
    let project = prepare_project();
    let run_ax = |_root: &Path, args: &[&str], stdin: Option<&str>| project.ax(args, stdin);
    let root = project.root();

    let (code, out_a, err) = run_ax(root, &["session", "start", "--json"], None);
    assert_eq!(code, 0, "session start: {err}\n{out_a}");
    let a: Value = serde_json::from_str(&out_a).unwrap();
    let sess_a = a["session"].as_str().unwrap().to_string();

    let (code, out_b, err) = run_ax(root, &["session", "start", "--json"], None);
    assert_eq!(code, 0, "session start: {err}\n{out_b}");
    let b: Value = serde_json::from_str(&out_b).unwrap();
    let sess_b = b["session"].as_str().unwrap().to_string();
    assert_ne!(sess_a, sess_b);

    let (code, _, err) = run_ax(
        root,
        &[
            "session",
            "add",
            "--session",
            &sess_a,
            "--input-stdin",
            "--json",
        ],
        Some(r#"{"facts":["only-session-a"]}"#),
    );
    assert_eq!(code, 0, "{err}");
    let (code, _, err) = run_ax(
        root,
        &[
            "session",
            "add",
            "--session",
            &sess_b,
            "--input-stdin",
            "--json",
        ],
        Some(r#"{"facts":["only-session-b"]}"#),
    );
    assert_eq!(code, 0, "{err}");

    let (code_a, out_a, err_a) = run_ax(
        root,
        &["session", "get", "--session", &sess_a, "--json"],
        None,
    );
    let (code_b, out_b, err_b) = run_ax(
        root,
        &["session", "get", "--session", &sess_b, "--json"],
        None,
    );
    assert_eq!(code_a, 0, "{err_a}");
    assert_eq!(code_b, 0, "{err_b}");
    assert!(out_a.contains("only-session-a"), "{out_a}");
    assert!(!out_a.contains("only-session-b"), "{out_a}");
    assert!(out_b.contains("only-session-b"), "{out_b}");
    assert!(!out_b.contains("only-session-a"), "{out_b}");
}

#[test]
fn unfinished_turn_stays_incomplete_after_restart() {
    let project = prepare_project();
    let run_ax = |_root: &Path, args: &[&str], stdin: Option<&str>| project.ax(args, stdin);
    let root = project.root();
    let (code, out, err) = run_ax(root, &["session", "start", "--json"], None);
    assert_eq!(code, 0, "session start: {err}\n{out}");
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
    let status: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(status["open"], serde_json::json!([turn]), "{status}");
    assert_eq!(status["finished"], serde_json::json!([]), "{status}");
}

#[test]
#[allow(clippy::await_holding_lock)]
fn cli_and_mcp_preflight_share_matched_policy_ids() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async {
        let project = prepare_project();
        let root = project.root().to_path_buf();
        // Seed a CRITICAL always-apply rule so both paths must surface it.
        let rules = root.join(".agents").join("rules");
        fs::create_dir_all(&rules).unwrap();
        fs::write(
            rules.join("parity-probe.mdc"),
            "---\nid: parity-probe\nlevel: CRITICAL\nalwaysApply: true\n---\nParity probe rule body.\n",
        )
        .unwrap();
        let (code, _, err) = project.ax(&["policy", "index"], None);
        assert_eq!(code, 0, "policy index: {err}");

        let prompt = "parity probe please";
        let (code, cli_out, err) = project.ax(
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
        let cli: Value = serde_json::from_str(&cli_out)
            .unwrap_or_else(|e| panic!("cli preflight is not JSON ({e}): {cli_out}"));

        // The in-process engine reads AX_USAGE_DB from this process.
        let _env = ScopedEnv::set("AX_USAGE_DB", &project.usage_db);
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

        let cli_ids = policy_ids(&cli);
        let mcp_ids = policy_ids(&mcp);
        assert!(cli_ids.contains("rule:parity-probe"), "CLI ids: {cli_ids:?}");
        assert_eq!(cli_ids, mcp_ids, "CLI and MCP matched different policy");
    });
}

/// `kind:id` of every policy source a preflight matched, from the CLI JSON or
/// an MCP tool result (structured content or a JSON text block).
fn policy_ids(v: &Value) -> std::collections::BTreeSet<String> {
    let body = if v.get("policySources").is_some() {
        v.clone()
    } else if let Some(sc) = v
        .get("structuredContent")
        .filter(|s| s.get("policySources").is_some())
    {
        sc.clone()
    } else {
        let text = v
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("no preflight body in {v}"));
        serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("preflight text is not JSON ({e}): {text}"))
    };
    body["policySources"]
        .as_array()
        .unwrap_or_else(|| panic!("no policySources in {body}"))
        .iter()
        .map(|s| {
            format!(
                "{}:{}",
                s["kind"].as_str().unwrap_or_default(),
                s["id"].as_str().unwrap_or_default()
            )
        })
        .collect()
}
