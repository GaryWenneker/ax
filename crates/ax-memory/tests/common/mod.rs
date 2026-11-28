//! Helpers shared by the ax-memory integration tests.

use std::path::Path;
use std::process::Command;

/// The null device git reads as an empty config file.
const NULL_CONFIG: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

/// Runs `git` in `repo`, isolated from the developer's global and system configuration:
/// no signing, no hooks, a fixed identity. `envs` adds variables such as commit dates.
pub fn git(repo: &Path, envs: &[(&str, &str)], args: &[&str]) {
    let no_hooks = repo.join(".git").join("ax-test-no-hooks");
    let output = Command::new("git")
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", NULL_CONFIG)
        .env("GIT_CONFIG_SYSTEM", NULL_CONFIG)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .envs(envs.iter().copied())
        .args(["-c", "commit.gpgsign=false", "-c", "tag.gpgsign=false"])
        .arg("-c")
        .arg(format!("core.hooksPath={}", no_hooks.display()))
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
