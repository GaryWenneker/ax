//! Path exclusion rules of the index, usable outside the directory walk.
//!
//! `scan_files` walks the tree with the `ignore` crate, which applies the
//! builtin skip list, dot-directory rule, `ax.json` excludes and gitignore files
//! while it walks. Callers that receive single paths (the file watcher, the
//! file-change memory capture) never walk, so they get the same rules here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::Match;

use crate::orchestrator::{build_exclude_matcher, keep_scan_entry_name};

/// Decides whether a project-relative path is one the index excludes.
///
/// Holds a cache of `.gitignore` matchers per directory, so build one filter
/// per batch of paths, not one per path.
pub struct IndexPathFilter {
    root: PathBuf,
    exclude: Option<Gitignore>,
    info_exclude: Option<Gitignore>,
    global: Option<Gitignore>,
    dir_ignores: HashMap<PathBuf, Option<Gitignore>>,
}

impl IndexPathFilter {
    /// `exclude` is the `ax.json` exclude list, as passed to `IndexOptions`.
    pub fn new(project_root: &Path, exclude: &[String]) -> Self {
        let root = project_root.to_path_buf();
        // Both files hold patterns relative to the project root, so root them there.
        let info_exclude = git_info_exclude(&root).and_then(|path| {
            let mut builder = GitignoreBuilder::new(&root);
            builder.add(path);
            builder.build().ok().filter(|matcher| !matcher.is_empty())
        });
        let (global, _) = GitignoreBuilder::new(&root).build_global();
        Self {
            exclude: build_exclude_matcher(&root, exclude),
            info_exclude,
            global: (!global.is_empty()).then_some(global),
            dir_ignores: HashMap::new(),
            root,
        }
    }

    /// True when the index would never hold `rel`: a builtin skipped segment
    /// (`target`, `node_modules`, `.git`, …), a dot-directory or dot-file, an
    /// `ax.json` exclude, a gitignored path, a path outside the project, or a
    /// directory.
    pub fn excludes(&mut self, rel: &str) -> bool {
        let rel = rel.trim().replace('\\', "/");
        let rel = rel.trim_start_matches("./");
        if !stays_in_project(rel) {
            return true;
        }
        if !rel.split('/').all(keep_scan_entry_name) {
            return true;
        }
        if let Some(exclude) = &self.exclude {
            if exclude.matched_path_or_any_parents(rel, false).is_ignore() {
                return true;
            }
        }
        let full = self.root.join(rel);
        if full.is_dir() {
            return true;
        }
        self.gitignored(&full)
    }

    /// The paths of `paths` the index does not exclude, in input order.
    pub fn retain(&mut self, paths: &[String]) -> Vec<String> {
        paths
            .iter()
            .filter(|path| !self.excludes(path))
            .cloned()
            .collect()
    }

    /// Git semantics, approximately: the deepest `.gitignore` with an opinion
    /// wins, then `.git/info/exclude`, then the global excludes file.
    fn gitignored(&mut self, full: &Path) -> bool {
        let mut dirs: Vec<PathBuf> = full
            .ancestors()
            .skip(1)
            .take_while(|dir| dir.starts_with(&self.root))
            .map(Path::to_path_buf)
            .collect();
        // Deepest first.
        dirs.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
        for dir in dirs {
            let matcher = self
                .dir_ignores
                .entry(dir.clone())
                .or_insert_with(|| dir_gitignore(&dir));
            if let Some(matcher) = matcher {
                match matcher.matched_path_or_any_parents(full, false) {
                    Match::Ignore(_) => return true,
                    Match::Whitelist(_) => return false,
                    Match::None => {}
                }
            }
        }
        for matcher in [&self.info_exclude, &self.global].into_iter().flatten() {
            match matcher.matched_path_or_any_parents(full, false) {
                Match::Ignore(_) => return true,
                Match::Whitelist(_) => return false,
                Match::None => {}
            }
        }
        false
    }
}

/// Convenience for a single batch: the paths of `paths` the index keeps.
pub fn retain_indexed_paths(
    project_root: &Path,
    exclude: &[String],
    paths: &[String],
) -> Vec<String> {
    if paths.is_empty() {
        return Vec::new();
    }
    IndexPathFilter::new(project_root, exclude).retain(paths)
}

fn dir_gitignore(dir: &Path) -> Option<Gitignore> {
    let file = dir.join(".gitignore");
    if !file.is_file() {
        return None;
    }
    let (matcher, _) = Gitignore::new(file);
    (!matcher.is_empty()).then_some(matcher)
}

