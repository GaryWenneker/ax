//! The prompt hook explores only what the user wrote (spec group E).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn ax(dir: &Path, home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ax"));
    cmd.current_dir(dir)
        .env("HOME", home)
        .env("AX_HOME_DIR", home)
        .env("USERPROFILE", home)
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("NO_COLOR", "1");
    cmd
}

/// An initialized project with one Rust function, `capture_affected_paths`.
fn project() -> (tempfile::TempDir, tempfile::TempDir) {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(repo.path().join("src")).unwrap();
    std::fs::write(
        repo.path().join("src/capture.rs"),
        "pub fn capture_affected_paths() -> usize {\n    0\n}\n",
    )
    .unwrap();
    let out = ax(repo.path(), home.path())
        .args(["init", "."])
        .stdin(Stdio::null())
        .output()
        .expect("run ax init");
    assert!(
        repo.path().join(".ax/ax.db").is_file(),
        "ax init failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    (repo, home)
}

fn hook(repo: &Path, home: &Path, prompt: &str) -> String {
    let mut child = ax(repo, home)
        .arg("prompt-hook")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ax prompt-hook");
    let payload = serde_json::json!({ "prompt": prompt, "cwd": repo });
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "exit {:?}", out.status.code());
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn e_prompt_hook_explores_only_what_the_user_wrote() {
    let (repo, home) = project();
    let (repo, home) = (repo.path(), home.path());

    // E4 (and the control for E1/E2): a real structural question gets context.
    let real = hook(repo, home, "how does capture_affected_paths work");
    assert!(real.contains("<ax_context"), "{real}");
    assert!(real.contains("capture_affected_paths"), "{real}");

    // E1: the same question inside a harness block is not the user's prompt.
    let wrapped = hook(
        repo,
        home,
        "<system-reminder>how does capture_affected_paths work</system-reminder>\nYes",
    );
    assert!(!wrapped.contains("<ax_context"), "{wrapped}");

    // E2: a prompt that is only a notification prints nothing at all.
    let notification = hook(
        repo,
        home,
        "<task-notification>\n<summary>how does capture_affected_paths work</summary>\n</task-notification>",
    );
    assert_eq!(notification, "");

    // E3 through the hook: plain words are not looked up as symbols.
    let plain = hook(repo, home, "Ok");
    assert!(!plain.contains("<ax_context"), "{plain}");
}
