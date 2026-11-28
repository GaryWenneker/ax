//! `ax uninit` asks before deleting `.ax/` and removes the git hooks first.

use std::path::Path;
use std::process::{Command, Output, Stdio};

fn ax(dir: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ax"))
        .args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("AX_HOME_DIR", home)
        .env("USERPROFILE", home)
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path();
    assert!(Command::new("git")
        .args(["init", "-q"])
        .current_dir(p)
        .status()
        .unwrap()
        .success());
    std::fs::create_dir_all(p.join(".ax/policy/rules")).unwrap();
    std::fs::write(p.join(".ax/policy/rules/team.mdc"), "rule\n").unwrap();
    std::fs::create_dir_all(p.join(".ax/memory")).unwrap();
    std::fs::write(p.join(".ax/memory/shared.jsonl"), "{}\n").unwrap();
    std::fs::write(p.join(".ax/ax.db"), "").unwrap();
    dir
}

#[test]
fn without_yes_and_without_a_terminal_nothing_is_deleted() {
    let dir = project();
    let home = tempfile::tempdir().unwrap();
    let out = ax(dir.path(), home.path(), &["uninit"]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "uninit must refuse without --yes");
    assert!(stderr.contains("--yes"), "{stderr}");
    assert!(stderr.contains("policy"), "lists what would go: {stderr}");
    assert!(dir.path().join(".ax/policy/rules/team.mdc").is_file());
    assert!(dir.path().join(".ax/memory/shared.jsonl").is_file());
}

#[test]
fn with_yes_the_ax_directory_is_removed() {
    let dir = project();
    let home = tempfile::tempdir().unwrap();
    let out = ax(dir.path(), home.path(), &["uninit", "--yes"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!dir.path().join(".ax").exists());
}

#[cfg(unix)]
#[test]
fn a_hook_that_cannot_be_cleaned_keeps_the_ax_directory() {
    use std::os::unix::fs::PermissionsExt;
    let dir = project();
    let home = tempfile::tempdir().unwrap();
    let hook = dir.path().join(".git/hooks/post-commit");
    std::fs::create_dir_all(hook.parent().unwrap()).unwrap();
    std::fs::write(&hook, "#!/bin/sh\nax capture-git # ax\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o444)).unwrap();
    let out = ax(dir.path(), home.path(), &["uninit", "--yes"]);
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(!out.status.success());
    assert!(
        dir.path().join(".ax/ax.db").exists(),
        "a failed hook removal must leave .ax in place"
    );
}
