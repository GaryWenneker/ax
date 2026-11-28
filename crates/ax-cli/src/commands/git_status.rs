//! `git status --porcelain=v1 -z` parsing shared by the turn and stop hooks.
//!
//! Porcelain paths are always relative to the repository top level, not to
//! the directory git ran in, so callers join them onto [`toplevel`].

use std::path::{Path, PathBuf};
use std::process::Command;

/// Arguments for a status listing that names every untracked file (not just
/// its new directory) and does not C-quote unusual file names.
pub const STATUS_ARGS: [&str; 4] = ["status", "--porcelain=v1", "-z", "--untracked-files=all"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    /// The two-letter `XY` status, such as ` M`, `??` or `R `.
    pub status: String,
    /// Path relative to the repository top level (the new path for a rename).
    pub path: String,
    /// The old path of a rename or copy.
    pub source: Option<String>,
}

impl StatusEntry {
    pub fn is_deleted(&self) -> bool {
        self.status.contains('D')
    }

    pub fn is_untracked(&self) -> bool {
        self.status.starts_with('?')
    }
}

/// Parses NUL-separated `git status --porcelain=v1 -z` output.
pub fn parse_porcelain_z(raw: &[u8]) -> Vec<StatusEntry> {
    let text = String::from_utf8_lossy(raw);
    let mut entries = text.split('\0');
    let mut out = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.len() < 4 || !entry.is_char_boundary(3) {
            continue;
        }
        let (status, path) = entry.split_at(3);
        let status = status[..2].to_string();
        // Either column can report a rename or copy (a worktree-side rename
        // shows as ` R`), and both carry the original path as the next field.
        let source = if status.contains(['R', 'C']) {
            entries.next().map(str::to_string)
        } else {
            None
        };
        out.push(StatusEntry {
            status,
            path: path.to_string(),
            source,
        });
    }
    out
}

/// The repository top level for `dir`, or `None` outside a git work tree.
pub fn toplevel(dir: &Path) -> Option<PathBuf> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let top = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!top.is_empty()).then(|| PathBuf::from(top))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modified_untracked_and_renamed() {
        let raw = b" M src/lib.rs\0?? caf\xc3\xa9 x.rs\0R  new.rs\0old.rs\0 D gone.rs\0";
        let got = parse_porcelain_z(raw);
        assert_eq!(got.len(), 4);
        assert_eq!(got[0].path, "src/lib.rs");
        assert_eq!(got[1].path, "caf\u{e9} x.rs");
        assert!(got[1].is_untracked());
        assert_eq!(got[2].path, "new.rs");
        assert_eq!(got[2].source.as_deref(), Some("old.rs"));
        assert!(got[3].is_deleted());
    }

    #[test]
    fn parses_a_worktree_side_rename() {
        let raw = b" R new.rs\0old.rs\0 M other.rs\0";
        let got = parse_porcelain_z(raw);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].status, " R");
        assert_eq!(got[0].path, "new.rs");
        assert_eq!(got[0].source.as_deref(), Some("old.rs"));
        assert_eq!(got[1].path, "other.rs");
    }

    #[test]
    fn ignores_short_and_empty_entries() {
        assert!(parse_porcelain_z(b"").is_empty());
        assert!(parse_porcelain_z(b"?? \0\0").is_empty());
    }
}
