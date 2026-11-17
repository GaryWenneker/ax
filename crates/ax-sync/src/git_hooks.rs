//! Git hook installer for ax sync, ship evaluate, memory capture, and policy pack sync.

use std::fs;
use std::path::{Path, PathBuf};

use ax_utils::errors::{AxError, FileError};

const SYNC_REST: &str = "sync --quiet";
const SHIP_REST: &str = "ship --evaluate --quiet";
/// Written by ax before v5.0.3; replaced in place so the gate never runs twice.
const LEGACY_SHIP_REST: &str = "ship --evaluate";
const CAPTURE_COMMIT_REST: &str = "capture-git --limit 1 --quiet";
const CAPTURE_MERGE_REST: &str = "capture-git --limit 20 --quiet";
const MEMORY_EXPORT_REST: &str = "memory export --quiet";
const MEMORY_IMPORT_REST: &str = "memory import --quiet";
const POLICY_PACK_EXPORT_REST: &str = "policy pack export --quiet";
const POLICY_PACK_IMPORT_REST: &str = "policy pack import --quiet";
const SHEBANG: &str = "#!/bin/sh";
const EXE_NAME: &str = if cfg!(windows) { "ax.exe" } else { "ax" };
const MISSING_BIN: &str = "ax binary not found; run ax install or repair hooks from a working ax";
/// Bare `ax …` lines and resolved-binary lines for stripping and repair detection.
const AX_MARKERS: [&str; 7] = [
    "ax sync",
    "ax ship",
    "ax capture-git",
    "ax memory export",
    "ax memory import",
    "ax policy pack export",
    "ax policy pack import",
];
const MANAGED_RESTS: [&str; 9] = [
    SYNC_REST,
    SHIP_REST,
    LEGACY_SHIP_REST,
    CAPTURE_COMMIT_REST,
    CAPTURE_MERGE_REST,
    MEMORY_EXPORT_REST,
    MEMORY_IMPORT_REST,
    POLICY_PACK_EXPORT_REST,
    POLICY_PACK_IMPORT_REST,
];

/// Back-compat names for tests and docs that refer to bare PATH lines.
#[allow(dead_code)]
const SYNC_LINE: &str = "ax sync --quiet";
#[allow(dead_code)]
const SHIP_LINE: &str = "ax ship --evaluate --quiet";
const LEGACY_SHIP_LINE: &str = "ax ship --evaluate";
const CAPTURE_COMMIT_LINE: &str = "ax capture-git --limit 1 --quiet";
#[allow(dead_code)]
const CAPTURE_MERGE_LINE: &str = "ax capture-git --limit 20 --quiet";
#[allow(dead_code)]
const MEMORY_EXPORT_LINE: &str = "ax memory export --quiet";
#[allow(dead_code)]
const MEMORY_IMPORT_LINE: &str = "ax memory import --quiet";
#[allow(dead_code)]
const POLICY_PACK_EXPORT_LINE: &str = "ax policy pack export --quiet";
#[allow(dead_code)]
const POLICY_PACK_IMPORT_LINE: &str = "ax policy pack import --quiet";

fn resolve_ax_binary() -> Result<PathBuf, AxError> {
    if let Ok(exe) = std::env::current_exe() {
        if exe.is_file() {
            return Ok(exe);
        }
    }
    if let Some(path_env) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_env).filter(|d| d.is_absolute()) {
            let candidate = dir.join(EXE_NAME);
            if is_executable(&candidate) {
                return Ok(candidate);
            }
        }
    }
    Err(AxError::Other(MISSING_BIN.into()))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

fn shell_bin(path: &Path) -> String {
    let raw = path.to_string_lossy();
    let normalized = if cfg!(windows) {
        let stripped = if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
            format!("//{unc}")
        } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
            rest.to_string()
        } else {
            raw.into_owned()
        };
        stripped.replace('\\', "/")
    } else {
        raw.into_owned()
    };
    if normalized.contains(char::is_whitespace) {
        format!("\"{normalized}\"")
    } else {
        normalized
    }
}

fn format_ax_line(bin: &str, rest: &str) -> String {
    format!("{bin} {rest}")
}

