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
    let base_oid = git_output(repo_path, &["rev-parse", &format!("{base_ref}^{{commit}}")])
        .or_else(|_| git_output(repo_path, &["rev-parse", base_ref]))?;

    let names = git_raw_output(
        repo_path,
        &[
            "diff",
            "--no-ext-diff",
            "--name-status",
            "-z",
            &format!("{base_ref}...HEAD"),
        ],
    )?;
    let files = parse_name_status_z(&names);

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
    let text = git_raw_output(
        repo_path,
        &[
            "diff",
            "--no-ext-diff",
            "--no-color",
            "--unified=0",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            &format!("{base_ref}...HEAD"),
        ],
    )?;
    Ok(parse_unified_diff(&text))
}

/// Run git with `core.quotePath=false` (non-ASCII paths stay readable) and
/// return stdout untrimmed, since NUL-separated output and paths may end in
/// whitespace.
fn git_raw_output(repo_path: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .args(["-c", "core.quotePath=false"])
        .args(args)
        .output()
        .map_err(|e| GitError::Other(format!("git {} failed: {e}", args[0])))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(GitError::Other(format!("git {}: {stderr}", args[0])));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse `git diff --name-status -z`: `STATUS NUL path NUL`, or for renames
/// and copies `STATUS NUL old NUL new NUL`. The new path is the one reported.
fn parse_name_status_z(text: &str) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    let mut fields = text.split('\0');
    while let Some(status) = fields.next() {
        let status = status.trim();
        if status.is_empty() {
            continue;
        }
        let kind = status.chars().next().unwrap_or('M');
        let mut path = fields.next().unwrap_or("");
        if matches!(kind, 'R' | 'C') {
            path = fields.next().unwrap_or(path);
        }
        if path.is_empty() {
            continue;
        }
        let change = match kind {
            'A' => "added",
            'D' => "deleted",
            'R' => "renamed",
            'C' => "copied",
            _ => "modified",
        };
        files.push(ChangedFile {
            path: path.to_string(),
            change: change.to_string(),
        });
    }
    files
}

fn git_output(repo_path: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = std::process::Command::new("git")
        .current_dir(repo_path)
        .args(args)
        .output()
        .map_err(|e| GitError::Other(e.to_string()))?;
    if !output.status.success() {
        return Err(GitError::Other(
            String::from_utf8_lossy(&output.stderr).into(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn parse_unified_diff(text: &str) -> Vec<ChangedHunk> {
    let mut hunks = Vec::new();
    let mut current_path = String::new();

    for line in text.lines() {
        if let Some(target) = line.strip_prefix("+++ ") {
            // `/dev/null` (deleted file) leaves no path, so its hunks are skipped.
            current_path = diff_target_path(target).unwrap_or_default();
            continue;
        }
        if line.starts_with("@@ ") && !current_path.is_empty() {
            if let Some(hunk) = parse_hunk_header(line, &current_path) {
                hunks.push(hunk);
            }
        }
    }
    hunks
}

/// Path from a `+++ ` line: `b/path`, `b/path<TAB>` (git adds a tab when the
/// name has a space) or a C-quoted `"b/..."`. `None` for `/dev/null`.
fn diff_target_path(target: &str) -> Option<String> {
    let target = target.strip_suffix('\t').unwrap_or(target);
    let unquoted = if target.starts_with('"') {
        unquote_c_style(target)?
    } else {
        target.to_string()
    };
    unquoted.strip_prefix("b/").map(str::to_string)
}

/// Undo git's C-style path quoting (`"a\tb\303\251"`).
fn unquote_c_style(quoted: &str) -> Option<String> {
    let inner = quoted.strip_prefix('"')?.strip_suffix('"')?;
    let bytes = inner.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'\\' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let next = *bytes.get(i + 1)?;
        i += 2;
        match next {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'a' => out.push(0x07),
            b'b' => out.push(0x08),
            b'f' => out.push(0x0c),
            b'v' => out.push(0x0b),
            b'0'..=b'7' => {
                let digits = bytes.get(i - 1..i + 2)?;
                let text = std::str::from_utf8(digits).ok()?;
                out.push(u8::from_str_radix(text, 8).ok()?);
                i += 2;
            }
            other => out.push(other),
        }
    }
    String::from_utf8(out).ok()
}

/// Parse `@@ -a,b +c,d @@ [function context]`. Git appends the enclosing
/// function line after the second `@@`, so the ranges end there.
fn parse_hunk_header(line: &str, path: &str) -> Option<ChangedHunk> {
    let rest = line.strip_prefix("@@ ")?;
    let inner = rest.split(" @@").next()?;
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
    let lines: u32 = it.next().and_then(|n| n.parse().ok()).unwrap_or(1);
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
    fn hunk_header_with_function_context_is_parsed() {
        let h = parse_hunk_header("@@ -40 +40 @@ pub fn run() {", "src/lib.rs").unwrap();
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (40, 1, 40, 1)
        );
        let h = parse_hunk_header("@@ -10,5 +12,0 @@ fn a() -> u8 { 1 @@ 2 }", "x").unwrap();
        assert_eq!((h.new_start, h.new_lines), (12, 0));
    }

    #[test]
    fn name_status_z_takes_the_new_path_of_renames_and_copies() {
        let files = parse_name_status_z(
            "R100\0src/a.rs\0src/b.rs\0C75\0x.rs\0y.rs\0M\0my file.rs\0D\0gone.rs\0",
        );
        let got: Vec<(&str, &str)> = files
            .iter()
            .map(|f| (f.path.as_str(), f.change.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("src/b.rs", "renamed"),
                ("y.rs", "copied"),
                ("my file.rs", "modified"),
                ("gone.rs", "deleted"),
            ]
        );
    }

    #[test]
    fn target_paths_are_unquoted() {
        assert_eq!(
            diff_target_path("b/my file.rs\t").as_deref(),
            Some("my file.rs")
        );
        assert_eq!(
            diff_target_path("\"b/caf\\303\\251 \\\"q\\\".rs\"").as_deref(),
            Some("café \"q\".rs")
        );
        assert_eq!(diff_target_path("/dev/null"), None);
    }

    #[test]
    fn parse_hunk_header_works() {
        let h = parse_hunk_header("@@ -10,5 +10,6 @@", "src/lib.rs").unwrap();
        assert_eq!(h.old_start, 10);
        assert_eq!(h.new_lines, 6);
    }
}
