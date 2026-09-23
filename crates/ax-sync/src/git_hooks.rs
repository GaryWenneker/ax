//! Git hook installer for ax sync, ship evaluate, memory capture, and policy pack sync.

use std::fs;
use std::path::Path;

use ax_utils::errors::{AxError, FileError};

const SYNC_LINE: &str = "ax sync --quiet";
const SHIP_LINE: &str = "ax ship --evaluate --quiet";
/// Written by ax before v5.0.3; replaced in place so the gate never runs twice.
const LEGACY_SHIP_LINE: &str = "ax ship --evaluate";
const CAPTURE_COMMIT_LINE: &str = "ax capture-git --limit 1 --quiet";
const CAPTURE_MERGE_LINE: &str = "ax capture-git --limit 20 --quiet";
const MEMORY_EXPORT_LINE: &str = "ax memory export --quiet";
const MEMORY_IMPORT_LINE: &str = "ax memory import --quiet";
const POLICY_PACK_EXPORT_LINE: &str = "ax policy pack export --quiet";
const POLICY_PACK_IMPORT_LINE: &str = "ax policy pack import --quiet";
const SHEBANG: &str = "#!/bin/sh";
const AX_MARKERS: [&str; 7] = [
    "ax sync",
    "ax ship",
    "ax capture-git",
    "ax memory export",
    "ax memory import",
    "ax policy pack export",
    "ax policy pack import",
];

fn hook_lines(name: &str, memory_sync: bool, policy_sync: bool) -> Vec<&'static str> {
    let mut lines: Vec<&'static str> = match name {
        "post-commit" => vec![SYNC_LINE, SHIP_LINE, CAPTURE_COMMIT_LINE],
        "post-merge" => vec![SYNC_LINE, SHIP_LINE, CAPTURE_MERGE_LINE],
        "post-checkout" => vec![SYNC_LINE, SHIP_LINE],
        _ => vec![SYNC_LINE, SHIP_LINE],
    };
    if memory_sync {
        match name {
            "post-commit" => lines.push(MEMORY_EXPORT_LINE),
            "post-merge" => lines.push(MEMORY_IMPORT_LINE),
            _ => {}
        }
    }
    if policy_sync {
        match name {
            "post-commit" => lines.push(POLICY_PACK_EXPORT_LINE),
            "post-merge" => lines.push(POLICY_PACK_IMPORT_LINE),
            _ => {}
        }
    }
    lines
}

fn merge_hook_content(existing: &str, required: &[&str]) -> String {
    let mut lines: Vec<String> = existing.lines().map(String::from).collect();
    for line in required {
        if !existing.contains(line) {
            lines.push((*line).into());
        }
    }
    lines.join("\n") + "\n"
}

/// Git skips a hook without a shebang or execute bit on macOS/Linux; Git for Windows runs it through `sh` regardless.
fn with_shebang(content: &str) -> String {
    if content.starts_with("#!") {
        content.to_string()
    } else {
        format!("{SHEBANG}\n{content}")
    }
}

