//! `ax init` keeps its local data (ax.db, logs, backups) out of git.

use std::path::Path;
use std::process::{Command, Stdio};

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn b1_b5_init_leaves_only_shareable_files_untracked() {
    let repo = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let p = repo.path();
    git(p, &["init", "-q"]);
    std::fs::write(p.join(".gitignore"), "target/\n").unwrap();
    std::fs::write(p.join("main.rs"), "fn main() {}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ax"))
        .arg("init")
        .current_dir(p)
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ax init: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(p.join(".ax/ax.db").is_file());

    let stray: Vec<String> = git(
        p,
        &[
            "status",
            "--porcelain",
            "--untracked-files=all",
            "--",
            ".ax",
        ],
    )
    .lines()
    .map(|l| l.trim_start_matches("?? ").to_string())
    .filter(|f| f != ".ax/.gitignore" && !f.starts_with(".ax/policy/"))
    .collect();
    assert!(stray.is_empty(), "untracked under .ax/: {stray:?}");
    assert_eq!(
        std::fs::read_to_string(p.join(".gitignore")).unwrap(),
        "target/\n"
    );
}
