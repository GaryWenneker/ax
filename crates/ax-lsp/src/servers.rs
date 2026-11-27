//! Known language servers ax can spawn for enrichment.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

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
                path: working.or(display).map(|p| p.display().to_string()),
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

/// Resolves each server once and remembers the answer for the rest of a run.
///
/// Resolving runs `rustup which` and version probes, so an enrich batch must not
/// repeat it per file.
pub(crate) struct ServerResolver {
    extra: Vec<PathBuf>,
    /// Also search `rustup which` and PATH (off only in tests).
    system: bool,
    cache: HashMap<&'static str, Option<PathBuf>>,
}

impl ServerResolver {
    pub(crate) fn for_project(project_root: Option<&Path>) -> Self {
        Self {
            extra: extra_bin_dirs(project_root),
            system: true,
            cache: HashMap::new(),
        }
    }

    /// Search only `dirs`: no rustup, no PATH.
    #[cfg(test)]
    pub(crate) fn only_dirs(dirs: Vec<PathBuf>) -> Self {
        Self {
            extra: dirs,
            system: false,
            cache: HashMap::new(),
        }
    }

    pub(crate) fn binary(&mut self, spec: &'static ServerSpec) -> Option<PathBuf> {
        let (extra, system) = (&self.extra, self.system);
        self.cache
            .entry(spec.id)
            .or_insert_with(|| resolve_in(spec.command, extra, system).1)
            .clone()
    }
}

/// Longest time one version probe (or `rustup which`) may run before it is killed.
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

enum Probe {
    Exited(Output),
    NotRun,
    TimedOut,
}

/// Run `cmd` with a null stdin and kill it after [`PROBE_TIMEOUT`].
fn run_bounded(mut cmd: Command) -> Probe {
    let mut child = match cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return Probe::NotRun,
    };
    let collect = |pipe: Option<Box<dyn Read + Send>>| {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            let _ = tx.send(buf);
        });
        rx
    };
    let out_rx = collect(child.stdout.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let err_rx = collect(child.stderr.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let deadline = Instant::now() + PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                // A grandchild may still hold the pipes open; do not wait on it forever.
                let grace = Duration::from_millis(500);
                return Probe::Exited(Output {
                    status,
                    stdout: out_rx.recv_timeout(grace).unwrap_or_default(),
                    stderr: err_rx.recv_timeout(grace).unwrap_or_default(),
                });
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Probe::TimedOut;
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Probe::NotRun;
            }
        }
    }
}

fn rustup_which(command: &str) -> Option<PathBuf> {
    if command != "rust-analyzer" {
        return None;
    }
    let mut cmd = Command::new("rustup");
    cmd.args(["which", command]);
    let Probe::Exited(output) = run_bounded(cmd) else {
        return None;
    };
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

fn collect_candidates(command: &str, extra: &[PathBuf], system: bool) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut push = |p: PathBuf| {
        if seen.insert(p.clone()) {
            out.push(p);
        }
    };
    if system {
        if let Some(p) = rustup_which(command) {
            push(p);
        }
    }
    for dir in extra {
        for p in join_command(dir, command) {
            push(p);
        }
    }
    if system {
        if let Ok(iter) = which::which_all(command) {
            for p in iter {
                push(p);
            }
        }
    }
    out
}

/// First PATH/display candidate, and first candidate that actually runs.
fn resolve_command(command: &str, extra: &[PathBuf]) -> (Option<PathBuf>, Option<PathBuf>) {
    resolve_in(command, extra, true)
}

fn resolve_in(
    command: &str,
    extra: &[PathBuf],
    system: bool,
) -> (Option<PathBuf>, Option<PathBuf>) {
    let candidates = collect_candidates(command, extra, system);
    if candidates.is_empty() {
        return (None, None);
    }
    let working = candidates.iter().find(|p| server_binary_works(p)).cloned();
    let display = working.clone().or_else(|| candidates.into_iter().next());
    (display, working)
}

fn probe_args(path: &Path, args: &[&str]) -> Probe {
    let mut cmd = Command::new(path);
    cmd.args(args);
    run_bounded(cmd)
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
    match probe_args(path, &["--version"]) {
        // `--version` (rust-analyzer, typescript-language-server, …)
        Probe::Exited(output) => {
            if output.status.success() {
                return true;
            }
            if looks_like_missing_shim(&output) {
                return false;
            }
            // Binary started but rejected `--version` (pyright-langserver, gopls, …)
            // Try `version` subcommand before accepting.
            match probe_args(path, &["version"]) {
                Probe::Exited(ver) => ver.status.success() || !looks_like_missing_shim(&ver),
                Probe::TimedOut => false,
                Probe::NotRun => true,
            }
        }
        // A probe that never answers is not a server ax can use.
        Probe::TimedOut => false,
        // Spawn failed for `--version` — last chance: `version` (unlikely)
        Probe::NotRun => match probe_args(path, &["version"]) {
            Probe::Exited(ver) => ver.status.success() || !looks_like_missing_shim(&ver),
            _ => false,
        },
    }
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
    SERVERS
        .iter()
        .find(|s| s.extensions.iter().any(|x| *x == e))
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
        // Search only the temp dir so a real pyright on this machine's PATH
        // cannot make the shim case pass.
        let (_d, w) = resolve_in("pyright-langserver", &extra, false);
        assert!(w.is_none());
        write_exec(&named_shim, "#!/bin/sh\necho pyright-langserver\nexit 0\n");
        let (_d, w) = resolve_in("pyright-langserver", &extra, false);
        assert!(w.is_some());
    }

    #[test]
    fn l3_node_modules_bin_is_searched() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("node_modules/.bin");
        fs::create_dir_all(&bin).unwrap();
        let tls = bin.join("typescript-language-server");
        write_exec(
            &tls,
            "#!/bin/sh\necho typescript-language-server 4.0.0\nexit 0\n",
        );
        let extra = extra_bin_dirs(Some(root.path()));
        let (_, working) = resolve_command("typescript-language-server", &extra);
        assert_eq!(working.as_ref(), Some(&tls));
    }

    #[test]
    fn l1_shim_text_is_case_insensitive() {
        let output = std::process::Command::new("sh")
            .args([
                "-c",
                "echo \"error: unknown binary 'rust-analyzer'\" >&2; exit 1",
            ])
            .output()
            .unwrap();
        assert!(looks_like_missing_shim(&output));
    }
}

#[cfg(all(test, unix))]
mod probe_timeout_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn a_hanging_probe_is_killed_and_counts_as_not_working() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("gopls");
        std::fs::write(&bin, "#!/bin/sh\nexec sleep 30\n").unwrap();
        let mut p = std::fs::metadata(&bin).unwrap().permissions();
        p.set_mode(0o755);
        std::fs::set_permissions(&bin, p).unwrap();
        let started = std::time::Instant::now();
        let works = server_binary_works(&bin);
        let elapsed = started.elapsed();
        assert!(
            elapsed < std::time::Duration::from_secs(15),
            "probe took {elapsed:?}"
        );
        assert!(!works, "a probe that never answers is not a working server");
    }
}
