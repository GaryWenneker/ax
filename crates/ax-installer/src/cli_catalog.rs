//! Declarative CLI catalog — tubelord3000-style spawn resolution and detection.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliInstallMethod {
    NativeScript,
    NpmGlobal,
    Winget,
    Scoop,
    Choco,
    Manual,
}

/// npm-global package entry point (spawn via `node script.js`).
#[derive(Debug, Clone, Copy)]
pub struct NpmEntry {
    pub pkg: &'static str,
    pub script: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadlessMode {
    /// `claude -p PROMPT --output-format text`
    PrintOutputText,
    /// `agent -p --output-format text --trust --workspace DIR PROMPT`
    CursorAgent,
    /// `codex exec PROMPT`
    CodexExec,
    /// `gemini -p PROMPT --output-format text -y`
    GeminiPrint,
    /// `opencode run -q PROMPT`
    OpencodeRun,
    /// `kiro-cli chat --no-interactive --trust-all-tools PROMPT`
    KiroChat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    ClaudeLogin,
    CursorLogin,
    CodexLogin,
    GeminiAuth,
    OpencodeLogin,
    KiroLogin,
}

#[derive(Debug, Clone, Copy)]
pub struct InstallSpec {
    pub method: CliInstallMethod,
    pub native_url_unix: Option<&'static str>,
    pub native_url_windows: Option<&'static str>,
    pub npm_package: Option<&'static str>,
    pub manual_url: Option<&'static str>,
    pub manual_hint: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub struct CliCatalogEntry {
    pub id: &'static str,
    pub display_name: &'static str,
    /// Binary name on PATH (`agent` for Cursor, not `cursor`).
    pub bin: &'static str,
    pub npm: Option<NpmEntry>,
    pub install: InstallSpec,
    pub headless: Option<HeadlessMode>,
    pub auth: Option<AuthMode>,
    /// Can run one-shot prompts from the agent terminal.
    pub runnable: bool,
}

#[derive(Debug, Clone)]
pub struct CliSpawn {
    pub program: PathBuf,
    pub args_prefix: Vec<String>,
    pub extra_env: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub extra_env: HashMap<String, String>,
}

static CATALOG: &[CliCatalogEntry] = &[
    CliCatalogEntry {
        id: "claude",
        display_name: "Claude Code",
        bin: "claude",
        npm: Some(NpmEntry {
            pkg: "@anthropic-ai/claude-code",
            script: "cli.js",
        }),
        install: InstallSpec {
            method: CliInstallMethod::NpmGlobal,
            native_url_unix: Some("https://claude.ai/install.sh"),
            native_url_windows: Some("https://claude.ai/install.ps1"),
            npm_package: Some("@anthropic-ai/claude-code"),
            manual_url: None,
            manual_hint: None,
        },
        headless: Some(HeadlessMode::PrintOutputText),
        auth: Some(AuthMode::ClaudeLogin),
        runnable: true,
    },
    CliCatalogEntry {
        id: "cursor",
        display_name: "Cursor",
        bin: "agent",
        npm: None,
        install: InstallSpec {
            method: CliInstallMethod::NativeScript,
            native_url_unix: Some("https://cursor.com/install"),
            native_url_windows: Some("https://cursor.com/install?win32=true"),
            npm_package: None,
            manual_url: Some("https://cursor.com/docs/cli/overview"),
            manual_hint: Some(
                "Installs the Cursor Agent CLI (`agent` command), not the desktop IDE.",
            ),
        },
        headless: Some(HeadlessMode::CursorAgent),
        auth: Some(AuthMode::CursorLogin),
        runnable: true,
    },
    CliCatalogEntry {
        id: "codex",
        display_name: "Codex CLI",
        bin: "codex",
        npm: Some(NpmEntry {
            pkg: "@openai/codex",
            script: "cli.js",
        }),
        install: InstallSpec {
            method: CliInstallMethod::NativeScript,
            native_url_unix: Some("https://chatgpt.com/codex/install.sh"),
            native_url_windows: Some("https://chatgpt.com/codex/install.ps1"),
            npm_package: Some("@openai/codex"),
            manual_url: None,
            manual_hint: None,
        },
        headless: Some(HeadlessMode::CodexExec),
        auth: Some(AuthMode::CodexLogin),
        runnable: true,
    },
    CliCatalogEntry {
        id: "gemini",
        display_name: "Gemini CLI",
        bin: "gemini",
        npm: Some(NpmEntry {
            pkg: "@google/gemini-cli",
            script: "cli.js",
        }),
        install: InstallSpec {
            method: CliInstallMethod::NpmGlobal,
            native_url_unix: None,
            native_url_windows: None,
            npm_package: Some("@google/gemini-cli"),
            manual_url: None,
            manual_hint: None,
        },
        headless: Some(HeadlessMode::GeminiPrint),
        auth: Some(AuthMode::GeminiAuth),
        runnable: true,
    },
    CliCatalogEntry {
        id: "opencode",
        display_name: "opencode",
        bin: "opencode",
        npm: Some(NpmEntry {
            pkg: "opencode-ai",
            script: "cli.js",
        }),
        install: InstallSpec {
            method: if cfg!(target_os = "windows") {
                CliInstallMethod::Scoop
            } else {
                CliInstallMethod::NativeScript
            },
            native_url_unix: Some("https://opencode.ai/install"),
            native_url_windows: None,
            npm_package: Some("opencode-ai"),
            manual_url: Some("https://opencode.ai/docs"),
            manual_hint: Some("On Windows, Scoop or WSL2 + curl script is recommended."),
        },
        headless: Some(HeadlessMode::OpencodeRun),
        auth: Some(AuthMode::OpencodeLogin),
        runnable: true,
    },
    CliCatalogEntry {
        id: "kiro",
        display_name: "Kiro",
        bin: "kiro-cli",
        npm: None,
        install: InstallSpec {
            method: CliInstallMethod::Manual,
            native_url_unix: None,
            native_url_windows: None,
            npm_package: None,
            manual_url: Some("https://kiro.dev"),
            manual_hint: Some("Install Kiro CLI from the official site."),
        },
        headless: Some(HeadlessMode::KiroChat),
        auth: Some(AuthMode::KiroLogin),
        runnable: true,
    },
    CliCatalogEntry {
        id: "hermes",
        display_name: "Hermes Agent",
        bin: "hermes",
        npm: None,
        install: InstallSpec {
            method: CliInstallMethod::Manual,
            native_url_unix: None,
            native_url_windows: None,
            npm_package: None,
            manual_url: Some("https://github.com/anthropics/hermes"),
            manual_hint: Some("Install Hermes Agent from the project docs, then wire MCP."),
        },
        headless: None,
        auth: None,
        runnable: false,
    },
    CliCatalogEntry {
        id: "antigravity",
        display_name: "Antigravity IDE",
        bin: "antigravity",
        npm: None,
        install: InstallSpec {
            method: CliInstallMethod::Manual,
            native_url_unix: None,
            native_url_windows: None,
            npm_package: None,
            manual_url: Some("https://antigravity.google"),
            manual_hint: Some("Install Antigravity IDE from Google."),
        },
        headless: None,
        auth: None,
        runnable: false,
    },
];

pub fn catalog() -> &'static [CliCatalogEntry] {
    CATALOG
}

pub fn catalog_entry(id: &str) -> Option<&'static CliCatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

pub fn cli_bin_name(target: &str) -> Option<&'static str> {
    catalog_entry(target).map(|e| e.bin)
}

