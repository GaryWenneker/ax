//! Commands and hooks started in a subdirectory of an initialized project use that project,
//! the way the MCP server does.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

struct Project {
    _dir: TempDir,
    home: PathBuf,
    root: PathBuf,
    nested: PathBuf,
}

fn ax(dir: &Path, home: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ax"));
    cmd.args(args)
        .current_dir(dir)
        .env("HOME", home)
        .env("AX_HOME_DIR", home)
        .env("USERPROFILE", home)
        .env("AX_USAGE_DB", home.join("usage.db"))
        .env("AX_GLOBAL_DB", home.join("global.db"))
        .env("AX_NO_UPDATE_CHECK", "1")
        .env("AX_OFFLOAD_DISABLE", "1")
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
    let mut child = cmd.spawn().expect("spawn ax");
    if let Some(input) = stdin {
        use std::io::Write;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "exit {:?}\nstdout: {}\nstderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn project() -> Project {
    let dir = tempfile::tempdir().unwrap();
    let base = dir.path().canonicalize().unwrap();
    let home = base.join("home");
    let root = base.join("project");
    let nested = root.join("crates").join("foo");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&nested).unwrap();
    fs::write(
        nested.join("lib.rs"),
        "pub fn ping() -> i32 { pong() }\npub fn pong() -> i32 { 1 }\n",
    )
    .unwrap();
    let root_arg = root.display().to_string();
    let out = ax(&root, &home, &["init", &root_arg], None);
    assert!(out.status.success(), "init: {}", text(&out));
    let out = ax(&root, &home, &["sync", "--quiet"], None);
    assert!(out.status.success(), "sync: {}", text(&out));
    Project {
        _dir: dir,
        home,
        root,
        nested,
    }
}

#[test]
fn query_commands_work_from_a_subdirectory() {
    let p = project();
    for args in [
        &["status", "--json"][..],
        &["callers", "pong"][..],
        &["query", "ping"][..],
    ] {
        let at_root = ax(&p.root, &p.home, args, None);
        assert!(
            at_root.status.success(),
            "{args:?} at root: {}",
            text(&at_root)
        );
        let nested = ax(&p.nested, &p.home, args, None);
        assert!(
            nested.status.success(),
            "{args:?} from a subdirectory: {}",
            text(&nested)
        );
        assert!(
            !String::from_utf8_lossy(&nested.stderr).contains("not initialized"),
            "{args:?}: {}",
            text(&nested)
        );
    }
}

#[test]
fn prompt_hook_injects_the_same_policy_from_a_subdirectory() {
    let p = project();
    let payload = |cwd: &Path| {
        serde_json::json!({
            "prompt": "always add tests when you fix a bug",
            "cwd": cwd.display().to_string(),
        })
        .to_string()
    };
    let at_root = ax(&p.root, &p.home, &["prompt-hook"], Some(&payload(&p.root)));
    assert!(at_root.status.success(), "{}", text(&at_root));
    let root_out = String::from_utf8_lossy(&at_root.stdout).into_owned();
    assert!(
        root_out.contains("<ax_policy"),
        "the root run injects policy: {}",
        text(&at_root)
    );

    let nested = ax(
        &p.nested,
        &p.home,
        &["prompt-hook"],
        Some(&payload(&p.nested)),
    );
    assert!(nested.status.success(), "{}", text(&nested));
    let nested_out = String::from_utf8_lossy(&nested.stdout);
    assert!(
        nested_out.contains("<ax_policy"),
        "a subdirectory run injects policy too: {}",
        text(&nested)
    );
}

#[test]
fn init_in_a_subdirectory_still_initializes_that_directory() {
    let p = project();
    let out = ax(&p.nested, &p.home, &["init"], None);
    assert!(out.status.success(), "{}", text(&out));
    assert!(p.nested.join(".ax").join("ax.db").is_file());
}

#[test]
fn a_directory_without_a_project_above_it_still_reports_not_initialized() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let lone = dir.path().join("lone");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(&lone).unwrap();
    let out = ax(&lone, &home, &["status"], None);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("not initialized"), "{}", text(&out));
}

#[test]
fn the_home_ax_folder_is_never_taken_for_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().canonicalize().unwrap().join("home");
    let work = home.join("work");
    fs::create_dir_all(home.join(".ax")).unwrap();
    fs::create_dir_all(&work).unwrap();
    // `~/.ax` is the global ax folder; a stray database there is not a project.
    fs::write(home.join(".ax").join("ax.db"), b"").unwrap();
    let out = ax(&work, &home, &["status"], None);
    assert!(!out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("not initialized"), "{}", text(&out));
}
