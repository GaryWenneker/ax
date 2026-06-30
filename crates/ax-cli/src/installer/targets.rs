//! Agent target installers — CG: installer/targets/*.ts

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::report::{FileAction, InstallSummary, TargetReport};

pub const TARGETS: &[&str] = &[
    "claude", "cursor", "codex", "opencode", "hermes", "gemini", "antigravity", "kiro",
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
        _ => "Unknown",
    }
}

pub fn is_detected(target: &str) -> bool {
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
        _ => return Ok(None),
    };
    Ok(Some(report))
}

fn ax_bin() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "ax".to_string())
}

fn mcp_config_entry(project_root: &Path) -> Value {
    serde_json::json!({
        "command": ax_bin(),
        "args": ["serve", "--mcp"],
        "cwd": project_root.to_string_lossy(),
    })
}

fn antigravity_mcp_entry(project_root: &Path) -> Value {
    mcp_config_entry(project_root)
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

fn upsert_mcp_servers(path: &Path, project_root: &Path) -> Result<FileAction, String> {
    let mut config = read_json(path);
    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }
    config["mcpServers"]["ax"] = mcp_config_entry(project_root);
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

fn install_cursor_mcp(project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("cursor", display_name("cursor"));
    let path = home_dir()?.join(".cursor").join("mcp.json");
    let action = upsert_mcp_servers(&path, project_root)?;
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
    report.push_file(global.clone(), upsert_mcp_servers(&global, project_root)?);
    let local = project_root.join(".mcp.json");
    report.push_file(local.clone(), upsert_mcp_servers(&local, project_root)?);
    let settings = home.join(".claude").join("settings.json");
    match install_claude_prompt_hook(&settings)? {
        Some((path, action)) => report.push_file(path, action),
        None => {}
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
    if remove_claude_prompt_hook(&settings)? {
        report.push_file(settings, FileAction::Updated);
    }
    Ok(report)
}

fn install_claude_prompt_hook(settings_path: &Path) -> Result<Option<(PathBuf, FileAction)>, String> {
    let bin = ax_bin();
    let hook_cmd = format!("{} prompt-hook", bin);
    let mut settings = read_json(settings_path);
    if settings.get("hooks").is_none() {
        settings["hooks"] = serde_json::json!({});
    }
    let hooks = settings["hooks"].as_object_mut().ok_or("invalid hooks")?;
    if hooks.get("UserPromptSubmit").is_none() {
        hooks.insert("UserPromptSubmit".to_string(), serde_json::json!([]));
    }
    let groups = hooks
        .get_mut("UserPromptSubmit")
        .and_then(|v| v.as_array_mut())
        .ok_or("invalid UserPromptSubmit")?;
    let already = groups.iter().any(|g| {
        g.get("hooks")
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter().any(|e| {
                    e.get("command")
                        .and_then(|c| c.as_str())
                        .map(|s| s.contains("prompt-hook"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    });
    if already {
        return Ok(Some((settings_path.to_path_buf(), FileAction::Unchanged)));
    }
    groups.push(serde_json::json!({
        "hooks": [{ "type": "command", "command": hook_cmd }]
    }));
    let action = write_json_action(settings_path, &settings)?;
    Ok(Some((settings_path.to_path_buf(), action)))
}

fn remove_claude_prompt_hook(settings_path: &Path) -> Result<bool, String> {
    if !settings_path.exists() {
        return Ok(false);
    }
    let mut settings = read_json(settings_path);
    let hooks = settings.get_mut("hooks").and_then(|v| v.as_object_mut());
    if hooks.is_none() {
        return Ok(false);
    }
    let groups = hooks
        .unwrap()
        .get_mut("UserPromptSubmit")
        .and_then(|v| v.as_array_mut());
    if groups.is_none() {
        return Ok(false);
    }
    let groups = groups.unwrap();
    let before = groups.len();
    groups.retain(|g| {
        !g.get("hooks")
            .and_then(|h| h.as_array())
            .map(|arr| {
                arr.iter().any(|e| {
                    e.get("command")
                        .and_then(|c| c.as_str())
                        .map(|s| s.contains("prompt-hook"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    });
    if groups.len() == before {
        return Ok(false);
    }
    write_json_action(settings_path, &settings)?;
    Ok(true)
}

fn install_codex_mcp(project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("codex", display_name("codex"));
    let dir = home_dir()?.join(".codex");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("config.toml");
    let bin = ax_bin();
    let block = format!(
        "[mcp_servers.ax]\ncommand = \"{bin}\"\nargs = [\"serve\", \"--mcp\"]\ncwd = \"{cwd}\"\n",
        cwd = project_root.to_string_lossy().replace('\\', "/")
    );
    let content = if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };
    if content.contains("[mcp_servers.ax]") {
        report.push_file(path, FileAction::Unchanged);
        return Ok(report);
    }
    let mut out = content;
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&block);
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

fn install_opencode_mcp(project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("opencode", display_name("opencode"));
    let path = opencode_config_path()?;
    let bin = ax_bin();
    let mut config = read_json(&path);
    if config.get("mcp").is_none() {
        config["mcp"] = serde_json::json!({});
    }
    config["mcp"]["ax"] = serde_json::json!({
        "type": "local",
        "command": [bin, "serve", "--mcp"],
        "enabled": true,
        "cwd": project_root.to_string_lossy(),
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

fn install_gemini_mcp(project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("gemini", display_name("gemini"));
    let dir = home_dir()?.join(".gemini");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("settings.json");
    report.push_file(path.clone(), upsert_mcp_servers(&path, project_root)?);
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

fn install_antigravity_mcp(project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("antigravity", display_name("antigravity"));
    let path = antigravity_mcp_path()?;
    let mut config = read_json(&path);
    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }
    config["mcpServers"]["ax"] = antigravity_mcp_entry(project_root);
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

fn install_kiro_mcp(project_root: &Path) -> Result<TargetReport, String> {
    let mut report = TargetReport::new("kiro", display_name("kiro"));
    let path = home_dir()?.join(".kiro").join("settings").join("mcp.json");
    report.push_file(path.clone(), upsert_mcp_servers(&path, project_root)?);
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
    let bin = ax_bin();
    let block = "mcp_servers:\n  ax:\n    command: {bin}\n    args:\n      - serve\n      - --mcp\n    timeout: 120\n    connect_timeout: 60\n    enabled: true\nplatform_toolsets:\n  cli:\n    - mcp-ax\n"
        .replace("{bin}", &bin);
    let content = if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };
    if content.contains("  ax:") && content.contains("mcp_servers:") {
        report.push_file(path, FileAction::Unchanged);
        return Ok(report);
    }
    let mut out = content;
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&block);
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
                && l.trim() != "timeout: 120"
                && l.trim() != "connect_timeout: 60"
                && l.trim() != "enabled: true"
        })
        .collect();
    report.push_file(path.clone(), write_text_action(&path, &(filtered.join("\n") + "\n"))?);
    Ok(report)
}
