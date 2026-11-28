//! `ax policy pull`: clone a remote policy registry and merge its rules and skills.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Upper bounds for what one pull may copy out of an untrusted clone.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PullLimits {
    pub max_files: usize,
    pub max_bytes: u64,
    pub max_depth: usize,
}

impl Default for PullLimits {
    fn default() -> Self {
        Self {
            max_files: 5_000,
            max_bytes: 50 * 1024 * 1024,
            max_depth: 16,
        }
    }
}

/// What a merge did with each rule and skill it found in the clone.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct PullReport {
    pub copied_rules: usize,
    pub copied_skills: usize,
    pub unchanged: usize,
    /// Local items that differ from the remote and were kept (no `--force`).
    pub kept_local: Vec<String>,
    /// Symlinks in the clone that were not followed.
    pub skipped_symlinks: Vec<String>,
}

/// Pull a remote policy registry into the project and re-index.
///
/// The clone lands in `.ax/policy/vendored/<name>/`; its `rules/*.mdc` and
/// `skills/<name>/` are merged into the project's agents folder. A local rule
/// or skill that differs from the remote one is kept unless `force` is set.
pub async fn run_pull(
    url: String,
    path: Option<String>,
    name: Option<String>,
    force: bool,
) -> Result<(), String> {
    let root = crate::commands::resolve_project_root(path);
    if !root.join(".ax").is_dir() {
        return Err("project not initialized — run ax init first".into());
    }
    let vendor_name = vendor_name(&url, name)?;
    let dest = root.join(".ax/policy/vendored").join(&vendor_name);
    clone_vendor(&url, &dest)?;

    let policy_src = if dest.join(".ax/policy").is_dir() {
        dest.join(".ax/policy")
    } else if dest.join("policy").is_dir() {
        dest.join("policy")
    } else {
        dest.clone()
    };
    let dest_root = ax_policy::agents_dir(&root);
    let report = merge_pulled_policy(&dest, &policy_src, &dest_root, force, PullLimits::default())?;

    println!(
        "Pulled policy from {url} → .ax/policy/vendored/{vendor_name}/ (copied {} rules, {} skills, {} unchanged) into {}",
        report.copied_rules,
        report.copied_skills,
        report.unchanged,
        dest_root.display()
    );
    for item in &report.kept_local {
        eprintln!(
            "kept local {item}: it differs from the remote copy (pass --force to replace it)"
        );
    }
    for item in &report.skipped_symlinks {
        eprintln!("skipped symlink {item} in the remote policy");
    }

    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let result = ax.index_policy(true).await.map_err(|e| e.to_string())?;
    println!(
        "Re-indexed policy: {} rules, {} skills",
        result.rules_indexed, result.skills_indexed
    );
    Ok(())
}

fn vendor_name(url: &str, name: Option<String>) -> Result<String, String> {
    let raw = name.unwrap_or_else(|| {
        url.trim_end_matches('/')
            .trim_end_matches(".git")
            .rsplit(['/', ':'])
            .next()
            .unwrap_or("remote-policy")
            .to_string()
    });
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        return Err("could not derive vendor name — pass --name".into());
    }
    Ok(cleaned)
}

/// `git clone` arguments. The URL comes after `--` so git never reads it as an option.
pub(crate) fn clone_args(url: &str, dest: &Path) -> Result<Vec<OsString>, String> {
    if url.trim().is_empty() || url.starts_with('-') {
        return Err(format!("not a git URL: {url}"));
    }
    Ok(vec![
        "clone".into(),
        "--depth".into(),
        "1".into(),
        "--quiet".into(),
        "--".into(),
        url.into(),
        dest.as_os_str().to_owned(),
    ])
}

