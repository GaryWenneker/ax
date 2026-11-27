//! `ax policy test` and `ax policy storage` against a freshly initialized
//! project. `ax policy test` must pass there, not only
//! inside the ax repository. Runs with a temp HOME so no real config is touched.

use std::path::Path;
use std::process::{Command, Output, Stdio};

fn ax(project: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ax"))
        .args(args)
        .current_dir(project)
        .env("HOME", home)
        .env("AX_HOME_DIR", home)
        .env("USERPROFILE", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("AX_GLOBAL_DB", home.join("global.db"))
        .env("AX_TELEMETRY", "0")
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("AX_NO_WATCHDOG", "1")
        .env("AX_NO_IDE_PANEL", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn fresh_project(ides: &str) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("main.rs"), "fn main() {}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ax"))
        .arg("init")
        .current_dir(&project)
        .env("HOME", &home)
        .env("AX_HOME_DIR", &home)
        .env("USERPROFILE", &home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("AX_GLOBAL_DB", home.join("global.db"))
        .env("AX_TELEMETRY", "0")
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("AX_NO_WATCHDOG", "1")
        .env("AX_NO_IDE_PANEL", "1")
        .env("AX_INIT_IDES", ides)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "ax init failed:\n{}", text(&out));
    (dir, home, project)
}

#[test]
fn policy_test_passes_on_a_fresh_project_without_ides() {
    let (_dir, home, project) = fresh_project("none");
    let out = ax(&project, &home, &["policy", "test", "--json"]);
    assert!(
        out.status.success(),
        "ax policy test failed:\n{}",
        text(&out)
    );
    let results: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let names: Vec<&str> = results
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"ensure_policy_ready"), "{names:?}");
    assert!(names.contains(&"guard_utf8_bom_blocked"), "{names:?}");
    assert!(!names.contains(&"match_release_trigger"), "{names:?}");
}

#[test]
fn policy_test_passes_on_a_fresh_cursor_project() {
    let (_dir, home, project) = fresh_project("cursor");
    let out = ax(&project, &home, &["policy", "test"]);
    assert!(
        out.status.success(),
        "ax policy test failed:\n{}",
        text(&out)
    );
}

#[test]
fn policy_test_fails_when_the_startup_skill_is_missing() {
    let (_dir, home, project) = fresh_project("none");
    for dir in [".agents/skills/startup", ".ax/policy/skills/startup"] {
        let _ = std::fs::remove_dir_all(project.join(dir));
    }
    let out = ax(&project, &home, &["policy", "index", "--force"]);
    assert!(out.status.success(), "{}", text(&out));
    let out = ax(&project, &home, &["policy", "test"]);
    let all = text(&out);
    assert!(!out.status.success(), "{all}");
    assert!(all.contains("FAIL startup_skill"), "{all}");
}

#[test]
fn hidden_self_flag_adds_the_ax_repository_checks() {
    let (_dir, home, project) = fresh_project("none");
    let out = ax(&project, &home, &["policy", "test", "--json", "--self"]);
    let results: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let names: Vec<&str> = results
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"self_match_release_trigger"), "{names:?}");
    // The ax sources are not in this project, so the repository check fails.
    assert!(!out.status.success(), "{}", text(&out));
}

fn project_storage(project: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(project.join("ax.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    v["policy"]["storage"].as_str().map(str::to_string)
}

#[test]
fn storage_switch_keeps_the_config_when_the_migration_fails() {
    let (_dir, home, project) = fresh_project("none");
    let before = project_storage(&project);
    // An ax.db the migration cannot open.
    let db = project.join(".ax/ax.db");
    let _ = std::fs::remove_file(&db);
    for ext in ["ax.db-wal", "ax.db-shm"] {
        let _ = std::fs::remove_file(project.join(".ax").join(ext));
    }
    std::fs::create_dir_all(db.join("not-a-database")).unwrap();

    let out = ax(&project, &home, &["policy", "storage", "database", "--yes"]);
    assert!(!out.status.success(), "{}", text(&out));
    assert_eq!(project_storage(&project), before, "{}", text(&out));
    assert!(
        project
            .join(".agents/rules")
            .read_dir()
            .unwrap()
            .next()
            .is_some(),
        "rule files must stay when the switch failed"
    );
}

#[test]
fn storage_switch_json_reports_what_was_migrated() {
    let (_dir, home, project) = fresh_project("none");
    let out = ax(
        &project,
        &home,
        &["policy", "storage", "database", "--yes", "--json"],
    );
    assert!(out.status.success(), "{}", text(&out));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(v["rulesFound"].as_u64().unwrap() > 0, "{v}");
    assert!(v["skillsFound"].as_u64().unwrap() > 0, "{v}");
    assert_eq!(project_storage(&project).as_deref(), Some("database"));
}
