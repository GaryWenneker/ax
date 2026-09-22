//! Known language servers ax can spawn for enrichment.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use ax_types::Language;

#[derive(Debug, Clone)]
pub struct ServerSpec {
    pub id: &'static str,
    pub language: Language,
    pub command: &'static str,
    pub args: &'static [&'static str],
    /// File extensions this server handles (without dot).
    pub extensions: &'static [&'static str],
}

pub const SERVERS: &[ServerSpec] = &[
    ServerSpec {
        id: "rust-analyzer",
        language: Language::Rust,
        command: "rust-analyzer",
        args: &[],
        extensions: &["rs"],
    },
    ServerSpec {
        id: "typescript-language-server",
        language: Language::Typescript,
        command: "typescript-language-server",
        args: &["--stdio"],
        extensions: &["ts", "tsx", "js", "jsx", "mts", "cts"],
    },
    ServerSpec {
        id: "pyright",
        language: Language::Python,
        command: "pyright-langserver",
        args: &["--stdio"],
        extensions: &["py", "pyi"],
    },
    ServerSpec {
        id: "gopls",
        language: Language::Go,
        command: "gopls",
        args: &[],
        extensions: &["go"],
    },
];

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    pub id: String,
    pub command: String,
    pub available: bool,
    pub path: Option<String>,
    pub languages: Vec<String>,
}

pub fn discover_servers() -> Vec<ServerStatus> {
    discover_servers_in(None)
}

pub fn discover_servers_in(project_root: Option<&Path>) -> Vec<ServerStatus> {
    let extra = extra_bin_dirs(project_root);
    SERVERS
        .iter()
        .map(|s| {
            let (display, working) = resolve_command(s.command, &extra);
            ServerStatus {
                id: s.id.into(),
                command: s.command.into(),
                available: working.is_some(),
                path: working
                    .or(display)
                    .map(|p| p.display().to_string()),
                languages: vec![format!("{:?}", s.language).to_ascii_lowercase()],
            }
        })
        .collect()
}

