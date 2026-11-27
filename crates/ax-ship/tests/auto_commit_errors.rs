//! A run that fails after the auto-commit checkpoint must undo the
//! checkpoint (with `revert_on_fail`) and close the run log.

use std::path::Path;
use std::process::Command;

use ax_ship::{read_run_log, ShipEventBus, ShipPipeline};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[tokio::test]
async fn a_diff_error_after_the_checkpoint_reverts_it() {
    let dir = tempfile::tempdir().unwrap();
    let ws = dir.path();
    git(ws, &["init", "-q", "-b", "main"]);
    git(ws, &["config", "commit.gpgsign", "false"]);
    std::fs::write(ws.join("base.txt"), "base\n").unwrap();
    git(ws, &["add", "base.txt"]);
    git(ws, &["commit", "-q", "-m", "base"]);
    // A branch with no history in common with `main`, so `main...HEAD` has
    // no merge base and the diff step fails.
    git(ws, &["checkout", "-q", "--orphan", "work"]);
    git(ws, &["rm", "-q", "-rf", "--cached", "."]);
    std::fs::remove_file(ws.join("base.txt")).unwrap();
    std::fs::write(ws.join("lib.rs"), "pub fn a() {}\n").unwrap();
    std::fs::write(ws.join(".gitignore"), ".ax/\n").unwrap();
    git(ws, &["add", "lib.rs", ".gitignore"]);
    git(ws, &["commit", "-q", "-m", "work"]);
    let head_before = git(ws, &["rev-parse", "HEAD"]);

    // Uncommitted work the checkpoint will commit.
    std::fs::write(ws.join("lib.rs"), "pub fn a() {}\npub fn b() {}\n").unwrap();

    drop(ax_core::Ax::init(ws).await.expect("ax init"));

    let mut config = ax_remote::ShipConfig::default();
    config.ship.target_branch = "main".into();
    config.auto_commit.enabled = true;
    config.auto_commit.revert_on_fail = true;
    config.sonar.enabled = false;

    let pipeline = ShipPipeline::new(ws.to_path_buf(), config, ShipEventBus::new(64));
    let err = pipeline
        .run_evaluate()
        .await
        .expect_err("the diff step must fail without a merge base");
    assert!(err.contains("merge base"), "unexpected error: {err}");

    assert_eq!(
        git(ws, &["rev-parse", "HEAD"]),
        head_before,
        "the checkpoint commit must be reverted"
    );
    let status = git(ws, &["status", "--porcelain", "--", "lib.rs"]);
    assert!(
        status.contains("lib.rs"),
        "the change stays uncommitted: {status}"
    );

    let log = read_run_log(ws);
    assert!(log.finished_at.is_some(), "the run log must be closed");
    assert!(!log.ok);
}