/// `.git/info/exclude`, following a linked worktree's `.git` file to the
/// common git directory, where `info/exclude` lives.
fn git_info_exclude(root: &Path) -> Option<PathBuf> {
    let dot_git = root.join(".git");
    let git_dir = if dot_git.is_dir() {
        dot_git
    } else {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let gitdir = text.strip_prefix("gitdir:")?.trim();
        let gitdir = root.join(gitdir);
        match std::fs::read_to_string(gitdir.join("commondir")) {
            Ok(common) => gitdir.join(common.trim()),
            Err(_) => gitdir,
        }
    };
    let path = git_dir.join("info").join("exclude");
    path.is_file().then_some(path)
}

fn stays_in_project(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join(".git/info")).unwrap();
        std::fs::write(
            root.join(".git/info/exclude"),
            ".claude/worktrees/\nlocal-only/\n",
        )
        .unwrap();
        std::fs::write(root.join(".gitignore"), "/target-dev/\nsecret.rs\n").unwrap();
        std::fs::create_dir_all(root.join("web/public/icons")).unwrap();
        std::fs::write(
            root.join("web/.gitignore"),
            "public/icons/\ntest-results/\n!keep.log\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("src/sub")).unwrap();
        std::fs::write(root.join("src/lib.rs"), "").unwrap();
        dir
    }

    fn excluded(filter: &mut IndexPathFilter, rel: &str) -> bool {
        filter.excludes(rel)
    }

    #[test]
    fn builtin_and_dot_segments_are_excluded() {
        let dir = project();
        let mut filter = IndexPathFilter::new(dir.path(), &[]);
        for rel in [
            "target/debug/build/x/output",
            "site/node_modules/pkg/index.js",
            ".claude/worktrees/a/src/lib.rs",
            ".git/index",
            ".ax/ax.db-wal",
            "dist/app.js",
            "src/.hidden.rs",
        ] {
            assert!(excluded(&mut filter, rel), "{rel} should be excluded");
        }
        assert!(!excluded(&mut filter, ".scripts/wcag/run.ts"));
    }

    #[test]
    fn gitignore_files_at_every_level_apply() {
        let dir = project();
        let mut filter = IndexPathFilter::new(dir.path(), &[]);
        assert!(excluded(&mut filter, "target-dev/release/build/x/out.rs"));
        assert!(excluded(&mut filter, "secret.rs"));
        assert!(excluded(&mut filter, "web/public/icons/a.svg"));
        assert!(excluded(
            &mut filter,
            "web/test-results/.playwright-artifacts-0/x.webm"
        ));
        assert!(excluded(&mut filter, "local-only/notes.rs"));
        assert!(!excluded(&mut filter, "web/keep.log"));
        assert!(!excluded(&mut filter, "src/lib.rs"));
        assert!(
            !excluded(&mut filter, "src/deleted.rs"),
            "deleted files still count"
        );
    }

    #[test]
    fn ax_json_excludes_directories_and_escapes_apply() {
        let dir = project();
        let mut filter = IndexPathFilter::new(dir.path(), &["generated/".to_string()]);
        assert!(excluded(&mut filter, "generated/api.rs"));
        assert!(
            excluded(&mut filter, "src/sub"),
            "directories carry no file change"
        );
        assert!(excluded(&mut filter, "../outside.rs"));
        assert!(excluded(&mut filter, "/etc/passwd"));
        assert!(excluded(&mut filter, ""));
        assert_eq!(
            retain_indexed_paths(
                dir.path(),
                &[],
                &[
                    "src/lib.rs".into(),
                    "target/x.rs".into(),
                    "src/new.rs".into()
                ]
            ),
            vec!["src/lib.rs".to_string(), "src/new.rs".to_string()]
        );
    }

    #[test]
    fn linked_worktree_reads_the_common_info_exclude() {
        let main = project();
        let wt = tempfile::tempdir().unwrap();
        let gitdir = main.path().join(".git/worktrees/wt");
        std::fs::create_dir_all(&gitdir).unwrap();
        std::fs::write(gitdir.join("commondir"), "../..\n").unwrap();
        std::fs::write(
            wt.path().join(".git"),
            format!("gitdir: {}\n", gitdir.display()),
        )
        .unwrap();
        let mut filter = IndexPathFilter::new(wt.path(), &[]);
        assert!(excluded(&mut filter, "local-only/x.rs"));
        assert!(!excluded(&mut filter, "src/x.rs"));
    }
}
