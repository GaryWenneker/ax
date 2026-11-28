//! `ax unlock` clears a stale lock and stops only this project's lock holder.

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Project {
    _dir: tempfile::TempDir,
    home: tempfile::TempDir,
    root: PathBuf,
}

fn ax(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ax"));
    cmd.env("HOME", home)
        .env("AX_HOME_DIR", home)
        .env("USERPROFILE", home)
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null());
    cmd
}

fn project() -> Project {
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    let out = ax(home.path())
        .arg("init")
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ax init: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Project {
        _dir: dir,
        home,
        root,
    }
}

fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|s| s.success())
}

/// A process whose executable is called `ax` but that has nothing to do with the project.
fn spawn_fake_ax(dir: &Path) -> std::process::Child {
    let fake = dir.join("ax");
    std::fs::copy("/bin/sleep", &fake).unwrap();
    let child = Command::new(&fake).arg("30").spawn().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(200));
    child
}

#[test]
fn unlock_clears_a_stale_lock_without_process_listing_tools() {
    let p = project();
    let lock = p.root.join(".ax/ax.lock");
    std::fs::write(&lock, "999999999\n").unwrap();
    let empty_path = tempfile::tempdir().unwrap();
    let out = ax(p.home.path())
        .arg("unlock")
        .current_dir(&p.root)
        .env("PATH", empty_path.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ax unlock must not need pgrep/kill: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read_to_string(&lock).unwrap(), "", "lock cleared");
}

#[test]
fn unlock_leaves_ax_processes_of_other_projects_running() {
    let p = project();
    let bin = tempfile::tempdir().unwrap();
    let mut other = spawn_fake_ax(bin.path());
    let pid = other.id();
    let out = ax(p.home.path())
        .arg("unlock")
        .current_dir(&p.root)
        .output()
        .unwrap();
    let survived = alive(pid);
    let _ = other.kill();
    let _ = other.wait();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(survived, "ax unlock must not stop an unrelated ax process");
}

#[test]
fn unlock_does_not_stop_a_non_ax_pid_named_in_a_held_lock() {
    let p = project();
    let lock = p.root.join(".ax/ax.lock");
    let mut sleeper = Command::new("sleep").arg("30").spawn().unwrap();
    let mut held = ax_utils::FileLock::new(&p.root.join(".ax"));
    held.acquire_wait(std::time::Duration::from_secs(5))
        .unwrap();
    std::fs::write(&lock, format!("{}\n", sleeper.id())).unwrap();
    let out = ax(p.home.path())
        .arg("unlock")
        .current_dir(&p.root)
        .output()
        .unwrap();
    let survived = alive(sleeper.id());
    let _ = sleeper.kill();
    let _ = sleeper.wait();
    assert!(survived, "a pid that is not ax must never be signalled");
    assert!(!out.status.success(), "a held lock is reported");
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("still held"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn unlock_stops_the_ax_process_named_in_a_held_lock() {
    let p = project();
    let lock = p.root.join(".ax/ax.lock");
    let bin = tempfile::tempdir().unwrap();
    let mut holder = spawn_fake_ax(bin.path());
    let mut held = ax_utils::FileLock::new(&p.root.join(".ax"));
    held.acquire_wait(std::time::Duration::from_secs(5))
        .unwrap();
    std::fs::write(&lock, format!("{}\n", holder.id())).unwrap();
    let out = ax(p.home.path())
        .arg("unlock")
        .current_dir(&p.root)
        .output()
        .unwrap();
    let status = holder.try_wait().unwrap();
    let _ = holder.kill();
    let _ = holder.wait();
    assert!(
        status.is_some(),
        "the ax lock holder must be stopped: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}