/// Clone `url` next to `dest` and swap it in only after the clone succeeded,
/// so a failed clone leaves the previous vendored copy in place.
pub(crate) fn clone_vendor(url: &str, dest: &Path) -> Result<(), String> {
    let args = clone_args(url, dest)?;
    let parent = dest
        .parent()
        .ok_or_else(|| format!("no parent folder for {}", dest.display()))?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let leaf = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let staging = parent.join(format!(".{leaf}.pull-{}", std::process::id()));
    if staging.exists() {
        std::fs::remove_dir_all(&staging).map_err(|e| e.to_string())?;
    }
    let mut args = args;
    if let Some(last) = args.last_mut() {
        *last = staging.as_os_str().to_owned();
    }
    let status = std::process::Command::new("git")
        .args(&args)
        .status()
        .map_err(|e| format!("failed to run git: {e}"));
    let ok = matches!(status, Ok(s) if s.success());
    if !ok {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(match status {
            Err(e) => e,
            Ok(_) => format!("git clone failed for {url}"),
        });
    }
    if dest.exists() {
        if let Err(e) = std::fs::remove_dir_all(dest) {
            let _ = std::fs::remove_dir_all(&staging);
            return Err(format!("failed to clear {}: {e}", dest.display()));
        }
    }
    std::fs::rename(&staging, dest).map_err(|e| {
        let _ = std::fs::remove_dir_all(&staging);
        format!("failed to move the clone to {}: {e}", dest.display())
    })
}

/// Files read from the clone, counted against [`PullLimits`].
struct Budget {
    limits: PullLimits,
    files: usize,
    bytes: u64,
}

impl Budget {
    fn read(&mut self, path: &Path) -> Result<Vec<u8>, String> {
        self.files += 1;
        if self.files > self.limits.max_files {
            return Err(format!(
                "remote policy has more than {} files; refusing to copy it",
                self.limits.max_files
            ));
        }
        let left = self.limits.max_bytes.saturating_sub(self.bytes);
        let bytes = ax_utils::read_bounded(path, left).map_err(|e| {
            format!(
                "remote policy is larger than {} bytes; refusing to copy it ({e})",
                self.limits.max_bytes
            )
        })?;
        self.bytes += bytes.len() as u64;
        Ok(bytes)
    }
}

type Tree = Vec<(PathBuf, Vec<u8>)>;

/// Read every regular file under `dir`, never following a symlink.
fn collect_tree(
    dir: &Path,
    label: &str,
    rel: &Path,
    depth: usize,
    budget: &mut Budget,
    symlinks: &mut Vec<String>,
    out: &mut Tree,
) -> Result<(), String> {
    if depth > budget.limits.max_depth {
        return Err(format!(
            "{label} is nested deeper than {} folders; refusing to copy it",
            budget.limits.max_depth
        ));
    }
    for entry in sorted_entries(dir)? {
        let name = entry.file_name();
        let child_rel = rel.join(&name);
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        if ft.is_symlink() {
            symlinks.push(format!("{label}/{}", child_rel.display()));
        } else if ft.is_dir() {
            collect_tree(
                &entry.path(),
                label,
                &child_rel,
                depth + 1,
                budget,
                symlinks,
                out,
            )?;
        } else if ft.is_file() {
            out.push((child_rel, budget.read(&entry.path())?));
        }
    }
    Ok(())
}

fn sorted_entries(dir: &Path) -> Result<Vec<std::fs::DirEntry>, String> {
    let mut entries = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|e| e.file_name());
    Ok(entries)
}

/// `sub` under `policy_src`, if present, after checking it resolves inside the clone.
fn source_dir(clone_canon: &Path, policy_src: &Path, sub: &str) -> Result<Option<PathBuf>, String> {
    let dir = policy_src.join(sub);
    if std::fs::symlink_metadata(&dir).is_err() {
        return Ok(None);
    }
    let canon = dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    if !canon.starts_with(clone_canon) {
        return Err(format!(
            "remote policy folder {sub}/ points outside the clone; refusing to copy it"
        ));
    }
    Ok(canon.is_dir().then_some(canon))
}

fn same_tree(dir: &Path, remote: &Tree) -> bool {
    if !std::fs::symlink_metadata(dir).is_ok_and(|m| m.is_dir()) {
        return false;
    }
    let mut budget = Budget {
        limits: PullLimits::default(),
        files: 0,
        bytes: 0,
    };
    let mut local = Vec::new();
    let mut links = Vec::new();
    collect_tree(
        dir,
        "",
        Path::new(""),
        0,
        &mut budget,
        &mut links,
        &mut local,
    )
    .is_ok()
        && links.is_empty()
        && &local == remote
}

fn remove_any(path: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
    .map_err(|e| format!("failed to replace {}: {e}", path.display()))
}

