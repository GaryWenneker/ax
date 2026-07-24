//! Agent target installers — CG: installer/targets/*.ts

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::report::{FileAction, InstallSummary, TargetReport};
use crate::cli_catalog::{catalog_entry, detect_cli_available};
use crate::cli_install::cli_installable;

pub const TARGETS: &[&str] = &[
    "claude", "cursor", "codex", "opencode", "hermes", "gemini", "antigravity", "kiro",
    "vscode", "windsurf", "zed",
];

pub fn display_name(target: &str) -> &'static str {
    match target {
        "claude" => "Claude Code",
        "cursor" => "Cursor",
        "codex" => "Codex CLI",
        "opencode" => "opencode",
        "hermes" => "Hermes Agent",
        "gemini" => "Gemini CLI",
        "antigravity" => "Antigravity IDE",
        "kiro" => "Kiro",
        "vscode" => "VS Code (Copilot Chat)",
        "windsurf" => "Windsurf (Cascade)",
        "zed" => "Zed",
        _ => "Unknown",
    }
}

pub fn is_detected(target: &str) -> bool {
    detect_cli_available(target) || has_agent_data_dir(target)
}

fn has_agent_data_dir(target: &str) -> bool {
    let Ok(home) = home_dir() else {
        return false;
    };
    match target {
        "claude" => home.join(".claude").is_dir() || home.join(".claude.json").is_file(),
        "cursor" => home.join(".cursor").is_dir(),
        "codex" => home.join(".codex").is_dir(),
        "opencode" => opencode_config_path().map(|p| p.exists()).unwrap_or(false)
            || home.join(".config").join("opencode").is_dir(),
        "hermes" => hermes_config_path().map(|p| p.parent().is_some_and(|d| d.is_dir())).unwrap_or(false),
        "gemini" => home.join(".gemini").is_dir(),
        "antigravity" => home.join(".gemini").is_dir(),
        "kiro" => home.join(".kiro").is_dir(),
        "vscode" => home.join(".vscode").is_dir() || vscode_user_dir().map(|p| p.is_dir()).unwrap_or(false),
        "windsurf" => windsurf_config_dir().map(|p| p.is_dir()).unwrap_or(false),
        "zed" => zed_settings_path().map(|p| p.parent().is_some_and(|d| d.is_dir())).unwrap_or(false),
        _ => false,
    }
}

