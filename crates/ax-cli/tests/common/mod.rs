//! Helpers shared by the `ax` integration tests.
//!
//! Each test binary compiles this module on its own and uses only part of it.
#![allow(dead_code)]

use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The `ax` binary cargo built for these tests.
pub fn ax_bin() -> PathBuf {
    std::env::var_os("CARGO_BIN_EXE_ax")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut p = std::env::current_exe().unwrap();
            p.pop();
            if p.ends_with("deps") {
                p.pop();
            }
            p.join("ax")
        })
}

/// Run `ax` in `root` with `envs` set on that child only, optionally feeding
/// `stdin`. Returns `(exit code, stdout, stderr)`.
pub fn run_ax_env(
    root: &Path,
    args: &[&str],
    stdin: Option<&str>,
    envs: &[(&str, &OsStr)],
) -> (i32, String, String) {
    let mut cmd = Command::new(ax_bin());
    cmd.current_dir(root).args(args);
    for (name, value) in envs {
        cmd.env(name, value);
    }
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ax");
    if let Some(input) = stdin {
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

/// Run one git command in `root` and fail the test when it does not succeed.
pub fn git(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .args(args)
        .current_dir(root)
        .output()
        .unwrap_or_else(|e| panic!("git {args:?} could not start (is git installed?): {e}"));
    assert!(
        out.status.success(),
        "git {args:?} failed: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Turn `root` into a git repository with one commit of its current files.
pub fn git_init_commit(root: &Path) {
    git(root, &["init", "-q"]);
    git(root, &["config", "user.email", "ax@test"]);
    git(root, &["config", "user.name", "ax"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "init"]);
}