/// Whether the CLI binary can be resolved (npm-root, PATH, or ~/.local/bin).
pub fn detect_cli_available(id: &str) -> bool {
    resolve_cli_spawn(id).is_ok()
}

/// Backward-compatible alias.
pub fn is_cli_on_path(target: &str) -> bool {
    detect_cli_available(target)
}

/// Resolve spawn program + optional args prefix (e.g. node + script path).
pub fn resolve_cli_spawn(id: &str) -> Result<CliSpawn, String> {
    let entry = catalog_entry(id).ok_or_else(|| format!("Unknown agent: {id}"))?;
    let bin = entry.bin;

    // 1. npm-root strategy
    if let Some(npm) = entry.npm {
        if let Some(script) = npm_global_script(npm.pkg, npm.script) {
            if let Some(node) = find_node() {
                return Ok(CliSpawn {
                    program: node,
                    args_prefix: vec![script.to_string_lossy().into_owned()],
                    extra_env: HashMap::new(),
                });
            }
        }
    }

    // 2. Cursor Agent CLI — native installer puts binaries in %LOCALAPPDATA%\cursor-agent
    if id == "cursor" {
        if let Some(path) = cursor_agent_bin() {
            return Ok(CliSpawn {
                program: path,
                args_prefix: Vec::new(),
                extra_env: HashMap::new(),
            });
        }
    }

    // 3. where/which candidates — never treat Cursor IDE (`cursor.exe`) as the agent CLI.
    let mut candidates = which_all(bin);
    if id == "cursor" {
        candidates.retain(|p| {
            !p.file_name()
                .is_some_and(|n| n.eq_ignore_ascii_case("cursor.exe"))
        });
    }
    if !candidates.is_empty() {
        if let Some(spawn) = resolve_from_candidates(&candidates, entry.npm) {
            return Ok(spawn);
        }
    }

    // 4. ~/.local/bin fallback (native installers)
    if let Some(home) = dirs::home_dir() {
        let name = if cfg!(target_os = "windows") {
            format!("{bin}.exe")
        } else {
            bin.to_string()
        };
        let candidate = home.join(".local").join("bin").join(&name);
        if candidate.is_file() {
            return Ok(CliSpawn {
                program: candidate,
                args_prefix: Vec::new(),
                extra_env: HashMap::new(),
            });
        }
    }

    Err(format!(
        "{} CLI (`{bin}`) not found — install from Settings → AI Agents",
        entry.display_name
    ))
}