pub fn install_detected(project_root: &Path, install_all: bool) -> Result<InstallSummary, String> {
    let mut reports = Vec::new();
    let mut any = false;
    for target in TARGETS {
        if !install_all && !is_detected(target) {
            continue;
        }
        if let Some(report) = install_target(target, project_root)? {
            if report.touched() || !report.notes.is_empty() {
                any = true;
                reports.push(report);
            }
        }
    }
    // --yes fallback: configure Claude + Cursor when nothing was detected (CG parity).
    if !any && install_all {
        for target in ["claude", "cursor"] {
            if let Some(report) = install_target(target, project_root)? {
                reports.push(report);
            }
        }
    }
    Ok(InstallSummary { reports })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AgentTargetStatus {
    pub id: String,
    pub display_name: String,
    pub bin: String,
    pub detected: bool,
    pub cli_available: bool,
    /// Backward-compatible alias for `cli_available`.
    pub cli_on_path: bool,
    pub data_dir_detected: bool,
    pub runnable: bool,
    pub cli_installable: bool,
    pub configured: bool,
    pub config_paths: Vec<String>,
}

pub fn agent_status(project_root: &Path) -> Result<Vec<AgentTargetStatus>, String> {
    TARGETS
        .iter()
        .map(|&id| target_status(id, project_root))
        .collect()
}

fn target_status(id: &str, project_root: &Path) -> Result<AgentTargetStatus, String> {
    let (configured, paths) = is_ax_configured(id, project_root)?;
    let cli_available = detect_cli_available(id);
    let data_dir_detected = has_agent_data_dir(id);
    let entry = catalog_entry(id);
    Ok(AgentTargetStatus {
        id: id.to_string(),
        display_name: display_name(id).to_string(),
        bin: entry.map(|e| e.bin.to_string()).unwrap_or_else(|| id.to_string()),
        detected: cli_available || data_dir_detected,
        cli_available,
        cli_on_path: cli_available,
        data_dir_detected,
        runnable: entry.map(|e| e.runnable).unwrap_or(false),
        cli_installable: cli_installable(id),
        configured,
        config_paths: paths,
    })
}

/// Enriched catalog entries for the agent terminal UI.
pub fn catalog_with_status(project_root: &Path) -> Result<Vec<AgentTargetStatus>, String> {
    agent_status(project_root)
}

pub fn install_targets(
    project_root: &Path,
    selected: &[String],
) -> Result<InstallSummary, String> {
    let mut reports = Vec::new();
    for target in selected {
        if let Some(report) = install_target(target, project_root)? {
            reports.push(report);
        }
    }
    Ok(InstallSummary { reports })
}

pub fn uninstall_targets(selected: &[String]) -> Result<Vec<TargetReport>, String> {
    let mut reports = Vec::new();
    for target in selected {
        if let Some(report) = uninstall_target(target)? {
            reports.push(report);
        }
    }
    Ok(reports)
}

fn is_ax_configured(target: &str, project_root: &Path) -> Result<(bool, Vec<String>), String> {
    let home = home_dir()?;
    let bin = ax_bin();
    let paths: Vec<PathBuf> = match target {
        "cursor" => vec![home.join(".cursor").join("mcp.json")],
        "claude" => vec![
            home.join(".claude.json"),
            project_root.join(".mcp.json"),
        ],
        "codex" => vec![home.join(".codex").join("config.toml")],
        "opencode" => vec![opencode_config_path()?],
        "gemini" => vec![home.join(".gemini").join("settings.json")],
        "antigravity" => vec![antigravity_mcp_path()?],
        "kiro" => vec![home.join(".kiro").join("settings").join("mcp.json")],
        "hermes" => vec![hermes_config_path()?],
        "vscode" => vec![vscode_mcp_path(project_root)],
        "windsurf" => vec![windsurf_mcp_path()?],
        "zed" => vec![zed_settings_path()?],
        _ => return Ok((false, Vec::new())),
    };
    let str_paths: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let configured = paths.iter().any(|p| config_has_ax(p, &bin, project_root));
    Ok((configured, str_paths))
}

fn config_has_ax(path: &Path, bin: &str, project_root: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    let content = fs::read_to_string(path).unwrap_or_default();
    if path.extension().and_then(|e| e.to_str()) == Some("toml") {
        return content.contains("[mcp_servers.ax]") && content.contains(bin);
    }
    if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
        return content.contains("mcp_servers:") && content.contains("  ax:") && content.contains(bin);
    }
    // Zed / VS Code settings files may contain JSONC comments — strip before parsing.
    let val = read_json_lenient(path);
    if val
        .get("mcpServers")
        .and_then(|v| v.get("ax"))
        .is_some()
    {
        return val
            .get("mcpServers")
            .and_then(|v| v.get("ax"))
            .and_then(|e| e.get("command"))
            .and_then(|c| c.as_str())
            .map(|c| c.contains(bin) || bin.ends_with(c))
            .unwrap_or(true);
    }
    if val.get("mcp").and_then(|v| v.get("ax")).is_some() {
        return true;
    }
    // VS Code `.vscode/mcp.json` root key.
    if val.get("servers").and_then(|v| v.get("ax")).is_some() {
        return true;
    }
    // Zed `settings.json` root key.
    if val.get("context_servers").and_then(|v| v.get("ax")).is_some() {
        return true;
    }
    let _ = project_root;
    false
}

pub fn uninstall_all() -> Result<Vec<TargetReport>, String> {
    let mut reports = Vec::new();
    for target in TARGETS {
        if let Some(report) = uninstall_target(target)? {
            reports.push(report);
        }
    }
    Ok(reports)
}

fn install_target(target: &str, project_root: &Path) -> Result<Option<TargetReport>, String> {
    let report = match target {
        "cursor" => install_cursor_mcp(project_root)?,
        "claude" => install_claude_mcp(project_root)?,
        "codex" => install_codex_mcp(project_root)?,
        "opencode" => install_opencode_mcp(project_root)?,
        "hermes" => install_hermes_mcp(project_root)?,
        "gemini" => install_gemini_mcp(project_root)?,
        "antigravity" => install_antigravity_mcp(project_root)?,
        "kiro" => install_kiro_mcp(project_root)?,
        "vscode" => install_vscode_mcp(project_root)?,
        "windsurf" => install_windsurf_mcp(project_root)?,
        "zed" => install_zed_mcp(project_root)?,
        _ => return Ok(None),
    };
    Ok(Some(report))
}

fn uninstall_target(target: &str) -> Result<Option<TargetReport>, String> {
    let report = match target {
        "cursor" => uninstall_cursor_mcp()?,
        "claude" => uninstall_claude_mcp()?,
        "codex" => uninstall_codex_mcp()?,
        "opencode" => uninstall_opencode_mcp()?,
        "hermes" => uninstall_hermes_mcp()?,
        "gemini" => uninstall_gemini_mcp()?,
        "antigravity" => uninstall_antigravity_mcp()?,
        "kiro" => uninstall_kiro_mcp()?,
        "vscode" => uninstall_vscode_mcp()?,
        "windsurf" => uninstall_windsurf_mcp()?,
        "zed" => uninstall_zed_mcp()?,
        _ => return Ok(None),
    };
    Ok(Some(report))
}