pub(crate) fn extra_bin_dirs(project_root: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(root) = project_root {
        dirs.push(root.join("node_modules/.bin"));
        dirs.push(root.join("crates/ax-web/web-ui/node_modules/.bin"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("node_modules/.bin"));
        dirs.push(cwd.join("crates/ax-web/web-ui/node_modules/.bin"));
    }
    dirs
}

/// Absolute path of a runnable server, if any.
pub fn working_binary(command: &str, project_root: Option<&Path>) -> Option<PathBuf> {
    resolve_command(command, &extra_bin_dirs(project_root)).1
}

fn rustup_which(command: &str) -> Option<PathBuf> {
    if command != "rust-analyzer" {
        return None;
    }
    let output = Command::new("rustup")
        .args(["which", command])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    let p = PathBuf::from(path);
    p.is_file().then_some(p)
}

fn join_command(dir: &Path, command: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let base = dir.join(command);
    if base.is_file() {
        out.push(base);
    }
    #[cfg(windows)]
    {
        for ext in ["cmd", "exe", "bat"] {
            let p = dir.join(format!("{command}.{ext}"));
            if p.is_file() {
                out.push(p);
            }
        }
    }
    out
}

fn collect_candidates(command: &str, extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut push = |p: PathBuf| {
        if seen.insert(p.clone()) {
            out.push(p);
        }
    };
    if let Some(p) = rustup_which(command) {
        push(p);
    }
    for dir in extra {
        for p in join_command(dir, command) {
            push(p);
        }
    }
    if let Ok(iter) = which::which_all(command) {
        for p in iter {
            push(p);
        }
    }
    out
}

/// First PATH/display candidate, and first candidate that actually runs.
fn resolve_command(command: &str, extra: &[PathBuf]) -> (Option<PathBuf>, Option<PathBuf>) {
    let candidates = collect_candidates(command, extra);
    if candidates.is_empty() {
        return (None, None);
    }
    let working = candidates.iter().find(|p| server_binary_works(p)).cloned();
    let display = working.clone().or_else(|| candidates.into_iter().next());
    (display, working)
}

fn probe_args(path: &Path, args: &[&str]) -> Option<std::process::Output> {
    Command::new(path)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()
}

pub fn looks_like_missing_shim(output: &std::process::Output) -> bool {
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
    .to_ascii_lowercase();
    combined.contains("unknown binary") || combined.contains("is not installed")
}

/// True when the binary is runnable for enrich.
///
/// Prefer `--version` success, then a `version` subcommand (gopls). Some servers
/// (notably `pyright-langserver`) reject version probes and exit non-zero while
/// still being a real install — those count as available unless the process looks
/// like a rustup shim without the component.
pub fn server_binary_works(path: &Path) -> bool {
    // `--version` (rust-analyzer, typescript-language-server, …)
    if let Some(output) = probe_args(path, &["--version"]) {
        if output.status.success() {
            return true;
        }
        if looks_like_missing_shim(&output) {
            return false;
        }
        // Binary started but rejected `--version` (pyright-langserver, gopls, …)
        // Try `version` subcommand before accepting.
        if let Some(ver) = probe_args(path, &["version"]) {
            if ver.status.success() {
                return true;
            }
            if looks_like_missing_shim(&ver) {
                return false;
            }
        }
        return true;
    }
    // Spawn failed for `--version` — last chance: `version` (unlikely)
    if let Some(ver) = probe_args(path, &["version"]) {
        if ver.status.success() {
            return true;
        }
        return !looks_like_missing_shim(&ver);
    }
    false
}

/// True when this server's command is on PATH and actually runnable.
pub fn server_available(spec: &ServerSpec) -> bool {
    server_available_in(spec, None)
}

pub fn server_available_in(spec: &ServerSpec, project_root: Option<&Path>) -> bool {
    working_binary(spec.command, project_root).is_some()
}

pub fn spec_for_extension(ext: &str) -> Option<&'static ServerSpec> {
    let e = ext.trim_start_matches('.').to_ascii_lowercase();
    SERVERS.iter().find(|s| s.extensions.iter().any(|x| *x == e))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    fn write_exec(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
        let mut p = fs::metadata(path).unwrap().permissions();
        p.set_mode(0o755);
        fs::set_permissions(path, p).unwrap();
    }

    #[test]
    fn l1_rustup_shim_is_not_available() {
        let dir = tempfile::tempdir().unwrap();
        let shim = dir.path().join("rust-analyzer");
        write_exec(
            &shim,
            "#!/bin/sh\necho \"error: unknown binary 'rust-analyzer' in toolchain\" >&2\nexit 1\n",
        );
        assert!(!server_binary_works(&shim));
    }

    #[test]
    fn l2_working_binary_wins_over_shim() {
        let dir = tempfile::tempdir().unwrap();
        let named_shim = dir.path().join("pyright-langserver");
        write_exec(
            &named_shim,
            "#!/bin/sh\necho \"error: unknown binary 'pyright-langserver'\" >&2\nexit 1\n",
        );
        let extra = vec![dir.path().to_path_buf()];
        let (_d, w) = resolve_command("pyright-langserver", &extra);
        assert!(w.is_none());
        write_exec(
            &named_shim,
            "#!/bin/sh\necho pyright-langserver\nexit 0\n",
        );
        let (_d, w) = resolve_command("pyright-langserver", &extra);
        assert!(w.is_some());
    }

    #[test]
    fn l3_node_modules_bin_is_searched() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("node_modules/.bin");
        fs::create_dir_all(&bin).unwrap();
        let tls = bin.join("typescript-language-server");
        write_exec(&tls, "#!/bin/sh\necho typescript-language-server 4.0.0\nexit 0\n");
        let extra = extra_bin_dirs(Some(root.path()));
        let (_, working) = resolve_command("typescript-language-server", &extra);
        assert_eq!(working.as_ref(), Some(&tls));
    }

    #[test]
    fn l1_shim_text_is_case_insensitive() {
        let output = std::process::Command::new("sh")
            .args(["-c", "echo \"error: unknown binary 'rust-analyzer'\" >&2; exit 1"])
            .output()
            .unwrap();
        assert!(looks_like_missing_shim(&output));
    }
}
