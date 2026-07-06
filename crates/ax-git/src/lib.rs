//! Git diff and blame utilities.

mod diff;
mod blame;
mod node_map;

pub use diff::{changed_files, diff_vs_base, ChangedFile, ChangedHunk, DiffResult, GitContext};
pub use blame::{blame_authors, aggregate_authors, BlameLine};
pub use node_map::{map_hunks_to_nodes, map_files_to_nodes, DirtyNode};

use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
    #[error("not a git repository: {0}")]
    NotRepo(String),
    #[error("reference not found: {0}")]
    RefNotFound(String),
    #[error("{0}")]
    Other(String),
}

pub fn open_repo(path: &Path) -> Result<(), GitError> {
    if !path.join(".git").exists() {
        return Err(GitError::NotRepo(path.display().to_string()));
    }
    Ok(())
}

pub fn current_branch(repo_path: &Path) -> Result<Option<String>, GitError> {
    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .map_err(|e| GitError::Other(e.to_string()))?;
    if !output.status.success() {
        return Err(GitError::NotRepo(repo_path.display().to_string()));
    }
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if name == "HEAD" {
        Ok(None)
    } else {
        Ok(Some(name))
    }
}