fn ax_bin() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "ax".to_string())
}

/// VS Code / Cursor lineage — resolved per open workspace.
const VS_WORKSPACE: &str = "${workspaceFolder}";

/// Claude Code injects this when spawning MCP servers.
const CLAUDE_WORKSPACE: &str = "${CLAUDE_PROJECT_DIR:-.}";

/// Agents that set MCP process cwd to the active workspace.
const PROCESS_CWD: &str = ".";

/// Workspace path token for MCP `--path` / `cwd` (never a fixed install-time directory).
fn mcp_path_token(target: &str) -> &'static str {
    match target {
        "claude" => CLAUDE_WORKSPACE,
        "codex" | "hermes" => PROCESS_CWD,
        _ => VS_WORKSPACE,
    }
}

fn mcp_serve_args(path_token: &str) -> Vec<String> {
    vec![
        "serve".to_string(),
        "--mcp".to_string(),
        "--path".to_string(),
        path_token.to_string(),
    ]
}

fn mcp_config_entry(target: &str) -> Value {
    let path_token = mcp_path_token(target);
    serde_json::json!({
        "command": ax_bin(),
        "args": mcp_serve_args(path_token),
        "cwd": path_token,
    })
}

fn antigravity_mcp_entry() -> Value {
    mcp_config_entry("antigravity")
}

fn replace_toml_section(content: &str, section: &str, block: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut skip = false;
    for line in lines {
        if line.trim() == section {
            skip = true;
            continue;
        }
        if skip {
            if line.starts_with('[') {
                skip = false;
                out.push(line);
            }
            continue;
        }
        out.push(line);
    }
    let mut result = out.join("\n");
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }
    result.push_str(block);
    result
}

fn codex_ax_block(bin: &str) -> String {
    let path = mcp_path_token("codex");
    format!(
        "[mcp_servers.ax]\ncommand = \"{bin}\"\nargs = [\"serve\", \"--mcp\", \"--path\", \"{path}\"]\ncwd = \"{path}\"\n",
    )
}

fn hermes_ax_block(bin: &str) -> String {
    let path = mcp_path_token("hermes");
    format!(
        "mcp_servers:\n  ax:\n    command: {bin}\n    args:\n      - serve\n      - --mcp\n      - --path\n      - {path}\n    cwd: {path}\n    timeout: 120\n    connect_timeout: 60\n    enabled: true\nplatform_toolsets:\n  cli:\n    - mcp-ax\n",
    )
}

fn replace_hermes_ax_block(content: &str, block: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut skip = false;
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "mcp_servers:" || trimmed == "  ax:" {
            skip = true;
            continue;
        }
        if skip {
            if trimmed == "platform_toolsets:" {
                skip = false;
                continue;
            }
            if !line.starts_with(' ') && !line.is_empty() {
                skip = false;
                out.push(line);
            }
            continue;
        }
        if trimmed == "platform_toolsets:" {
            continue;
        }
        if trimmed == "cli:" || trimmed == "- mcp-ax" {
            continue;
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    let mut result = out.join("\n");
    if !result.is_empty() {
        result.push('\n');
    }
    result.push('\n');
    result.push_str(block);
    result
}

fn home_dir() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "no home dir".to_string())
}

fn read_json(path: &Path) -> Value {
    if path.exists() {
        let content = fs::read_to_string(path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    }
}

fn json_equal(a: &Value, b: &Value) -> bool {
    serde_json::to_string(a).ok() == serde_json::to_string(b).ok()
}

fn write_json_action(path: &Path, value: &Value) -> Result<FileAction, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let existed = path.exists();
    let before = read_json(path);
    if existed && json_equal(&before, value) {
        return Ok(FileAction::Unchanged);
    }
    fs::write(
        path,
        serde_json::to_string_pretty(value).unwrap_or_default(),
    )
    .map_err(|e| e.to_string())?;
    Ok(if existed {
        FileAction::Updated
    } else {
        FileAction::Created
    })
}

fn upsert_mcp_servers(path: &Path, target: &str) -> Result<FileAction, String> {
    let mut config = read_json(path);
    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }
    config["mcpServers"]["ax"] = mcp_config_entry(target);
    write_json_action(path, &config)
}

fn remove_mcp_servers(path: &Path) -> Result<Option<FileAction>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let mut config = read_json(path);
    let had = config
        .get("mcpServers")
        .and_then(|v| v.get("ax"))
        .is_some();
    if !had {
        return Ok(None);
    }
    if let Some(servers) = config.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        servers.remove("ax");
    }
    Ok(Some(write_json_action(path, &config)?))
}

