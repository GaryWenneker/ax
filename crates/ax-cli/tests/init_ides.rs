//! `ax init` IDE choice (A2–A9 from docs/specs/init-ide-selection.md, F1–F2 from
//! docs/specs/ide-detection-and-removal.md), run without a terminal
//! in a temp project with a temp HOME so no real IDE config is touched.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Fx {
    _dir: tempfile::TempDir,
    home: PathBuf,
    project: PathBuf,
}

fn fixture() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let project = dir.path().join("proj");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("main.rs"), "fn main() {}\n").unwrap();
    Fx { home, project, _dir: dir }
}

fn init(fx: &Fx, answer: Option<&str>) -> String {
    init_with_path(fx, answer, None)
}

/// `path` replaces PATH so only the fake CLIs in it count as installed.
fn init_with_path(fx: &Fx, answer: Option<&str>, path: Option<&Path>) -> String {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ax"));
    if let Some(bin) = path {
        cmd.env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
    }
    cmd.arg("init")
        .current_dir(&fx.project)
        .env("HOME", &fx.home)
        .env("AX_HOME_DIR", &fx.home)
        .env("USERPROFILE", &fx.home)
        .env("XDG_CONFIG_HOME", fx.home.join(".config"))
        .env("AX_GLOBAL_DB", fx.home.join("global.db"))
        .env("AX_TELEMETRY", "0")
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("AX_NO_WATCHDOG", "1")
        .env("AX_NO_IDE_PANEL", "1")
        .env_remove("AX_INIT_IDES")
        .stdin(Stdio::null());
    if let Some(a) = answer {
        cmd.env("AX_INIT_IDES", a);
    }
    let out = cmd.output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "ax init failed:\n{text}");
    text
}

