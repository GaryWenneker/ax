//! Git diff via git CLI with gix for repository discovery.

use std::path::Path;

use crate::GitError;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitContext {
    pub head_branch: Option<String>,
    pub base_ref: String,
    pub head_oid: String,
    pub base_oid: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangedFile {
    pub path: String,
    pub change: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangedHunk {
    pub path: String,
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DiffResult {
    pub context: GitContext,
    pub files: Vec<ChangedFile>,
    pub hunks: Vec<ChangedHunk>,
}

pub fn diff_vs_base(repo_path: &Path, base_ref: &str) -> Result<DiffResult, GitError> {
    let head_branch = current_branch(repo_path)?;
    let head_oid = git_output(repo_path, &["rev-parse", "HEAD"])?;
    let base_oid = git_output(
        repo_path,
        &["rev-parse", &format!("{base_ref}^{{commit}}")],
    )
    .or_else(|_| git_output(repo_path, &["rev-parse", base_ref]))?;

    let names = git_output(repo_path, &["diff", "--name-status", &format!("{base_ref}...HEAD")])?;
    let mut files = Vec::new();
    for line in names.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, '\t');
        let status = parts.next().unwrap_or("M");
        let path = parts.next().unwrap_or("").to_string();
        let change = match status.chars().next() {
            Some('A') => "added",
            Some('D') => "deleted",
            Some('R') => "renamed",
            _ => "modified",
        };
        files.push(ChangedFile {
            path,
            change: change.to_string(),
        });
    }

    let hunks = line_hunks_via_git(repo_path, base_ref)?;

    Ok(DiffResult {
        context: GitContext {
            head_branch,
            base_ref: base_ref.to_string(),
            head_oid,
            base_oid,
        },
        files,
        hunks,
    })
}

pub fn changed_files(repo_path: &Path, base_ref: &str) -> Result<Vec<String>, GitError> {
    Ok(diff_vs_base(repo_path, base_ref)?
        .files
        .into_iter()
        .filter(|f| f.change != "deleted")
        .map(|f| f.path)
        .collect())
}

fn line_hunks_via_git(repo_path: &Path, base_ref: &str) -> Result<Vec<ChangedHunk>, GitError> {
    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["diff", "--unified=0", &format!("{base_ref}...HEAD")])
        .output()
        .map_err(|e| GitError::Other(format!("git diff failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::Other(format!("git diff: {stderr}")));
    }

    Ok(parse_unified_diff(&String::from_utf8_lossy(&output.stdout)))
}

fn git_output(repo_path: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .args(args)
        .output()
        .map_err(|e| GitError::Other(e.to_string()))?;
    if !output.status.success() {
        return Err(GitError::Other(String::from_utf8_lossy(&output.stderr).into()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn parse_unified_diff(text: &str) -> Vec<ChangedHunk> {
    let mut hunks = Vec::new();
    let mut current_path = String::new();

    for line in text.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            current_path = path.to_string();
            continue;
        }
        if line.starts_with("@@ ") {
            if let Some(hunk) = parse_hunk_header(line, &current_path) {
                hunks.push(hunk);
            }
        }
    }
    hunks
}

fn parse_hunk_header(line: &str, path: &str) -> Option<ChangedHunk> {
    let inner = line.strip_prefix("@@ ")?.strip_suffix(" @@")?;
    let parts: Vec<&str> = inner.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }
    let (old_start, old_lines) = parse_range(parts[0].trim_start_matches('-'))?;
    let (new_start, new_lines) = parse_range(parts[1].trim_start_matches('+'))?;
    Some(ChangedHunk {
        path: path.to_string(),
        old_start,
        old_lines,
        new_start,
        new_lines,
    })
}

fn parse_range(s: &str) -> Option<(u32, u32)> {
    let mut it = s.split(',');
    let start: u32 = it.next()?.parse().ok()?;
    let lines: u32 = it.next().map(|n| n.parse().ok()).flatten().unwrap_or(1);
    Some((start, lines))
}

fn current_branch(repo_path: &Path) -> Result<Option<String>, GitError> {
    let name = git_output(repo_path, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if name == "HEAD" {
        Ok(None)
    } else {
        Ok(Some(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hunk_header_works() {
        let h = parse_hunk_header("@@ -10,5 +10,6 @@", "src/lib.rs").unwrap();
        assert_eq!(h.old_start, 10);
        assert_eq!(h.new_lines, 6);
    }
}
