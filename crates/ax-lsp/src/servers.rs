//! Known language servers ax can spawn for enrichment.

use std::path::{Path, PathBuf};

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
    SERVERS
        .iter()
        .map(|s| {
            let path = resolve_command(s.command);
            let available = path
                .as_ref()
                .map(|p| server_binary_works(p))
                .unwrap_or(false);
            ServerStatus {
                id: s.id.into(),
                command: s.command.into(),
                available,
                path: path.map(|p| p.display().to_string()),
                languages: vec![format!("{:?}", s.language).to_ascii_lowercase()],
            }
        })
        .collect()
}

/// Resolve a language-server command on PATH.
///
/// On Windows, prefer `.exe` / `.cmd` / `.bat` over extensionless Unix shims
/// (Volta installs both a bash script and a `.cmd` wrapper).
fn resolve_command(command: &str) -> Option<PathBuf> {
    let candidates: Vec<PathBuf> = which::which_all(command).ok()?.collect();
    if candidates.is_empty() {
        return None;
    }
    #[cfg(windows)]
    {
        let preferred = candidates.iter().find(|p| {
            matches!(
                p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_ascii_lowercase())
                    .as_deref(),
                Some("exe") | Some("cmd") | Some("bat")
            )
        });
        return preferred.cloned().or_else(|| candidates.into_iter().next());
    }
    #[cfg(not(windows))]
    {
        candidates.into_iter().next()
    }
}

fn probe_args(path: &Path, args: &[&str]) -> Option<std::process::Output> {
    std::process::Command::new(path)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .ok()
}

fn looks_like_missing_shim(output: &std::process::Output) -> bool {
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    combined.contains("Unknown binary") || combined.contains("is not installed")
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
    resolve_command(spec.command)
        .map(|p| server_binary_works(&p))
        .unwrap_or(false)
}

pub fn spec_for_extension(ext: &str) -> Option<&'static ServerSpec> {
    let e = ext.trim_start_matches('.').to_ascii_lowercase();
    SERVERS.iter().find(|s| s.extensions.iter().any(|x| *x == e))
}