fn install_cursor_mcp(_project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("cursor", display_name("cursor"));
    let path = home_dir()?.join(".cursor").join("mcp.json");
    let action = upsert_mcp_servers(&path, "cursor")?;
    report.push_file(path, action);
    report.note("Restart Cursor for MCP changes to take effect.");
    Ok(report)
}

fn uninstall_cursor_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("cursor", display_name("cursor"));
    let path = home_dir()?.join(".cursor").join("mcp.json");
    if let Some(action) = remove_mcp_servers(&path)? {
        report.push_file(path, action);
    }
    Ok(report)
}

fn install_claude_mcp(project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("claude", display_name("claude"));
    let home = home_dir()?;
    let global = home.join(".claude.json");
    report.push_file(global.clone(), upsert_mcp_servers(&global, "claude")?);
    let local = project_root.join(".mcp.json");
    report.push_file(local.clone(), upsert_mcp_servers(&local, "claude")?);
    let settings = home.join(".claude").join("settings.json");
    if let Some((path, action)) = install_claude_hook(&settings, "UserPromptSubmit", "prompt-hook")? {
        report.push_file(path, action);
    }
    // Stop + SubagentStop: turn-end policy-guard check (see `ax stop-hook`).
    // Skippable via AX_NO_STOP_HOOK=1 at hook-run time if it proves noisy.
    if let Some((path, action)) = install_claude_hook(&settings, "Stop", "stop-hook")? {
        report.push_file(path, action);
    }
    if let Some((path, action)) = install_claude_hook(&settings, "SubagentStop", "stop-hook")? {
        report.push_file(path, action);
    }
    Ok(report)
}

fn uninstall_claude_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("claude", display_name("claude"));
    let home = home_dir()?;
    let global = home.join(".claude.json");
    if let Some(action) = remove_mcp_servers(&global)? {
        report.push_file(global, action);
    }
    let settings = home.join(".claude").join("settings.json");
    let mut touched = false;
    touched |= remove_claude_hook(&settings, "UserPromptSubmit", "prompt-hook")?;
    touched |= remove_claude_hook(&settings, "Stop", "stop-hook")?;
    touched |= remove_claude_hook(&settings, "SubagentStop", "stop-hook")?;
    if touched {
        report.push_file(settings, FileAction::Updated);
    }
    Ok(report)
}

/// Register `ax <hook_subcommand>` under `hooks.<event>` in Claude's `settings.json`.
/// Idempotent — matches on the subcommand name already appearing in a `command` string.
fn install_claude_hook(
    settings_path: &Path,
    event: &str,
    hook_subcommand: &str,
) -> Result<Option<(PathBuf, FileAction)>, String> {
    let bin = ax_bin();
    let hook_cmd = format!("{bin} {hook_subcommand}");
    let mut settings = read_json(settings_path);
    if settings.get("hooks").is_none() {
        settings["hooks"] = serde_json::json!({});
    }
    let hooks = settings["hooks"].as_object_mut().ok_or("invalid hooks")?;
    if hooks.get(event).is_none() {
        hooks.insert(event.to_string(), serde_json::json!([]));
    }
    let groups = hooks
        .get_mut(event)
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| format!("invalid {event}"))?;
    let already = hook_group_matches(groups, hook_subcommand);
    if already {
        return Ok(Some((settings_path.to_path_buf(), FileAction::Unchanged)));
    }
    groups.push(serde_json::json!({
        "hooks": [{ "type": "command", "command": hook_cmd }]
    }));
    let action = write_json_action(settings_path, &settings)?;
    Ok(Some((settings_path.to_path_buf(), action)))
}

fn remove_claude_hook(settings_path: &Path, event: &str, hook_subcommand: &str) -> Result<bool, String> {
    if !settings_path.exists() {
        return Ok(false);
    }
    let mut settings = read_json(settings_path);
    let Some(hooks) = settings.get_mut("hooks").and_then(|v| v.as_object_mut()) else {
        return Ok(false);
    };
    let Some(groups) = hooks.get_mut(event).and_then(|v| v.as_array_mut()) else {
        return Ok(false);
    };
    let before = groups.len();
    groups.retain(|g| !hook_group_matches(std::slice::from_ref(g), hook_subcommand));
    if groups.len() == before {
        return Ok(false);
    }
    write_json_action(settings_path, &settings)?;
    Ok(true)
}

