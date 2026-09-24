//! Git hooks run `ax sync --quiet` and `ax ship --evaluate --quiet` in every checkout,
//! including ones that were never initialized (a fresh git worktree).

use std::path::Path;
use std::process::{Command, Output};

fn ax(dir: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ax"))
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("NO_COLOR", "1")
        .output()
        .expect("run ax")
}

fn git_repo() -> (tempfile::TempDir, tempfile::TempDir) {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(repo.path())
        .status()
        .unwrap();
    assert!(status.success());
    (repo, home)
}

fn assert_silent_success(out: &Output, repo: &Path) {
    assert!(out.status.success(), "exit {:?}", out.status.code());
    assert!(
        out.stdout.is_empty(),
        "stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        out.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!repo.join(".ax").exists(), ".ax/ was created");
}

#[test]
fn a1_quiet_sync_without_ax_is_a_silent_no_op() {
    let (repo, home) = git_repo();
    let out = ax(repo.path(), home.path(), &["sync", "--quiet"]);
    assert_silent_success(&out, repo.path());
}

#[test]
fn a2_quiet_ship_evaluate_without_ax_is_a_silent_no_op() {
    let (repo, home) = git_repo();
    let out = ax(repo.path(), home.path(), &["ship", "--evaluate", "--quiet"]);
    assert_silent_success(&out, repo.path());
}

#[test]
fn a3_sync_and_ship_without_quiet_still_report_the_missing_project() {
    let (repo, home) = git_repo();
    for args in [&["sync"][..], &["ship", "--evaluate"][..]] {
        let out = ax(repo.path(), home.path(), args);
        assert_eq!(out.status.code(), Some(1), "{args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains("project not initialized - run ax init"),
            "{args:?}: {stderr}"
        );
    }
}