/// Merge the clone's `rules/*.mdc` and `skills/<name>/` into `dest_root`.
///
/// Everything is read (and checked against `limits`) before anything is
/// written. Symlinks are never followed. A local item that differs from the
/// remote one is kept and reported unless `force` is set.
pub(crate) fn merge_pulled_policy(
    clone_root: &Path,
    policy_src: &Path,
    dest_root: &Path,
    force: bool,
    limits: PullLimits,
) -> Result<PullReport, String> {
    let clone_canon = clone_root
        .canonicalize()
        .map_err(|e| format!("{}: {e}", clone_root.display()))?;
    let mut report = PullReport::default();
    let mut budget = Budget {
        limits,
        files: 0,
        bytes: 0,
    };

    let mut rules: Vec<(String, Vec<u8>)> = Vec::new();
    if let Some(dir) = source_dir(&clone_canon, policy_src, "rules")? {
        for entry in sorted_entries(&dir)? {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".mdc") {
                continue;
            }
            let ft = entry.file_type().map_err(|e| e.to_string())?;
            if ft.is_symlink() {
                report.skipped_symlinks.push(format!("rules/{name}"));
            } else if ft.is_file() {
                rules.push((name, budget.read(&entry.path())?));
            }
        }
    }

    let mut skills: Vec<(String, Tree)> = Vec::new();
    if let Some(dir) = source_dir(&clone_canon, policy_src, "skills")? {
        for entry in sorted_entries(&dir)? {
            let name = entry.file_name().to_string_lossy().into_owned();
            let ft = entry.file_type().map_err(|e| e.to_string())?;
            let label = format!("skills/{name}");
            if ft.is_symlink() {
                report.skipped_symlinks.push(label);
            } else if ft.is_dir() {
                let mut tree = Vec::new();
                collect_tree(
                    &entry.path(),
                    &label,
                    Path::new(""),
                    1,
                    &mut budget,
                    &mut report.skipped_symlinks,
                    &mut tree,
                )?;
                skills.push((name, tree));
            }
        }
    }

    let rules_dst = dest_root.join("rules");
    let skills_dst = dest_root.join("skills");
    std::fs::create_dir_all(&rules_dst).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&skills_dst).map_err(|e| e.to_string())?;

    for (name, bytes) in rules {
        let target = rules_dst.join(&name);
        if std::fs::symlink_metadata(&target).is_ok() {
            let same = std::fs::symlink_metadata(&target).is_ok_and(|m| m.is_file())
                && std::fs::read(&target).is_ok_and(|b| b == bytes);
            if same {
                report.unchanged += 1;
                continue;
            }
            if !force {
                report.kept_local.push(format!("rules/{name}"));
                continue;
            }
        }
        ax_utils::atomic_write(&target, &bytes)
            .map_err(|e| format!("failed to write {}: {e}", target.display()))?;
        report.copied_rules += 1;
    }

    for (name, tree) in skills {
        let target = skills_dst.join(&name);
        if std::fs::symlink_metadata(&target).is_ok() {
            if same_tree(&target, &tree) {
                report.unchanged += 1;
                continue;
            }
            if !force {
                report.kept_local.push(format!("skills/{name}"));
                continue;
            }
            remove_any(&target)?;
        }
        for (rel, bytes) in &tree {
            let file = target.join(rel);
            if let Some(parent) = file.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(&file, bytes)
                .map_err(|e| format!("failed to write {}: {e}", file.display()))?;
        }
        report.copied_skills += 1;
    }

    report.skipped_symlinks.sort();
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    /// A clone root with `rules/` and `skills/`, plus an empty destination.
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let clone = tmp.path().join("clone");
        let dest = tmp.path().join("project/.agents");
        fs::create_dir_all(clone.join("rules")).unwrap();
        fs::create_dir_all(clone.join("skills")).unwrap();
        fs::create_dir_all(&dest).unwrap();
        (tmp, clone, dest)
    }

    #[test]
    fn clone_args_end_options_before_the_url() {
        let args = clone_args("https://example.com/p.git", Path::new("/tmp/x")).unwrap();
        let pos = args.iter().position(|a| a == "--").expect("-- present");
        assert_eq!(args[pos + 1], "https://example.com/p.git");
    }

    #[test]
    fn clone_args_reject_a_url_that_looks_like_an_option() {
        assert!(clone_args("--upload-pack=/tmp/x", Path::new("/tmp/x")).is_err());
        assert!(clone_args("-c", Path::new("/tmp/x")).is_err());
    }

    #[test]
    fn failed_clone_keeps_the_previous_vendored_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("vendored/org");
        write(&dest.join("rules/old.mdc"), "old");
        let missing = tmp.path().join("no-such-repo");
        let err = clone_vendor(missing.to_str().unwrap(), &dest);
        assert!(err.is_err());
        assert_eq!(
            fs::read_to_string(dest.join("rules/old.mdc")).unwrap(),
            "old"
        );
    }

    #[test]
    fn successful_clone_replaces_the_vendored_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        write(&repo.join("rules/new.mdc"), "new");
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                .args(args)
                .current_dir(&repo)
                .output()
                .unwrap()
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        let dest = tmp.path().join("vendored/org");
        write(&dest.join("rules/old.mdc"), "old");
        clone_vendor(repo.to_str().unwrap(), &dest).unwrap();
        assert!(!dest.join("rules/old.mdc").exists());
        assert_eq!(
            fs::read_to_string(dest.join("rules/new.mdc")).unwrap(),
            "new"
        );
        let leftovers: Vec<_> = fs::read_dir(dest.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("org")]);
    }

    #[test]
    fn new_rules_and_skills_are_copied() {
        let (_tmp, clone, dest) = fixture();
        write(&clone.join("rules/team.mdc"), "team");
        write(&clone.join("skills/review/SKILL.md"), "skill");
        write(&clone.join("skills/review/refs/a.md"), "a");
        let r = merge_pulled_policy(&clone, &clone, &dest, false, PullLimits::default()).unwrap();
        assert_eq!((r.copied_rules, r.copied_skills), (1, 1));
        assert_eq!(
            fs::read_to_string(dest.join("rules/team.mdc")).unwrap(),
            "team"
        );
        assert_eq!(
            fs::read_to_string(dest.join("skills/review/refs/a.md")).unwrap(),
            "a"
        );
    }

    #[test]
    fn a_changed_local_rule_is_kept_without_force() {
        let (_tmp, clone, dest) = fixture();
        write(&clone.join("rules/english-only.mdc"), "remote");
        write(&dest.join("rules/english-only.mdc"), "local edit");
        let r = merge_pulled_policy(&clone, &clone, &dest, false, PullLimits::default()).unwrap();
        assert_eq!(r.copied_rules, 0);
        assert_eq!(r.kept_local, vec!["rules/english-only.mdc".to_string()]);
        assert_eq!(
            fs::read_to_string(dest.join("rules/english-only.mdc")).unwrap(),
            "local edit"
        );
    }

    #[test]
    fn force_replaces_a_changed_local_rule() {
        let (_tmp, clone, dest) = fixture();
        write(&clone.join("rules/english-only.mdc"), "remote");
        write(&dest.join("rules/english-only.mdc"), "local edit");
        let r = merge_pulled_policy(&clone, &clone, &dest, true, PullLimits::default()).unwrap();
        assert_eq!(r.copied_rules, 1);
        assert_eq!(
            fs::read_to_string(dest.join("rules/english-only.mdc")).unwrap(),
            "remote"
        );
    }

    #[test]
    fn an_identical_local_rule_counts_as_unchanged() {
        let (_tmp, clone, dest) = fixture();
        write(&clone.join("rules/same.mdc"), "same");
        write(&dest.join("rules/same.mdc"), "same");
        let r = merge_pulled_policy(&clone, &clone, &dest, false, PullLimits::default()).unwrap();
        assert_eq!((r.copied_rules, r.unchanged), (0, 1));
        assert!(r.kept_local.is_empty());
    }

    #[test]
    fn a_changed_local_skill_is_kept_without_force() {
        let (_tmp, clone, dest) = fixture();
        write(&clone.join("skills/review-loop/SKILL.md"), "remote");
        write(&dest.join("skills/review-loop/SKILL.md"), "local");
        write(&dest.join("skills/review-loop/notes.md"), "mine");
        let r = merge_pulled_policy(&clone, &clone, &dest, false, PullLimits::default()).unwrap();
        assert_eq!(r.copied_skills, 0);
        assert_eq!(r.kept_local, vec!["skills/review-loop".to_string()]);
        assert_eq!(
            fs::read_to_string(dest.join("skills/review-loop/SKILL.md")).unwrap(),
            "local"
        );
        assert!(dest.join("skills/review-loop/notes.md").exists());
    }

    #[test]
    fn force_replaces_a_changed_local_skill_whole() {
        let (_tmp, clone, dest) = fixture();
        write(&clone.join("skills/review-loop/SKILL.md"), "remote");
        write(&dest.join("skills/review-loop/SKILL.md"), "local");
        write(&dest.join("skills/review-loop/notes.md"), "mine");
        let r = merge_pulled_policy(&clone, &clone, &dest, true, PullLimits::default()).unwrap();
        assert_eq!(r.copied_skills, 1);
        assert_eq!(
            fs::read_to_string(dest.join("skills/review-loop/SKILL.md")).unwrap(),
            "remote"
        );
        assert!(!dest.join("skills/review-loop/notes.md").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_rule_is_not_followed() {
        let (tmp, clone, dest) = fixture();
        let secret = tmp.path().join("secret.txt");
        fs::write(&secret, "PRIVATE KEY").unwrap();
        std::os::unix::fs::symlink(&secret, clone.join("rules/team.mdc")).unwrap();
        let r = merge_pulled_policy(&clone, &clone, &dest, false, PullLimits::default()).unwrap();
        assert_eq!(r.copied_rules, 0);
        assert_eq!(r.skipped_symlinks, vec!["rules/team.mdc".to_string()]);
        assert!(!dest.join("rules/team.mdc").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_in_skills_are_not_followed() {
        let (tmp, clone, dest) = fixture();
        let outside = tmp.path().join("outside");
        write(&outside.join("SKILL.md"), "outside");
        std::os::unix::fs::symlink(&outside, clone.join("skills/x")).unwrap();
        write(&clone.join("skills/ok/SKILL.md"), "ok");
        std::os::unix::fs::symlink(tmp.path(), clone.join("skills/ok/loop")).unwrap();
        let r = merge_pulled_policy(&clone, &clone, &dest, false, PullLimits::default()).unwrap();
        assert_eq!(r.copied_skills, 1);
        assert!(!dest.join("skills/x").exists());
        assert!(!dest.join("skills/ok/loop").exists());
        assert_eq!(
            r.skipped_symlinks,
            vec!["skills/ok/loop".to_string(), "skills/x".to_string()]
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_rules_folder_that_links_out_of_the_clone_is_refused() {
        let (tmp, clone, dest) = fixture();
        fs::remove_dir(clone.join("rules")).unwrap();
        let outside = tmp.path().join("outside");
        write(&outside.join("x.mdc"), "outside");
        std::os::unix::fs::symlink(&outside, clone.join("rules")).unwrap();
        let r = merge_pulled_policy(&clone, &clone, &dest, false, PullLimits::default());
        assert!(r.is_err(), "{r:?}");
        assert!(!dest.join("rules/x.mdc").exists());
    }

    #[test]
    fn a_tree_over_the_file_cap_is_refused() {
        let (_tmp, clone, dest) = fixture();
        for i in 0..5 {
            write(&clone.join(format!("skills/big/f{i}.md")), "x");
        }
        let limits = PullLimits {
            max_files: 3,
            ..PullLimits::default()
        };
        let r = merge_pulled_policy(&clone, &clone, &dest, false, limits);
        assert!(r.is_err(), "{r:?}");
        assert!(!dest.join("skills/big").exists());
    }

    #[test]
    fn a_tree_over_the_byte_cap_is_refused() {
        let (_tmp, clone, dest) = fixture();
        write(&clone.join("rules/huge.mdc"), &"x".repeat(100));
        let limits = PullLimits {
            max_bytes: 10,
            ..PullLimits::default()
        };
        assert!(merge_pulled_policy(&clone, &clone, &dest, false, limits).is_err());
        assert!(!dest.join("rules/huge.mdc").exists());
    }
}