fn hook_group_matches(groups: &[Value], hook_subcommand: &str) -> bool {
    groups.iter().any(|g| {
        g.get("hooks")
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter().any(|e| {
                    e.get("command")
                        .and_then(|c| c.as_str())
                        .map(|s| s.contains(hook_subcommand))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    })
}

fn install_codex_mcp(_project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("codex", display_name("codex"));
    let dir = home_dir()?.join(".codex");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("config.toml");
    let content = if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let block = codex_ax_block(&ax_bin());
    let out = replace_toml_section(&content, "[mcp_servers.ax]", &block);
    let action = write_text_action(&path, &out)?;
    report.push_file(path, action);
    Ok(report)
}

fn uninstall_codex_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("codex", display_name("codex"));
    let path = home_dir()?.join(".codex").join("config.toml");
    if !path.exists() || !fs::read_to_string(&path).unwrap_or_default().contains("[mcp_servers.ax]") {
        return Ok(report);
    }
    let content = fs::read_to_string(&path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let mut out = Vec::new();
    let mut skip = false;
    for line in lines {
        if line.trim() == "[mcp_servers.ax]" {
            skip = true;
            continue;
        }
        if skip {
            if line.starts_with('[') {
                skip = false;
                out.push(line);
            }
            continue;
        }
        out.push(line);
    }
    let new_content = out.join("\n") + "\n";
    let action = write_text_action(&path, &new_content)?;
    report.push_file(path, action);
    Ok(report)
}

fn write_text_action(path: &Path, content: &str) -> Result<FileAction, String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let existed = path.exists();
    if existed {
        let old = fs::read_to_string(path).unwrap_or_default();
        if old == content {
            return Ok(FileAction::Unchanged);
        }
    }
    fs::write(path, content).map_err(|e| e.to_string())?;
    Ok(if existed {
        FileAction::Updated
    } else {
        FileAction::Created
    })
}

fn opencode_config_dir() -> Result<PathBuf, String> {
    let xdg = match std::env::var("XDG_CONFIG_HOME") {
        Ok(s) if !s.trim().is_empty() => PathBuf::from(s),
        _ => home_dir()?.join(".config"),
    };
    Ok(xdg.join("opencode"))
}

fn opencode_config_path() -> Result<PathBuf, String> {
    let dir = opencode_config_dir()?;
    let jsonc = dir.join("opencode.jsonc");
    let json = dir.join("opencode.json");
    if jsonc.exists() {
        Ok(jsonc)
    } else if json.exists() {
        Ok(json)
    } else {
        Ok(jsonc)
    }
}

fn install_opencode_mcp(_project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("opencode", display_name("opencode"));
    let path = opencode_config_path()?;
    let bin = ax_bin();
    let path_token = mcp_path_token("opencode");
    let mut config = read_json(&path);
    if config.get("mcp").is_none() {
        config["mcp"] = serde_json::json!({});
    }
    let args: Vec<String> = std::iter::once(bin)
        .chain(mcp_serve_args(path_token))
        .collect();
    config["mcp"]["ax"] = serde_json::json!({
        "type": "local",
        "command": args,
        "enabled": true,
        "cwd": path_token,
    });
    report.push_file(path.clone(), write_json_action(&path, &config)?);
    if let Ok(app_data) = std::env::var("APPDATA") {
        let legacy = PathBuf::from(app_data).join("opencode").join("opencode.jsonc");
        if legacy.exists() && legacy != path {
            let mut legacy_cfg = read_json(&legacy);
            if legacy_cfg.get("mcp").and_then(|v| v.get("ax")).is_some() {
                if let Some(mcp) = legacy_cfg.get_mut("mcp").and_then(|v| v.as_object_mut()) {
                    mcp.remove("ax");
                    report.push_file(legacy.clone(), write_json_action(&legacy, &legacy_cfg)?);
                }
            }
        }
    }
    Ok(report)
}

fn uninstall_opencode_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("opencode", display_name("opencode"));
    let path = opencode_config_path()?;
    if path.exists() {
        let mut config = read_json(&path);
        if config.get("mcp").and_then(|v| v.get("ax")).is_some() {
            if let Some(mcp) = config.get_mut("mcp").and_then(|v| v.as_object_mut()) {
                mcp.remove("ax");
                report.push_file(path.clone(), write_json_action(&path, &config)?);
            }
        }
    }
    Ok(report)
}

fn install_gemini_mcp(_project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("gemini", display_name("gemini"));
    let dir = home_dir()?.join(".gemini");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("settings.json");
    report.push_file(path.clone(), upsert_mcp_servers(&path, "gemini")?);
    report.note("Restart Gemini CLI for MCP changes to take effect.");
    Ok(report)
}

fn uninstall_gemini_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("gemini", display_name("gemini"));
    let path = home_dir()?.join(".gemini").join("settings.json");
    if let Some(action) = remove_mcp_servers(&path)? {
        report.push_file(path, action);
    }
    Ok(report)
}