fn hook_lines(name: &str, memory_sync: bool, policy_sync: bool, bin: &str) -> Vec<String> {
    let line = |rest: &str| format_ax_line(bin, rest);
    let mut lines: Vec<String> = match name {
        "post-commit" => vec![
            line(SYNC_REST),
            line(SHIP_REST),
            line(CAPTURE_COMMIT_REST),
        ],
        "post-merge" => vec![
            line(SYNC_REST),
            line(SHIP_REST),
            line(CAPTURE_MERGE_REST),
        ],
        "post-checkout" => vec![line(SYNC_REST), line(SHIP_REST)],
        _ => vec![line(SYNC_REST), line(SHIP_REST)],
    };
    if memory_sync {
        match name {
            "post-commit" => lines.push(line(MEMORY_EXPORT_REST)),
            "post-merge" => lines.push(line(MEMORY_IMPORT_REST)),
            _ => {}
        }
    }
    if policy_sync {
        match name {
            "post-commit" => lines.push(line(POLICY_PACK_EXPORT_REST)),
            "post-merge" => lines.push(line(POLICY_PACK_IMPORT_REST)),
            _ => {}
        }
    }
    lines
}

fn merge_hook_content(existing: &str, required: &[&str]) -> String {
    let mut lines: Vec<String> = existing.lines().map(String::from).collect();
    for line in required {
        if !existing.contains(line) {
            lines.push(line.to_string());
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

fn line_is_ax_invoke(line: &str, rest: &str) -> bool {
    if line == format!("ax {rest}") {
        return true;
    }
    let suffix = format!(" {rest}");
    if !line.ends_with(&suffix) {
        return false;
    }
    let prefix = line[..line.len() - suffix.len()].trim_end();
    prefix == "ax"
        || prefix.ends_with("/ax")
        || prefix.ends_with("\\ax")
        || prefix.ends_with("/ax.exe")
        || prefix.ends_with("\\ax.exe")
        || (prefix.starts_with('"') && prefix.ends_with('"') && prefix[1..prefix.len() - 1].ends_with("ax"))
}

fn rewrite_ax_binary_paths(content: &str, bin: &str) -> String {
    let out: Vec<String> = content
        .lines()
        .map(|l| {
            let trimmed = l.trim();
            for rest in MANAGED_RESTS {
                if line_is_ax_invoke(trimmed, rest) {
                    let expected = format_ax_line(bin, rest);
                    if trimmed != expected {
                        return expected;
                    }
                }
            }
            l.to_string()
        })
        .collect();
    let mut joined = out.join("\n");
    if content.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

fn upgrade_legacy_lines(content: &str, bin: &str) -> String {
    let legacy_resolved = format_ax_line(bin, LEGACY_SHIP_REST);
    let replacement = format_ax_line(bin, SHIP_REST);
    let bare_legacy = LEGACY_SHIP_LINE;
    if !content
        .lines()
        .any(|l| l.trim() == bare_legacy || l.trim() == legacy_resolved)
    {
        return content.to_string();
    }
    let mut out: String = content
        .lines()
        .map(|l| {
            let t = l.trim();
            if t == bare_legacy || t == legacy_resolved {
                replacement.clone()
            } else {
                l.to_string()
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
    let bin = shell_bin(&resolve_ax_binary()?);
    let memory_sync = root_bool_flag(project_root, "memorySync");
    let policy_sync = root_bool_flag(project_root, "policySync");
    for name in ["post-commit", "post-merge", "post-checkout"] {
        let hook_path = hooks_dir.join(name);
        let required = hook_lines(name, memory_sync, policy_sync, &bin);
        let required_refs: Vec<&str> = required.iter().map(String::as_str).collect();
        let existing = if hook_path.exists() {
            Some(fs::read_to_string(&hook_path).unwrap_or_default())
        } else {
            None
        };
        let upgraded = existing
            .as_deref()
            .map(|e| upgrade_legacy_lines(e, &bin))
            .map(|e| rewrite_ax_binary_paths(&e, &bin));
        let content = match upgraded.as_deref() {
            Some(e) if required_refs.iter().all(|line| e.contains(line)) => with_shebang(e),
            Some(e) => with_shebang(&merge_hook_content(e, &required_refs)),
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
    let bin = shell_bin(&resolve_ax_binary()?);
    for name in ["post-commit", "post-merge", "post-checkout"] {
        let hook_path = hooks_dir.join(name);
        let Ok(existing) = fs::read_to_string(&hook_path) else {
            continue;
        };
        if !existing.lines().any(is_ax_line) {
            continue;
        }
        let body = rewrite_ax_binary_paths(&upgrade_legacy_lines(&existing, &bin), &bin);
        let content = with_shebang(&body);
        write_hook(&hook_path, Some(&existing), &content)?;
    }
    Ok(())
}

fn is_resolved_ax_command(line: &str) -> bool {
    MANAGED_RESTS.iter().any(|rest| {
        line.ends_with(rest) && line.len() > rest.len() + 1 && line.as_bytes()[line.len() - rest.len() - 1] == b' '
    })
}

fn is_ax_line(line: &str) -> bool {
    let trimmed = line.trim();
    AX_MARKERS.iter().any(|m| line.contains(m))
        || MANAGED_RESTS
            .iter()
            .any(|rest| line_is_ax_invoke(trimmed, rest))
        || is_resolved_ax_command(trimmed)
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
            let content = fs::read_to_string(&hook_path).unwrap_or_default();
            let filtered: String = content
                .lines()
                .filter(|l| !is_ax_line(l))
                .collect::<Vec<_>>()
                .join("\n");
            fs::write(&hook_path, filtered).map_err(|e| {
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

    fn hook_bin() -> String {
        shell_bin(&resolve_ax_binary().expect("test binary"))
    }

    fn line(rest: &str) -> String {
        format_ax_line(&hook_bin(), rest)
    }

    #[test]
    fn post_commit_includes_capture_git() {
        let lines = hook_lines("post-commit", false, false, &hook_bin());
        assert!(lines.iter().any(|l| l.contains("capture-git")));
    }

    #[test]
    fn post_checkout_skips_capture_git() {
        let lines = hook_lines("post-checkout", false, false, &hook_bin());
        assert!(!lines.iter().any(|l| l.contains("capture-git")));
    }

    #[test]
    fn memory_sync_adds_export_on_commit() {
        let bin = hook_bin();
        let lines = hook_lines("post-commit", true, false, &bin);
        assert!(lines.contains(&line(MEMORY_EXPORT_REST)));
        let merge = hook_lines("post-merge", true, false, &bin);
        assert!(merge.contains(&line(MEMORY_IMPORT_REST)));
    }

    #[test]
    fn policy_sync_adds_pack_on_commit_and_merge() {
        let bin = hook_bin();
        let lines = hook_lines("post-commit", false, true, &bin);
        assert!(lines.contains(&line(POLICY_PACK_EXPORT_REST)));
        let merge = hook_lines("post-merge", false, true, &bin);
        assert!(merge.contains(&line(POLICY_PACK_IMPORT_REST)));
    }

    #[test]
    fn merge_adds_missing_lines() {
        let bin = hook_bin();
        let lines = hook_lines("post-commit", false, false, &bin);
        let required: Vec<&str> = lines.iter().map(String::as_str).collect();
        let merged = merge_hook_content(&format!("{SYNC_LINE}\n"), &required);
        assert!(merged.contains(&line(SHIP_REST)));
        assert!(merged.contains(&line(CAPTURE_COMMIT_REST)));
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

    fn post_commit_body() -> String {
        format!(
            "{}\n{}\n{}\n",
            line(SYNC_REST),
            line(SHIP_REST),
            line(CAPTURE_COMMIT_REST)
        )
    }

    #[test]
    fn new_hook_gets_shebang_and_exec_bit() {
        let dir = repo_with_hooks();
        install_git_sync_hooks(dir.path()).unwrap();
        let bin = hook_bin();
        for name in ["post-commit", "post-merge", "post-checkout"] {
            let path = hook(&dir, name);
            let content = read(&path);
            assert!(content.starts_with("#!/bin/sh\n"), "{name}: {content:?}");
            assert!(content.contains(&format_ax_line(&bin, SYNC_REST)));
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
        assert_eq!(content, format!("#!/bin/sh\n{}", post_commit_body()));
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
        assert!(content.contains(&line(CAPTURE_COMMIT_REST)));
        assert_executable(&path);
    }

    #[test]
    fn complete_hook_is_left_untouched() {
        let dir = repo_with_hooks();
        let complete = format!("#!/bin/sh\n{}", post_commit_body());
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
        write_hook(&dir, "post-commit", &post_commit_body(), 0o644);
        install_git_sync_hooks(dir.path()).unwrap();
        let path = hook(&dir, "post-commit");
        assert_eq!(
            read(&path),
            format!("#!/bin/sh\n{}", post_commit_body())
        );
        assert_executable(&path);
    }

    #[test]
    fn repair_fixes_broken_ax_hook() {
        let dir = repo_with_hooks();
        write_hook(
            &dir,
            "post-merge",
            &format!("{}\n{}\n", line(SYNC_REST), line(SHIP_REST)),
            0o644,
        );
        repair_git_hooks(dir.path()).unwrap();
        let path = hook(&dir, "post-merge");
        assert_eq!(
            read(&path),
            format!("#!/bin/sh\n{}\n{}\n", line(SYNC_REST), line(SHIP_REST))
        );
        assert_executable(&path);
    }

    #[test]
    fn repair_rewrites_stale_absolute_binary() {
        let dir = repo_with_hooks();
        let stale = "/definitely/missing/ax";
        write_hook(
            &dir,
            "post-commit",
            &format!("{stale} sync --quiet\n"),
            0o644,
        );
        repair_git_hooks(dir.path()).unwrap();
        assert_eq!(
            read(&hook(&dir, "post-commit")),
            format!("#!/bin/sh\n{}\n", line(SYNC_REST))
        );
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
            "#!/bin/sh\necho mine\n{}\n{}\n{}\n{}\n{}\n",
            line(SYNC_REST),
            line(SHIP_REST),
            line(CAPTURE_COMMIT_REST),
            line(MEMORY_EXPORT_REST),
            line(POLICY_PACK_EXPORT_REST)
        );
        write_hook(&dir, "post-commit", &content, 0o755);
        remove_git_sync_hooks(dir.path()).unwrap();
        assert_eq!(read(&hook(&dir, "post-commit")), "#!/bin/sh\necho mine");
    }

    #[test]
    fn new_hooks_run_the_gate_quietly() {
        let dir = repo_with_hooks();
        install_git_sync_hooks(dir.path()).unwrap();
        let quiet = line(SHIP_REST);
        for name in ["post-commit", "post-merge", "post-checkout"] {
            let content = read(&hook(&dir, name));
            assert!(content.lines().any(|l| l == quiet), "{name}: {content:?}");
            assert!(
                !content.lines().any(|l| l == line(LEGACY_SHIP_REST)),
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
            &format!(
                "#!/bin/sh\n{SYNC_LINE}\n{LEGACY_SHIP_LINE}\n{CAPTURE_COMMIT_LINE}\n"
            ),
            0o755,
        );
        install_git_sync_hooks(dir.path()).unwrap();
        assert_eq!(
            read(&hook(&dir, "post-commit")),
            format!("#!/bin/sh\n{}", post_commit_body())
        );
    }

    #[test]
    fn repair_replaces_legacy_ship_line() {
        let dir = repo_with_hooks();
        write_hook(
            &dir,
            "post-checkout",
            &format!("{SYNC_LINE}\n{LEGACY_SHIP_LINE}\n"),
            0o644,
        );
        repair_git_hooks(dir.path()).unwrap();
        let path = hook(&dir, "post-checkout");
        assert_eq!(
            read(&path),
            format!("#!/bin/sh\n{}\n{}\n", line(SYNC_REST), line(SHIP_REST))
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
            format!(
                "#!/bin/sh\n{}\nax ship --evaluate --open\n",
                line(SHIP_REST)
            )
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