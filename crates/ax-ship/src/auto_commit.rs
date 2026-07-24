//! Opt-in Aider-style auto-commit/rollback checkpointing around a quality-gate run.
//!
//! When `[auto_commit] enabled = true` in `ship.toml`, `ShipPipeline::run_evaluate`
//! commits any uncommitted working-tree changes *before* running the gate (so
//! diff/TIA/Sonar evaluate them as part of branch history), then — only if the
//! gate fails and `revert_on_fail = true` — undoes that specific commit.
//!
//! Safety invariant: rollback is always `git reset --mixed`, never `--hard`.
//! File contents on disk are never discarded, only un-committed, and only the
//! exact commit this run created is ever touched (refuses if HEAD moved).

use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Checkpoint {
    pub repo: String,
    pub sha: String,
}

/// Commit all uncommitted changes in `git_root` under `message`. Returns
/// `None` (no-op) if the working tree was already clean.
pub fn create_checkpoint(git_root: &Path, repo_label: &str, message: &str) -> Result<Option<Checkpoint>, String> {
    if !has_uncommitted_changes(git_root)? {
        return Ok(None);
    }
    run_git(git_root, &["add", "-A"])?;
    run_git(git_root, &["commit", "-m", message])?;
    let sha = run_git(git_root, &["rev-parse", "HEAD"])?.trim().to_string();
    Ok(Some(Checkpoint {
        repo: repo_label.to_string(),
        sha,
    }))
}

/// Undo `checkpoint` via `git reset --mixed HEAD~1` — but only if it is still
/// HEAD (nothing else has been committed in that repo since). Never destroys
/// file contents: changes stay on disk, simply uncommitted again.
pub fn revert_checkpoint(git_root: &Path, checkpoint: &Checkpoint) -> Result<(), String> {
    let head = run_git(git_root, &["rev-parse", "HEAD"])?.trim().to_string();
    if head != checkpoint.sha {
        return Err(format!(
            "refusing to revert checkpoint {} in {} — HEAD has moved to {head} since it was created",
            checkpoint.sha, checkpoint.repo
        ));
    }
    run_git(git_root, &["reset", "--mixed", "HEAD~1"])?;
    Ok(())
}

fn has_uncommitted_changes(git_root: &Path) -> Result<bool, String> {
    let out = run_git(git_root, &["status", "--porcelain"])?;
    Ok(!out.trim().is_empty())
}

fn run_git(git_root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(git_root)
        .args(args)
        .output()
        .map_err(|e| format!("git {}: {e}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = Command::new("git").current_dir(dir.path()).args(args).status().unwrap();
            assert!(status.success(), "git {args:?} failed");
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "init"]);
        dir
    }

    #[test]
    fn no_checkpoint_when_working_tree_is_clean() {
        let dir = init_repo();
        let cp = create_checkpoint(dir.path(), "repo", "ax: checkpoint").unwrap();
        assert!(cp.is_none());
    }

    #[test]
    fn checkpoint_commits_uncommitted_changes() {
        let dir = init_repo();
        std::fs::write(dir.path().join("a.txt"), "two\n").unwrap();
        let cp = create_checkpoint(dir.path(), "repo", "ax: checkpoint")
            .unwrap()
            .expect("checkpoint created");
        let status = run_git(dir.path(), &["status", "--porcelain"]).unwrap();
        assert!(status.trim().is_empty(), "working tree should be clean after commit");
        let head = run_git(dir.path(), &["rev-parse", "HEAD"]).unwrap().trim().to_string();
        assert_eq!(head, cp.sha);
    }

    #[test]
    fn revert_restores_uncommitted_state_without_losing_content() {
        let dir = init_repo();
        std::fs::write(dir.path().join("a.txt"), "two\n").unwrap();
        let cp = create_checkpoint(dir.path(), "repo", "ax: checkpoint").unwrap().unwrap();

        revert_checkpoint(dir.path(), &cp).unwrap();

        // File content must survive the revert — only the commit is undone.
        assert_eq!(std::fs::read_to_string(dir.path().join("a.txt")).unwrap(), "two\n");
        let status = run_git(dir.path(), &["status", "--porcelain"]).unwrap();
        assert!(!status.trim().is_empty(), "reverted checkpoint should leave changes uncommitted");
    }

    #[test]
    fn revert_refuses_if_head_moved_since_checkpoint() {
        let dir = init_repo();
        std::fs::write(dir.path().join("a.txt"), "two\n").unwrap();
        let cp = create_checkpoint(dir.path(), "repo", "ax: checkpoint").unwrap().unwrap();

        std::fs::write(dir.path().join("b.txt"), "three\n").unwrap();
        create_checkpoint(dir.path(), "repo", "ax: checkpoint 2").unwrap();

        let result = revert_checkpoint(dir.path(), &cp);
        assert!(result.is_err(), "must refuse to revert a checkpoint that is no longer HEAD");
    }
}