fn antigravity_mcp_path() -> Result<PathBuf, String> {
    let unified_dir = home_dir()?.join(".gemini").join("config");
    let unified = unified_dir.join("mcp_config.json");
    let marker = unified_dir.join(".migrated");
    let legacy = home_dir()?.join(".gemini").join("antigravity").join("mcp_config.json");
    if marker.exists() || unified.exists() {
        Ok(unified)
    } else {
        Ok(legacy)
    }
}

fn install_antigravity_mcp(_project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("antigravity", display_name("antigravity"));
    let path = antigravity_mcp_path()?;
    let mut config = read_json(&path);
    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }
    config["mcpServers"]["ax"] = antigravity_mcp_entry();
    report.push_file(path.clone(), write_json_action(&path, &config)?);
    report.note("Restart Antigravity for MCP changes to take effect.");
    Ok(report)
}

fn uninstall_antigravity_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("antigravity", display_name("antigravity"));
    for path in [
        home_dir()?.join(".gemini").join("config").join("mcp_config.json"),
        home_dir()?.join(".gemini").join("antigravity").join("mcp_config.json"),
    ] {
        if let Some(action) = remove_mcp_servers(&path)? {
            report.push_file(path, action);
        }
    }
    Ok(report)
}

fn install_kiro_mcp(_project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("kiro", display_name("kiro"));
    let path = home_dir()?.join(".kiro").join("settings").join("mcp.json");
    report.push_file(path.clone(), upsert_mcp_servers(&path, "kiro")?);
    Ok(report)
}

fn uninstall_kiro_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("kiro", display_name("kiro"));
    let path = home_dir()?.join(".kiro").join("settings").join("mcp.json");
    if let Some(action) = remove_mcp_servers(&path)? {
        report.push_file(path, action);
    }
    Ok(report)
}

/// VS Code writes MCP config under the workspace-local `.vscode/mcp.json`,
/// using root key `servers` (not `mcpServers`) with a required `type` field.
fn vscode_mcp_path(project_root: &Path) -> PathBuf {
    project_root.join(".vscode").join("mcp.json")
}

fn vscode_user_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(|a| PathBuf::from(a).join("Code").join("User"))
    }
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir().map(|h| {
            h.join("Library")
                .join("Application Support")
                .join("Code")
                .join("User")
        })
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        dirs::home_dir().map(|h| h.join(".config").join("Code").join("User"))
    }
}

fn vscode_mcp_entry() -> Value {
    let path_token = mcp_path_token("vscode");
    serde_json::json!({
        "type": "stdio",
        "command": ax_bin(),
        "args": mcp_serve_args(path_token),
        "cwd": path_token,
    })
}

fn install_vscode_mcp(project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("vscode", display_name("vscode"));
    let path = vscode_mcp_path(project_root);
    let mut config = read_json(&path);
    if config.get("servers").is_none() {
        config["servers"] = serde_json::json!({});
    }
    config["servers"]["ax"] = vscode_mcp_entry();
    report.push_file(path.clone(), write_json_action(&path, &config)?);
    report.note("Reload the VS Code window and set Copilot Chat to Agent mode for MCP tools.");
    Ok(report)
}

fn uninstall_vscode_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("vscode", display_name("vscode"));
    // Best-effort: workspace path unknown at uninstall time for a home-only call;
    // callers pass project_root through install/uninstall_targets for scoped removal.
    if let Ok(cwd) = std::env::current_dir() {
        let path = vscode_mcp_path(&cwd);
        if path.exists() {
            let mut config = read_json(&path);
            if config.get("servers").and_then(|v| v.get("ax")).is_some() {
                if let Some(servers) = config.get_mut("servers").and_then(|v| v.as_object_mut()) {
                    servers.remove("ax");
                    report.push_file(path.clone(), write_json_action(&path, &config)?);
                }
            }
        }
    }
    Ok(report)
}

/// Windsurf (Cascade) — global-only config at `~/.codeium/windsurf/mcp_config.json`,
/// same `mcpServers` shape as Cursor.
fn windsurf_config_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".codeium").join("windsurf"))
}

fn windsurf_mcp_path() -> Result<PathBuf, String> {
    Ok(windsurf_config_dir().ok_or("no home dir")?.join("mcp_config.json"))
}

fn install_windsurf_mcp(_project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("windsurf", display_name("windsurf"));
    let path = windsurf_mcp_path()?;
    report.push_file(path.clone(), upsert_mcp_servers(&path, "windsurf")?);
    report.note("Refresh the MCP server list in the Cascade panel (or restart Windsurf).");
    Ok(report)
}

fn uninstall_windsurf_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("windsurf", display_name("windsurf"));
    let path = windsurf_mcp_path()?;
    if let Some(action) = remove_mcp_servers(&path)? {
        report.push_file(path, action);
    }
    Ok(report)
}