/// Full path to the CLI program (for display / legacy callers).
pub fn resolve_cli_path(target: &str) -> Option<PathBuf> {
    resolve_cli_spawn(target).ok().map(|s| s.program)
}

pub fn build_child_env() -> HashMap<String, String> {
    let mut env: HashMap<String, String> = std::env::vars().collect();
    augment_path(&mut env);
    env
}

pub fn headless_command(
    id: &str,
    prompt: &str,
    workspace: &Path,
    profile_env: &[(String, String)],
) -> Result<CommandSpec, String> {
    let entry = catalog_entry(id).ok_or_else(|| format!("Unknown agent: {id}"))?;
    let mode = entry
        .headless
        .ok_or_else(|| format!("{} is not runnable from the agent terminal", entry.display_name))?;

    let spawn = resolve_cli_spawn(id)?;
    let mut args = spawn.args_prefix.clone();
    match mode {
        HeadlessMode::PrintOutputText => {
            args.extend(["-p".into(), prompt.into(), "--output-format".into(), "text".into()]);
        }
        HeadlessMode::CursorAgent => {
            args.extend([
                "-p".into(),
                "--output-format".into(),
                "text".into(),
                "--trust".into(),
                "--workspace".into(),
                workspace.to_string_lossy().into_owned(),
                prompt.into(),
            ]);
        }
        HeadlessMode::CodexExec => {
            args.extend(["exec".into(), prompt.into()]);
        }
        HeadlessMode::GeminiPrint => {
            args.extend([
                "-p".into(),
                prompt.into(),
                "--output-format".into(),
                "text".into(),
                "-y".into(),
            ]);
        }
        HeadlessMode::OpencodeRun => {
            args.extend(["run".into(), "-q".into(), prompt.into()]);
        }
        HeadlessMode::KiroChat => {
            args.extend([
                "chat".into(),
                "--no-interactive".into(),
                "--trust-all-tools".into(),
                prompt.into(),
            ]);
        }
    }

    let mut extra_env = spawn.extra_env.clone();
    for (k, v) in profile_env {
        extra_env.insert(k.clone(), v.clone());
    }

    Ok(CommandSpec {
        program: spawn.program,
        args,
        extra_env,
    })
}

pub fn auth_command(
    id: &str,
    data_dir: &str,
    profile_env: &[(String, String)],
) -> Result<CommandSpec, String> {
    let entry = catalog_entry(id).ok_or_else(|| format!("Unknown agent: {id}"))?;
    let auth = entry
        .auth
        .ok_or_else(|| format!("Auth not supported for {}", entry.display_name))?;

    let spawn = resolve_cli_spawn(id)?;
    let mut args = spawn.args_prefix.clone();
    let mut extra_env = spawn.extra_env.clone();

    match auth {
        AuthMode::ClaudeLogin => {
            extra_env.insert("CLAUDE_CONFIG_DIR".into(), data_dir.into());
            args.extend(["auth".into(), "login".into()]);
        }
        AuthMode::CursorLogin => {
            for (k, v) in profile_env {
                extra_env.insert(k.clone(), v.clone());
            }
            args.push("login".into());
        }
        AuthMode::CodexLogin => {
            args.push("login".into());
        }
        AuthMode::GeminiAuth => {
            args.extend(["auth".into(), "login".into()]);
        }
        AuthMode::OpencodeLogin => {
            args.extend(["auth".into(), "login".into()]);
        }
        AuthMode::KiroLogin => {
            args.push("login".into());
        }
    }

    Ok(CommandSpec {
        program: spawn.program,
        args,
        extra_env,
    })
}