fn upgrade_legacy_lines(content: &str) -> String {
    if !content.lines().any(|l| l.trim() == LEGACY_SHIP_LINE) {
        return content.to_string();
    }
    let mut out: String = content
        .lines()
        .map(|l| {
            if l.trim() == LEGACY_SHIP_LINE {
                SHIP_LINE
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if content.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn file_error(e: std::io::Error, path: &Path) -> AxError {
    AxError::File(FileError::with_path(
        e.to_string(),
        path.display().to_string(),
    ))
}

#[cfg(unix)]
fn ensure_executable(path: &Path) -> Result<(), AxError> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|e| file_error(e, path))?
        .permissions();
    let mode = perms.mode();
    if mode & 0o755 != 0o755 {
        perms.set_mode(mode | 0o755);
        fs::set_permissions(path, perms).map_err(|e| file_error(e, path))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_executable(_path: &Path) -> Result<(), AxError> {
    Ok(())
}

fn write_hook(path: &Path, existing: Option<&str>, content: &str) -> Result<(), AxError> {
    if existing != Some(content) {
        fs::write(path, content).map_err(|e| file_error(e, path))?;
    }
    ensure_executable(path)
}

pub fn install_git_sync_hooks(project_root: &Path) -> Result<(), AxError> {
    let hooks_dir = project_root.join(".git").join("hooks");
    if !hooks_dir.exists() {
        return Ok(());
    }
    let memory_sync = root_bool_flag(project_root, "memorySync");
    let policy_sync = root_bool_flag(project_root, "policySync");
    for name in ["post-commit", "post-merge", "post-checkout"] {
        let hook_path = hooks_dir.join(name);
        let required = hook_lines(name, memory_sync, policy_sync);
        let existing = if hook_path.exists() {
            Some(fs::read_to_string(&hook_path).unwrap_or_default())
        } else {
            None
        };
        let upgraded = existing.as_deref().map(upgrade_legacy_lines);
        let content = match upgraded.as_deref() {
            Some(e) if required.iter().all(|line| e.contains(line)) => with_shebang(e),
            Some(e) => with_shebang(&merge_hook_content(e, &required)),
            None => with_shebang(&(required.join("\n") + "\n")),
        };
        write_hook(&hook_path, existing.as_deref(), &content)?;
    }
    Ok(())
}

/// Give hooks that ax wrote a shebang and execute bit. Hooks without an ax line are left alone.
pub fn repair_git_hooks(project_root: &Path) -> Result<(), AxError> {
    let hooks_dir = project_root.join(".git").join("hooks");
    if !hooks_dir.exists() {
        return Ok(());
    }
    for name in ["post-commit", "post-merge", "post-checkout"] {
        let hook_path = hooks_dir.join(name);
        let Ok(existing) = fs::read_to_string(&hook_path) else {
            continue;
        };
        if !existing.lines().any(is_ax_line) {
            continue;
        }
        let content = with_shebang(&upgrade_legacy_lines(&existing));
        write_hook(&hook_path, Some(&existing), &content)?;
    }
    Ok(())
}

fn is_ax_line(line: &str) -> bool {
    AX_MARKERS.iter().any(|m| line.contains(m))
}

fn root_bool_flag(project_root: &Path, key: &str) -> bool {
    for name in ["ax.json", ".ax.json"] {
        let path = project_root.join(name);
        let Ok(content) = fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if v.get(key).and_then(|x| x.as_bool()) == Some(true) {
                return true;
            }
        }
    }
    false
}

pub fn remove_git_sync_hooks(project_root: &Path) -> Result<(), AxError> {
    let hooks_dir = project_root.join(".git").join("hooks");
    if !hooks_dir.exists() {
        return Ok(());
    }
    for name in ["post-commit", "post-merge", "post-checkout"] {
        let hook_path = hooks_dir.join(name);
        if hook_path.exists() {
            let content = fs::read_to_string(hook_path.display().to_string()).unwrap_or_default();
            let filtered: String = content
                .lines()
                .filter(|l| !is_ax_line(l))
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(hook_path.display().to_string(), filtered).map_err(|e| {
                AxError::File(FileError::with_path(
                    e.to_string(),
                    hook_path.display().to_string(),
                ))
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_commit_includes_capture_git() {
        let lines = hook_lines("post-commit", false, false);
        assert!(lines.contains(&CAPTURE_COMMIT_LINE));
    }

    #[test]
    fn post_checkout_skips_capture_git() {
        let lines = hook_lines("post-checkout", false, false);
        assert!(!lines.iter().any(|l| l.contains("capture-git")));
    }

    #[test]
    fn memory_sync_adds_export_on_commit() {
        let lines = hook_lines("post-commit", true, false);
        assert!(lines.contains(&MEMORY_EXPORT_LINE));
        let merge = hook_lines("post-merge", true, false);
        assert!(merge.contains(&MEMORY_IMPORT_LINE));
    }

    #[test]
    fn policy_sync_adds_pack_on_commit_and_merge() {
        let lines = hook_lines("post-commit", false, true);
        assert!(lines.contains(&POLICY_PACK_EXPORT_LINE));
        let merge = hook_lines("post-merge", false, true);
        assert!(merge.contains(&POLICY_PACK_IMPORT_LINE));
    }

    #[test]
    fn merge_adds_missing_lines() {
        let lines = hook_lines("post-commit", false, false);
        let merged = merge_hook_content("ax sync --quiet\n", &lines);
        assert!(merged.contains(SHIP_LINE));
        assert!(merged.contains(CAPTURE_COMMIT_LINE));
    }

    fn repo_with_hooks() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git").join("hooks")).unwrap();
        dir
    }

    fn hook(dir: &tempfile::TempDir, name: &str) -> std::path::PathBuf {
        dir.path().join(".git").join("hooks").join(name)
    }

    fn write_hook(dir: &tempfile::TempDir, name: &str, content: &str, mode: u32) {
        let path = hook(dir, name);
        fs::write(&path, content).unwrap();
        set_mode(&path, mode);
    }

    #[cfg(unix)]
    fn set_mode(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[cfg(not(unix))]
    fn set_mode(_path: &Path, _mode: u32) {}

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    fn assert_executable(path: &Path) {
        #[cfg(unix)]
        assert_eq!(
            mode(path) & 0o755,
            0o755,
            "{} is not executable",
            path.display()
        );
        #[cfg(not(unix))]
        let _ = path;
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    #[test]
    fn new_hook_gets_shebang_and_exec_bit() {
        let dir = repo_with_hooks();
        install_git_sync_hooks(dir.path()).unwrap();
        for name in ["post-commit", "post-merge", "post-checkout"] {
            let path = hook(&dir, name);
            let content = read(&path);
            assert!(content.starts_with("#!/bin/sh\n"), "{name}: {content:?}");
            assert!(content.contains(SYNC_LINE));
            assert_executable(&path);
        }
    }

    #[test]
    fn broken_ax_hook_is_repaired_on_install() {
        let dir = repo_with_hooks();
        write_hook(&dir, "post-commit", "ax sync --quiet\n", 0o644);
        install_git_sync_hooks(dir.path()).unwrap();
        let path = hook(&dir, "post-commit");
        let content = read(&path);
        assert_eq!(
            content,
            format!("#!/bin/sh\n{SYNC_LINE}\n{SHIP_LINE}\n{CAPTURE_COMMIT_LINE}\n")
        );
        assert_executable(&path);
    }

    #[test]
    fn user_shebang_and_lines_are_kept() {
        let dir = repo_with_hooks();
        write_hook(
            &dir,
            "post-commit",
            "#!/usr/bin/env bash\necho custom\n",
            0o644,
        );
        install_git_sync_hooks(dir.path()).unwrap();
        let path = hook(&dir, "post-commit");
        let content = read(&path);
        assert!(
            content.starts_with("#!/usr/bin/env bash\necho custom\n"),
            "{content:?}"
        );
        assert_eq!(content.matches("#!").count(), 1, "{content:?}");
        assert!(content.contains(CAPTURE_COMMIT_LINE));
        assert_executable(&path);
    }

    #[test]
    fn complete_hook_is_left_untouched() {
        let dir = repo_with_hooks();
        let complete = format!("#!/bin/sh\n{SYNC_LINE}\n{SHIP_LINE}\n{CAPTURE_COMMIT_LINE}\n");
        write_hook(&dir, "post-commit", &complete, 0o755);
        let path = hook(&dir, "post-commit");
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        install_git_sync_hooks(dir.path()).unwrap();
        assert_eq!(read(&path), complete);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
        #[cfg(unix)]
        assert_eq!(mode(&path), 0o755);
    }

    #[test]
    fn all_lines_but_no_shebang_is_repaired() {
        let dir = repo_with_hooks();
        write_hook(
            &dir,
            "post-commit",
            &format!("{SYNC_LINE}\n{SHIP_LINE}\n{CAPTURE_COMMIT_LINE}\n"),
            0o644,
        );
        install_git_sync_hooks(dir.path()).unwrap();
        let path = hook(&dir, "post-commit");
        assert_eq!(
            read(&path),
            format!("#!/bin/sh\n{SYNC_LINE}\n{SHIP_LINE}\n{CAPTURE_COMMIT_LINE}\n")
        );
        assert_executable(&path);
    }

    #[test]
    fn repair_fixes_broken_ax_hook() {
        let dir = repo_with_hooks();
        write_hook(
            &dir,
            "post-merge",
            &format!("{SYNC_LINE}\n{SHIP_LINE}\n"),
            0o644,
        );
        repair_git_hooks(dir.path()).unwrap();
        let path = hook(&dir, "post-merge");
        assert_eq!(
            read(&path),
            format!("#!/bin/sh\n{SYNC_LINE}\n{SHIP_LINE}\n")
        );
        assert_executable(&path);
    }

    #[test]
    fn repair_ignores_hooks_without_ax_lines() {
        let dir = repo_with_hooks();
        write_hook(&dir, "post-commit", "echo mine\n", 0o644);
        repair_git_hooks(dir.path()).unwrap();
        let path = hook(&dir, "post-commit");
        assert_eq!(read(&path), "echo mine\n");
        #[cfg(unix)]
        assert_eq!(mode(&path), 0o644);
        assert!(!hook(&dir, "post-merge").exists());
    }

    #[test]
    fn remove_strips_every_ax_line_and_keeps_the_rest() {
        let dir = repo_with_hooks();
        let content = format!(
            "#!/bin/sh\necho mine\n{SYNC_LINE}\n{SHIP_LINE}\n{CAPTURE_COMMIT_LINE}\n{MEMORY_EXPORT_LINE}\n{POLICY_PACK_EXPORT_LINE}\n"
        );
        write_hook(&dir, "post-commit", &content, 0o755);
        remove_git_sync_hooks(dir.path()).unwrap();
        assert_eq!(read(&hook(&dir, "post-commit")), "#!/bin/sh\necho mine");
    }

    #[test]
    fn new_hooks_run_the_gate_quietly() {
        let dir = repo_with_hooks();
        install_git_sync_hooks(dir.path()).unwrap();
        for name in ["post-commit", "post-merge", "post-checkout"] {
            let content = read(&hook(&dir, name));
            assert!(
                content.lines().any(|l| l == "ax ship --evaluate --quiet"),
                "{name}: {content:?}"
            );
            assert!(
                !content.lines().any(|l| l == "ax ship --evaluate"),
                "{name}: {content:?}"
            );
        }
    }

    #[test]
    fn legacy_ship_line_is_replaced_in_place_on_install() {
        let dir = repo_with_hooks();
        write_hook(
            &dir,
            "post-commit",
            "#!/bin/sh\nax sync --quiet\nax ship --evaluate\nax capture-git --limit 1 --quiet\n",
            0o755,
        );
        install_git_sync_hooks(dir.path()).unwrap();
        assert_eq!(
            read(&hook(&dir, "post-commit")),
            "#!/bin/sh\nax sync --quiet\nax ship --evaluate --quiet\nax capture-git --limit 1 --quiet\n"
        );
    }

    #[test]
    fn repair_replaces_legacy_ship_line() {
        let dir = repo_with_hooks();
        write_hook(
            &dir,
            "post-checkout",
            "ax sync --quiet\nax ship --evaluate\n",
            0o644,
        );
        repair_git_hooks(dir.path()).unwrap();
        let path = hook(&dir, "post-checkout");
        assert_eq!(
            read(&path),
            "#!/bin/sh\nax sync --quiet\nax ship --evaluate --quiet\n"
        );
        assert_executable(&path);
    }

    #[test]
    fn user_ship_variant_is_left_alone() {
        let dir = repo_with_hooks();
        write_hook(
            &dir,
            "post-checkout",
            "#!/bin/sh\nax ship --evaluate --open\n",
            0o755,
        );
        repair_git_hooks(dir.path()).unwrap();
        assert_eq!(
            read(&hook(&dir, "post-checkout")),
            "#!/bin/sh\nax ship --evaluate --open\n"
        );
    }

    #[test]
    fn user_ship_variant_next_to_legacy_line_is_left_alone() {
        let dir = repo_with_hooks();
        write_hook(
            &dir,
            "post-commit",
            "#!/bin/sh\nax ship --evaluate\nax ship --evaluate --open\n",
            0o755,
        );
        repair_git_hooks(dir.path()).unwrap();
        assert_eq!(
            read(&hook(&dir, "post-commit")),
            "#!/bin/sh\nax ship --evaluate --quiet\nax ship --evaluate --open\n"
        );
    }

    #[test]
    fn missing_hooks_dir_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        install_git_sync_hooks(dir.path()).unwrap();
        repair_git_hooks(dir.path()).unwrap();
        assert!(!dir.path().join(".git").exists());
    }
}