fn saved_ides(project: &Path) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(project.join("ax.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let list = v.get("agents")?.get("ides")?.as_array()?.clone();
    Some(list.iter().map(|x| x.as_str().unwrap().to_string()).collect())
}

fn cursor_mcp(fx: &Fx) -> PathBuf {
    fx.home.join(".cursor").join("mcp.json")
}

fn cursor_has_ax(fx: &Fx) -> bool {
    std::fs::read_to_string(cursor_mcp(fx)).is_ok_and(|t| t.contains("\"ax\""))
}

#[cfg(unix)]
fn fake_cli(fx: &Fx, name: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = fx.home.join("fakebin");
    std::fs::create_dir_all(&bin).unwrap();
    let exe = bin.join(name);
    std::fs::write(&exe, "#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

fn codex_has_ax(fx: &Fx) -> bool {
    std::fs::read_to_string(fx.home.join(".codex").join("config.toml")).is_ok_and(|t| t.contains("ax"))
}

#[cfg(unix)]
#[test]
fn a7_f2_without_a_terminal_connects_installed_ides_only() {
    let fx = fixture();
    let bin = fake_cli(&fx, "codex");
    let continue_dir = fx.home.join(".continue");
    std::fs::create_dir_all(&continue_dir).unwrap();
    let out = init_with_path(&fx, None, Some(&bin));
    assert!(codex_has_ax(&fx), "installed Codex CLI was not connected:\n{out}");
    assert_eq!(std::fs::read_dir(&continue_dir).unwrap().count(), 0, "a config folder alone counted as installed:\n{out}");
    assert_eq!(saved_ides(&fx.project), None);
}

#[test]
fn a2_a5_a6_an_answer_is_saved_reused_and_dropped_ides_are_removed() {
    let fx = fixture();
    let out = init(&fx, Some("cursor"));
    assert_eq!(saved_ides(&fx.project), Some(vec!["cursor".to_string()]));
    assert!(cursor_has_ax(&fx), "{out}");

    init(&fx, None);
    assert!(cursor_has_ax(&fx), "A5: a rerun without an answer kept the saved IDE");
    assert_eq!(saved_ides(&fx.project), Some(vec!["cursor".to_string()]));

    let out = init(&fx, Some("none"));
    assert_eq!(saved_ides(&fx.project), Some(Vec::new()));
    assert!(!cursor_has_ax(&fx), "A6: deselected Cursor still has ax:\n{out}");
    assert!(out.contains("Removed ax from Cursor"), "{out}");
    let removed = out.lines().find(|l| l.contains("Removed ax from Cursor")).unwrap();
    let hooks = removed.matches("hooks.json").count();
    assert!(hooks <= 1, "a removed file is listed twice: {removed}");
    assert!(out.contains("No IDEs chosen"), "{out}");
}

#[test]
fn a3_an_empty_saved_list_connects_nothing_even_when_ides_are_found() {
    let fx = fixture();
    std::fs::create_dir_all(fx.home.join(".cursor")).unwrap();
    init(&fx, Some("none"));
    let out = init(&fx, None);
    assert!(!cursor_has_ax(&fx), "{out}");
    assert!(out.contains("No IDEs chosen"), "{out}");
}

#[test]
fn f1_configured_ides_are_removed_even_when_never_saved() {
    let fx = fixture();
    init(&fx, Some("cursor codex"));
    assert!(codex_has_ax(&fx));
    std::fs::remove_file(fx.project.join("ax.json")).unwrap();
    let out = init(&fx, Some("cursor"));
    assert!(!codex_has_ax(&fx), "configured Codex was not removed:\n{out}");
    assert!(out.contains("Removed ax from Codex CLI"), "{out}");
    assert!(cursor_has_ax(&fx), "{out}");
    assert!(!out.contains("Removed ax from Cursor"), "{out}");
    assert!(!out.contains("Removed ax from Zed"), "an IDE without ax config was touched:\n{out}");
}

#[test]
fn d1_disconnecting_claude_cleans_the_project_mcp_json() {
    let fx = fixture();
    let local = fx.project.join(".mcp.json");
    std::fs::write(&local, r#"{"mcpServers":{"other":{"command":"x"}}}"#).unwrap();
    init(&fx, Some("cursor claude"));
    let text = std::fs::read_to_string(&local).unwrap();
    assert!(text.contains("\"ax\""), "Claude connect did not write .mcp.json: {text}");
    let out = init(&fx, Some("cursor"));
    let text = std::fs::read_to_string(&local).unwrap();
    assert!(!text.contains("\"ax\""), "ax still in the project .mcp.json:\n{text}\n{out}");
    assert!(text.contains("\"other\""), "another server was removed: {text}");
    let removed = out.lines().find(|l| l.contains("Removed ax from Claude Code")).unwrap_or_default();
    assert!(removed.contains(".mcp.json"), "{out}");
}

#[test]
fn a9_unknown_saved_ids_are_ignored_with_a_warning() {
    let fx = fixture();
    std::fs::write(fx.project.join("ax.json"), r#"{"agents":{"ides":["cursor","notepad"]}}"#).unwrap();
    let out = init(&fx, None);
    assert!(out.contains("Ignoring unknown saved IDE(s): notepad"), "{out}");
    assert!(cursor_has_ax(&fx), "{out}");
}

#[test]
fn a8_an_unknown_answer_fails_and_names_the_valid_ids() {
    let fx = fixture();
    let out = Command::new(env!("CARGO_BIN_EXE_ax"))
        .arg("init")
        .current_dir(&fx.project)
        .env("HOME", &fx.home)
        .env("AX_HOME_DIR", &fx.home)
        .env("AX_GLOBAL_DB", fx.home.join("global.db"))
        .env("AX_TELEMETRY", "0")
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("AX_NO_IDE_PANEL", "1")
        .env("AX_INIT_IDES", "notepad")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!out.status.success(), "{text}");
    assert!(text.contains("unknown IDE 'notepad'") && text.contains("vscode"), "{text}");
}