pub fn native_install_url(entry: &CliCatalogEntry) -> Option<&'static str> {
    if cfg!(target_os = "windows") {
        entry.install.native_url_windows
    } else {
        entry.install.native_url_unix
    }
}

// ─── Internal helpers ───────────────────────────────────────────────────────

fn npm_global_root() -> Option<PathBuf> {
    let npm = find_on_path("npm")?;
    let output = Command::new(&npm).arg("root").arg("-g").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(PathBuf::from(text))
}

fn npm_global_script(pkg: &str, script: &str) -> Option<PathBuf> {
    let root = npm_global_root()?;
    let candidates = [
        script,
        "cli.js",
        "dist/index.js",
        "bin/cli.js",
    ];
    for name in candidates {
        let path = root.join(pkg).join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

fn find_node() -> Option<PathBuf> {
    if let Ok(exe) = std::env::var("NODE") {
        let p = PathBuf::from(&exe);
        if p.is_file() {
            return Some(p);
        }
    }
    find_on_path("node").map(PathBuf::from)
}

fn which_all(name: &str) -> Vec<PathBuf> {
    let program = if cfg!(target_os = "windows") {
        "where"
    } else {
        "which"
    };
    let output = match Command::new(program).arg(name).output() {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn find_on_path(name: &str) -> Option<String> {
    which_all(name)
        .into_iter()
        .next()
        .map(|p| p.to_string_lossy().into_owned())
}

fn resolve_from_candidates(candidates: &[PathBuf], npm: Option<NpmEntry>) -> Option<CliSpawn> {
    if cfg!(target_os = "windows") {
        resolve_windows_candidates(candidates, npm)
    } else if let Some(first) = candidates.first() {
        Some(CliSpawn {
            program: first.clone(),
            args_prefix: Vec::new(),
            extra_env: HashMap::new(),
        })
    } else {
        None
    }
}

fn resolve_windows_candidates(candidates: &[PathBuf], npm: Option<NpmEntry>) -> Option<CliSpawn> {
    if let Some(exe) = candidates.iter().find(|c| c.extension().is_some_and(|e| e == "exe")) {
        return Some(CliSpawn {
            program: exe.clone(),
            args_prefix: Vec::new(),
            extra_env: HashMap::new(),
        });
    }

    if let Some(cmd) = candidates
        .iter()
        .find(|c| c.extension().is_some_and(|e| e == "cmd" || e == "bat"))
    {
        let exe_sibling = cmd.with_extension("exe");
        if exe_sibling.is_file() {
            return Some(CliSpawn {
                program: exe_sibling,
                args_prefix: Vec::new(),
                extra_env: HashMap::new(),
            });
        }

        if let Ok(content) = std::fs::read_to_string(cmd) {
            if let Some(script) = parse_cmd_script_path(&content, cmd) {
                if let Some(node) = find_node() {
                    return Some(CliSpawn {
                        program: node,
                        args_prefix: vec![script.to_string_lossy().into_owned()],
                        extra_env: HashMap::new(),
                    });
                }
            }
        }

        let comspec = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into());
        return Some(CliSpawn {
            program: PathBuf::from(comspec),
            args_prefix: vec!["/c".into(), cmd.to_string_lossy().into_owned()],
            extra_env: HashMap::new(),
        });
    }

    // npm fallback via cmd shim directory
    if npm.is_some() {
        if let Some(first) = candidates.first() {
            return Some(CliSpawn {
                program: first.clone(),
                args_prefix: Vec::new(),
                extra_env: HashMap::new(),
            });
        }
    }

    candidates.first().map(|p| CliSpawn {
        program: p.clone(),
        args_prefix: Vec::new(),
        extra_env: HashMap::new(),
    })
}

/// Extract the `.js` path string from an npm `.cmd` line (before filesystem resolution).
pub fn extract_js_from_cmd_line(line: &str) -> Option<String> {
    let line = line.trim();
    let marker = line.find(".js\" %*").or_else(|| line.find(".js\"%*"))?;
    let before_marker = &line[..marker];
    let start = before_marker.rfind('"')?;
    let raw = &line[start + 1..marker + 3];
    if raw.ends_with(".js") {
        Some(raw.to_string())
    } else {
        None
    }
}

/// Extract `node script.js` path from npm-generated `.cmd` wrapper.
pub fn parse_cmd_script_path(content: &str, cmd_path: &Path) -> Option<PathBuf> {
    let re_match = content.lines().rev().find_map(extract_js_from_cmd_line)?;

    let cmd_dir = cmd_path.parent()?;
    let script_raw = re_match
        .replace("%~dp0%\\", "")
        .replace("%dp0%\\", "")
        .replace("%~dp0%/", "")
        .replace("%dp0%/", "");
    let script_abs = if Path::new(&script_raw).is_absolute() {
        PathBuf::from(&script_raw)
    } else {
        cmd_dir.join(&script_raw)
    };
    if script_abs.is_file() {
        Some(script_abs)
    } else {
        None
    }
}

/// Windows: `%LOCALAPPDATA%\cursor-agent` (Cursor Agent CLI installer target).
fn cursor_agent_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        local_app_data().map(|local| local.join("cursor-agent"))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

fn cursor_agent_bin() -> Option<PathBuf> {
    let dir = cursor_agent_dir()?;
    for name in ["agent.exe", "agent.cmd", "cursor-agent.exe", "cursor-agent.cmd"] {
        let path = dir.join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn local_app_data() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("LOCALAPPDATA") {
        if !v.is_empty() {
            return Some(PathBuf::from(v));
        }
    }
    dirs::home_dir().map(|h| h.join("AppData").join("Local"))
}

fn augment_path(env: &mut HashMap<String, String>) {
    let key = if cfg!(target_os = "windows") {
        "Path"
    } else {
        "PATH"
    };

    let mut extras: Vec<PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        extras.push(home.join(".local").join("bin"));
        extras.push(home.join(".npm-global").join("bin"));
        if cfg!(not(target_os = "windows")) {
            extras.push(PathBuf::from("/usr/local/bin"));
            extras.push(PathBuf::from("/usr/local/sbin"));
            extras.push(PathBuf::from("/opt/homebrew/bin"));
            extras.push(PathBuf::from("/opt/homebrew/sbin"));
        }
    }

    if cfg!(target_os = "windows") {
        if let Ok(prefix) = npm_prefix() {
            extras.push(prefix);
        }
        if let Some(dir) = cursor_agent_dir() {
            extras.push(dir);
        }
    }

    let current = env.get(key).cloned().unwrap_or_default();
    let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
    let mut parts: Vec<String> = current
        .split(sep)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    for extra in extras {
        let s = extra.to_string_lossy().into_owned();
        if !parts.iter().any(|p| p.eq_ignore_ascii_case(&s)) && extra.is_dir() {
            parts.insert(0, s);
        }
    }

    env.insert(key.into(), parts.join(sep));
}

fn npm_prefix() -> Result<PathBuf, String> {
    let npm = find_on_path("npm").ok_or_else(|| "npm not found".to_string())?;
    let output = Command::new(&npm)
        .args(["prefix", "-g"])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("npm prefix failed".into());
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_covers_all_targets() {
        for id in [
            "claude", "cursor", "codex", "opencode", "hermes", "gemini", "antigravity", "kiro",
        ] {
            assert!(catalog_entry(id).is_some(), "missing catalog entry for {id}");
        }
    }

    #[test]
    fn cursor_uses_agent_bin() {
        assert_eq!(catalog_entry("cursor").unwrap().bin, "agent");
    }

    #[test]
    fn kiro_uses_kiro_cli_bin() {
        assert_eq!(catalog_entry("kiro").unwrap().bin, "kiro-cli");
    }

    #[test]
    fn hermes_not_runnable() {
        assert!(!catalog_entry("hermes").unwrap().runnable);
    }

    #[test]
    fn extract_js_from_cmd_line_parses_dp0() {
        let line = r#""%_prog%"  "%dp0%\cli.js" %*"#;
        assert_eq!(extract_js_from_cmd_line(line), Some(r"%dp0%\cli.js".to_string()));
        let normalized = r"%dp0%\cli.js".replace("%dp0%\\", "");
        assert_eq!(normalized, "cli.js");
    }

    #[test]
    fn parse_cmd_extracts_cli_js() {
        let dir = std::env::temp_dir().join(format!("ax-cli-catalog-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let script = dir.join("cli.js");
        std::fs::write(&script, "// test").expect("write script");
        let cmd_file = dir.join("claude.cmd");
        let rel_content = r#""%_prog%"  "%dp0%\cli.js" %*"#.to_string();
        std::fs::write(&cmd_file, &rel_content).expect("write cmd");
        let parsed = parse_cmd_script_path(&rel_content, &cmd_file);
        assert_eq!(parsed, Some(script));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
