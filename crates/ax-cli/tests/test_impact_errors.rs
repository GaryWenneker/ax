//! `ax test-impact` fails when git cannot compute the diff, instead of
//! reporting an empty change set that would make CI skip every test.

use std::path::Path;
use std::process::{Command, Output, Stdio};

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed");
}

fn ax(dir: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ax"))
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("AX_HOME_DIR", home)
        .env("USERPROFILE", home)
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn an_unknown_base_ref_is_an_error() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let p = repo.path();
    git(p, &["init", "-q", "-b", "trunk"]);
    git(p, &["config", "user.email", "t@example.com"]);
    git(p, &["config", "user.name", "t"]);
    std::fs::write(p.join("main.rs"), "fn main() {}\n").unwrap();
    git(p, &["add", "."]);
    git(p, &["commit", "-q", "-m", "init"]);
    let init = ax(p, home.path(), &["init"]);
    assert!(
        init.status.success(),
        "ax init: {}",
        String::from_utf8_lossy(&init.stderr)
    );

    let out = ax(p, home.path(), &["test-impact", "--base", "no-such-branch"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "expected failure, got stdout: {stdout}"
    );
    assert!(!stdout.contains("Changed files: 0"), "{stdout}");
    assert!(stderr.contains("no-such-branch"), "{stderr}");

    let ok = ax(p, home.path(), &["test-impact", "--base", "trunk"]);
    assert!(
        ok.status.success(),
        "known base: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
}
