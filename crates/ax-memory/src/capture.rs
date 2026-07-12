//! Auto-capture memories from git history.
//!
//! Commit subjects/bodies carry the "why" behind changes. This mines recent
//! non-merge commits into `kind = "git"` memories, linked to the files each
//! commit touched. Memory ids are derived from the commit hash so re-running
//! capture never duplicates.

use std::path::Path;

use serde::Serialize;
use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

#[derive(Debug, Clone, Serialize)]
pub struct GitCaptureResult {
    pub scanned: usize,
    pub captured: usize,
    pub skipped_existing: usize,
    pub skipped_trivial: usize,
}

struct CommitInfo {
    hash: String,
    subject: String,
    body: String,
    author_time_ms: i64,
    files: Vec<String>,
}

/// Subjects that carry no "why" and are not worth remembering.
fn is_trivial(subject: &str) -> bool {
    let s = subject.trim().to_lowercase();
    s.len() < 12
        || s.starts_with("wip")
        || s.starts_with("merge")
        || s.starts_with("bump")
        || s.starts_with("typo")
        || s.starts_with("format")
        || s.starts_with("lint")
}

fn run_git_log(repo_root: &Path, limit: usize) -> Result<Vec<CommitInfo>, String> {
    // \x1f separates fields, \x1e separates commits; --name-only appends the file list.
    let output = std::process::Command::new("git")
        .current_dir(repo_root)
        .args([
            "log",
            "--no-merges",
            &format!("-n{limit}"),
            "--pretty=format:%x1e%H%x1f%at%x1f%s%x1f%b%x1f",
            "--name-only",
        ])
        .output()
        .map_err(|e| format!("git log failed: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git log failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();
    for record in text.split('\x1e').filter(|r| !r.trim().is_empty()) {
        let mut fields = record.splitn(5, '\x1f');
        let hash = fields.next().unwrap_or("").trim().to_string();
        let at: i64 = fields.next().unwrap_or("0").trim().parse().unwrap_or(0);
        let subject = fields.next().unwrap_or("").trim().to_string();
        let body = fields.next().unwrap_or("").trim().to_string();
        let files: Vec<String> = fields
            .next()
            .unwrap_or("")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .take(20)
            .map(String::from)
            .collect();
        if hash.is_empty() || subject.is_empty() {
            continue;
        }
        commits.push(CommitInfo {
            hash,
            subject,
            body,
            author_time_ms: at * 1000,
            files,
        });
    }
    Ok(commits)
}

pub async fn capture_git_history(
    pool: &SqlitePool,
    repo_root: &Path,
    limit: usize,
) -> Result<GitCaptureResult, AxError> {
    let commits = run_git_log(repo_root, limit.clamp(1, 500))
        .map_err(|e| AxError::Other(e))?;

    let mut result = GitCaptureResult {
        scanned: commits.len(),
        captured: 0,
        skipped_existing: 0,
        skipped_trivial: 0,
    };

    for commit in commits {
        if is_trivial(&commit.subject) {
            result.skipped_trivial += 1;
            continue;
        }
        let id = format!("git-{}", &commit.hash[..commit.hash.len().min(12)]);
        let body = if commit.body.is_empty() {
            commit.subject.clone()
        } else {
            format!("{}\n\n{}", commit.subject, commit.body)
        };
        let embedding = crate::embed::embedding_to_blob(&crate::embed::embed_text(&format!(
            "{} {}",
            commit.subject, commit.body
        )));
        let outcome = sqlx::query(
            r#"INSERT OR IGNORE INTO memories
               (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding)
               VALUES (?, 'git', ?, ?, '["git"]', ?, 0.8, 'git', ?, ?, ?)"#,
        )
        .bind(&id)
        .bind(commit.subject.chars().take(120).collect::<String>())
        .bind(&body)
        .bind(serde_json::to_string(&commit.files).unwrap_or_else(|_| "[]".into()))
        .bind(commit.author_time_ms)
        .bind(commit.author_time_ms)
        .bind(embedding)
        .execute(pool)
        .await
        .map_err(|e| AxError::Database(DatabaseError::new(e.to_string())))?;

        if outcome.rows_affected() > 0 {
            result.captured += 1;
        } else {
            result.skipped_existing += 1;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trivial_subjects_are_skipped() {
        assert!(is_trivial("wip"));
        assert!(is_trivial("Merge branch 'main'"));
        assert!(is_trivial("typo"));
        assert!(!is_trivial("Fix incremental sync to hash file contents"));
    }
}