/// Zed — user `settings.json` under `context_servers` (not `mcpServers`);
/// manual entries require `"source": "custom"`. No documented `cwd` field.
fn zed_settings_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .map(|a| PathBuf::from(a).join("Zed").join("settings.json"))
            .map_err(|_| "APPDATA not set".to_string())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let base = match std::env::var("XDG_CONFIG_HOME") {
            Ok(s) if !s.trim().is_empty() => PathBuf::from(s),
            _ => home_dir()?.join(".config"),
        };
        Ok(base.join("zed").join("settings.json"))
    }
}

fn zed_context_server_entry() -> Value {
    let path_token = mcp_path_token("zed");
    serde_json::json!({
        "source": "custom",
        "command": ax_bin(),
        "args": mcp_serve_args(path_token),
        "env": {},
    })
}

fn read_json_lenient(path: &Path) -> Value {
    if !path.exists() {
        return serde_json::json!({});
    }
    let content = fs::read_to_string(path).unwrap_or_default();
    if content.contains("//") || content.contains("/*") {
        serde_json::from_str(&strip_json_comments(&content)).unwrap_or_else(|_| read_json(path))
    } else {
        read_json(path)
    }
}

fn install_zed_mcp(_project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("zed", display_name("zed"));
    let path = zed_settings_path()?;
    let mut config = read_json_lenient(&path);
    if config.get("context_servers").is_none() {
        config["context_servers"] = serde_json::json!({});
    }
    config["context_servers"]["ax"] = zed_context_server_entry();
    report.push_file(path.clone(), write_json_action(&path, &config)?);
    report.note("Reopen the Agent Panel in Zed for the new context server to connect.");
    Ok(report)
}

fn uninstall_zed_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("zed", display_name("zed"));
    let path = zed_settings_path()?;
    if path.exists() {
        let mut config = read_json_lenient(&path);
        if config.get("context_servers").and_then(|v| v.get("ax")).is_some() {
            if let Some(servers) = config.get_mut("context_servers").and_then(|v| v.as_object_mut()) {
                servers.remove("ax");
                report.push_file(path.clone(), write_json_action(&path, &config)?);
            }
        }
    }
    Ok(report)
}

/// Strip `//` and `/* */` JSON comments (JSONC) without disturbing string content.
/// Ported from `ax-resolution::import_resolver::strip_json_comments`.
fn strip_json_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            out.push('"');
            i += 1;
            while i < bytes.len() {
                let c = bytes[i];
                out.push(c as char);
                if c == b'\\' && i + 1 < bytes.len() {
                    i += 1;
                    out.push(bytes[i] as char);
                } else if c == b'"' {
                    break;
                }
                i += 1;
            }
            i += 1;
        } else if bytes.get(i..i + 2) == Some(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
        } else if bytes.get(i..i + 2) == Some(b"/*") {
            i += 2;
            while i < bytes.len() && bytes.get(i..i + 2) != Some(b"*/") {
                i += 1;
            }
            i += 2;
        } else {
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

fn hermes_config_path() -> Result<PathBuf, String> {
    let home = match std::env::var("HERMES_HOME") {
        Ok(s) if !s.trim().is_empty() => PathBuf::from(s),
        _ => home_dir()?.join(".hermes"),
    };
    Ok(home.join("config.yaml"))
}

fn install_hermes_mcp(_project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("hermes", display_name("hermes"));
    let path = hermes_config_path()?;
    let content = if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };
    let out = replace_hermes_ax_block(&content, &hermes_ax_block(&ax_bin()));
    report.push_file(path.clone(), write_text_action(&path, &out)?);
    report.note("Start a new Hermes session for MCP changes to take effect.");
    Ok(report)
}

fn uninstall_hermes_mcp() -> Result<TargetReport, String> {
    let mut report = TargetReport::new("hermes", display_name("hermes"));
    let path = hermes_config_path()?;
    if !path.exists() || !fs::read_to_string(&path).unwrap_or_default().contains("  ax:") {
        return Ok(report);
    }
    let content = fs::read_to_string(&path).unwrap_or_default();
    let filtered: Vec<&str> = content
        .lines()
        .filter(|l| {
            !l.contains("mcp-ax")
                && l.trim() != "  ax:"
                && !l.trim().starts_with("command:")
                && !l.trim().starts_with("- serve")
                && !l.trim().starts_with("- --mcp")
                && !l.trim().starts_with("- --path")
                && l.trim() != "cwd: ."
                && l.trim() != "timeout: 120"
                && l.trim() != "connect_timeout: 60"
                && l.trim() != "enabled: true"
        })
        .collect();
    report.push_file(path.clone(), write_text_action(&path, &(filtered.join("\n") + "\n"))?);
    Ok(report)
}

#[cfg(test)]
mod mcp_path_tests {
    use super::*;

    #[test]
    fn global_targets_use_workspace_tokens_not_fixed_paths() {
        assert_eq!(mcp_path_token("cursor"), VS_WORKSPACE);
        assert_eq!(mcp_path_token("kiro"), VS_WORKSPACE);
        assert_eq!(mcp_path_token("gemini"), VS_WORKSPACE);
        assert_eq!(mcp_path_token("antigravity"), VS_WORKSPACE);
        assert_eq!(mcp_path_token("opencode"), VS_WORKSPACE);
        assert_eq!(mcp_path_token("claude"), CLAUDE_WORKSPACE);
        assert_eq!(mcp_path_token("codex"), PROCESS_CWD);
        assert_eq!(mcp_path_token("hermes"), PROCESS_CWD);
    }

    #[test]
    fn mcp_config_entry_never_embeds_install_cwd() {
        let entry = mcp_config_entry("cursor");
        let args = entry["args"].as_array().expect("args");
        assert_eq!(args[2], "--path");
        assert_eq!(args[3], VS_WORKSPACE);
        assert_eq!(entry["cwd"], VS_WORKSPACE);
        let serialized = serde_json::to_string(&entry).unwrap();
        assert!(!serialized.contains("Temp"));
        assert!(!serialized.contains("continue-smoke"));
    }

    #[test]
    fn codex_block_upserts_with_process_cwd() {
        let block = codex_ax_block("ax");
        assert!(block.contains(r#"args = ["serve", "--mcp", "--path", "."]"#));
        assert!(block.contains(r#"cwd = ".""#));
        let merged = replace_toml_section(
            "[other]\nkey = 1\n\n[mcp_servers.ax]\ncommand = \"old\"\n",
            "[mcp_servers.ax]",
            &block,
        );
        assert!(merged.contains("command = \"ax\""));
        assert!(!merged.contains("command = \"old\""));
    }

    #[test]
    fn hermes_block_includes_path_arg() {
        let block = hermes_ax_block("ax");
        assert!(block.contains("- --path"));
        assert!(block.contains("cwd: ."));
    }

    #[test]
    fn new_ide_targets_default_to_workspace_token() {
        assert_eq!(mcp_path_token("vscode"), VS_WORKSPACE);
        assert_eq!(mcp_path_token("windsurf"), VS_WORKSPACE);
        assert_eq!(mcp_path_token("zed"), VS_WORKSPACE);
    }

    #[test]
    fn vscode_entry_uses_servers_shape_with_type_stdio() {
        let entry = vscode_mcp_entry();
        assert_eq!(entry["type"], "stdio");
        assert!(entry.get("mcpServers").is_none());
    }

    #[test]
    fn zed_entry_requires_source_custom() {
        let entry = zed_context_server_entry();
        assert_eq!(entry["source"], "custom");
        assert!(entry.get("cwd").is_none(), "Zed context_servers has no documented cwd field");
    }

    fn temp_settings_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ax-installer-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("mkdir");
        dir.join("settings.json")
    }

    #[test]
    fn claude_stop_hook_install_is_idempotent_and_removable() {
        let path = temp_settings_path("stop-hook");

        let first = install_claude_hook(&path, "Stop", "stop-hook").unwrap();
        assert!(matches!(first, Some((_, FileAction::Created))));
        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("stop-hook"));
        assert!(content.contains("\"Stop\""));

        // Second install call must be a no-op, not a duplicate entry.
        let second = install_claude_hook(&path, "Stop", "stop-hook").unwrap();
        assert!(matches!(second, Some((_, FileAction::Unchanged))));
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["hooks"]["Stop"].as_array().unwrap().len(), 1);

        let removed = remove_claude_hook(&path, "Stop", "stop-hook").unwrap();
        assert!(removed);
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["hooks"]["Stop"].as_array().unwrap().len(), 0);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn claude_hooks_for_different_events_do_not_collide() {
        let path = temp_settings_path("multi-hook");
        install_claude_hook(&path, "UserPromptSubmit", "prompt-hook").unwrap();
        install_claude_hook(&path, "Stop", "stop-hook").unwrap();
        install_claude_hook(&path, "SubagentStop", "stop-hook").unwrap();

        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 1);
        assert_eq!(value["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(value["hooks"]["SubagentStop"].as_array().unwrap().len(), 1);

        // Removing the prompt hook must not disturb the Stop/SubagentStop hooks.
        remove_claude_hook(&path, "UserPromptSubmit", "prompt-hook").unwrap();
        let value: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(value["hooks"]["UserPromptSubmit"].as_array().unwrap().len(), 0);
        assert_eq!(value["hooks"]["Stop"].as_array().unwrap().len(), 1);

        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
