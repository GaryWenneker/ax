//! Agent target installers — CG: installer/targets/*.ts

use std::fs;
use std::path::{Path, PathBuf};

pub const TARGETS: &[&str] = &[
    "claude", "cursor", "codex", "opencode", "hermes", "gemini", "antigravity", "kiro",
];

pub fn install_all_detected(project_root: &Path) -> Result<(), String> {
    for target in TARGETS {
        install_target(target, project_root)?;
    }
    Ok(())
}

pub fn uninstall_all() -> Result<(), String> {
    for target in TARGETS {
        uninstall_target(target)?;
    }
    Ok(())
}

fn install_target(target: &str, project_root: &Path) -> Result<(), String> {
    match target {
        "cursor" => install_cursor_mcp(project_root),
        "claude" => install_claude_mcp(project_root),
        "codex" => install_codex_mcp(project_root),
        "opencode" => install_opencode_mcp(project_root),
        "hermes" => install_hermes_mcp(project_root),
        "gemini" => install_gemini_mcp(project_root),
        "antigravity" => install_antigravity_mcp(project_root),
        "kiro" => install_kiro_mcp(project_root),
        _ => Ok(()),
    }
}

fn uninstall_target(target: &str) -> Result<(), String> {
    match target {
        "cursor" => uninstall_cursor_mcp(),
        "claude" => uninstall_claude_mcp(),
        "codex" => uninstall_codex_mcp(),
        "opencode" => uninstall_opencode_mcp(),
        "hermes" => uninstall_hermes_mcp(),
        "gemini" => uninstall_gemini_mcp(),
        "antigravity" => uninstall_antigravity_mcp(),
        "kiro" => uninstall_kiro_mcp(),
        _ => Ok(()),
    }
}

fn ax_bin() -> String {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "ax".to_string())
}

fn mcp_config_entry(project_root: &Path) -> serde_json::Value {
    serde_json::json!({
        "command": ax_bin(),
        "args": ["serve", "--mcp"],
        "cwd": project_root.to_string_lossy(),
    })
}

fn antigravity_mcp_entry(project_root: &Path) -> serde_json::Value {
    serde_json::json!({
        "command": ax_bin(),
        "args": ["serve", "--mcp"],
        "cwd": project_root.to_string_lossy(),
    })
}

fn home_dir() -> Result<PathBuf, String> {
    dirs::home_dir().ok_or_else(|| "no home dir".to_string())
}

fn read_json(path: &Path) -> serde_json::Value {
    if path.exists() {
        let content = fs::read_to_string(path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    }
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(path, serde_json::to_string_pretty(value).unwrap_or_default()).map_err(|e| e.to_string())?;
    Ok(())
}

fn upsert_mcp_servers(path: &Path, project_root: &Path) -> Result<(), String> {
    let mut config = read_json(path);
    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }
    config["mcpServers"]["ax"] = mcp_config_entry(project_root);
    write_json(path, &config)
}

fn remove_mcp_servers(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let mut config = read_json(path);
    if let Some(servers) = config.get_mut("mcpServers").and_then(|v| v.as_object_mut()) {
        servers.remove("ax");
    }
    write_json(path, &config)
}

fn install_cursor_mcp(project_root: &Path) -> Result<(), String> {
    let path = home_dir()?.join(".cursor").join("mcp.json");
    upsert_mcp_servers(&path, project_root)
}

fn uninstall_cursor_mcp() -> Result<(), String> {
    let path = home_dir()?.join(".cursor").join("mcp.json");
    remove_mcp_servers(&path)
}

fn install_claude_mcp(project_root: &Path) -> Result<(), String> {
    let home = home_dir()?;
    upsert_mcp_servers(&home.join(".claude.json"), project_root)?;
    // Project-local MCP file Claude Code actually reads (CG #207).
    upsert_mcp_servers(&project_root.join(".mcp.json"), project_root)?;
    install_claude_prompt_hook(&home.join(".claude").join("settings.json"))?;
    Ok(())
}

fn uninstall_claude_mcp() -> Result<(), String> {
    let home = home_dir()?;
    remove_mcp_servers(&home.join(".claude.json"))?;
    remove_claude_prompt_hook(&home.join(".claude").join("settings.json"))?;
    Ok(())
}

fn install_claude_prompt_hook(settings_path: &Path) -> Result<(), String> {
    let bin = ax_bin();
    let hook_cmd = format!("{} prompt-hook", bin);
    let mut settings = read_json(settings_path);
    if settings.get("hooks").is_none() {
        settings["hooks"] = serde_json::json!({});
    }
    let hooks = settings["hooks"].as_object_mut().ok_or("invalid hooks")?;
    if hooks.get("UserPromptSubmit").is_none() {
        hooks.insert(
            "UserPromptSubmit".to_string(),
            serde_json::json!([]),
        );
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
        return Ok(());
    }
    groups.push(serde_json::json!({
        "hooks": [{ "type": "command", "command": hook_cmd }]
    }));
    if let Some(parent) = settings_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_json(settings_path, &settings)
}

fn remove_claude_prompt_hook(settings_path: &Path) -> Result<(), String> {
    if !settings_path.exists() {
        return Ok(());
    }
    let mut settings = read_json(settings_path);
    let hooks = settings
        .get_mut("hooks")
        .and_then(|v| v.as_object_mut());
    if hooks.is_none() {
        return Ok(());
    }
    let groups = hooks
        .unwrap()
        .get_mut("UserPromptSubmit")
        .and_then(|v| v.as_array_mut());
    if groups.is_none() {
        return Ok(());
    }
    groups.unwrap().retain(|g| {
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
    write_json(settings_path, &settings)
}

fn install_codex_mcp(project_root: &Path) -> Result<(), String> {
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
        return Ok(());
    }
    let mut out = content;
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&block);
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(())
}

fn uninstall_codex_mcp() -> Result<(), String> {
    let path = home_dir()?.join(".codex").join("config.toml");
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&path).unwrap_or_default();
    if !content.contains("[mcp_servers.ax]") {
        return Ok(());
    }
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
    fs::write(&path, out.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok(())
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

fn install_opencode_mcp(project_root: &Path) -> Result<(), String> {
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
    write_json(&path, &config)?;
    // Legacy Windows path cleanup (#535)
    if let Ok(app_data) = std::env::var("APPDATA") {
        let legacy = PathBuf::from(app_data).join("opencode").join("opencode.jsonc");
        if legacy.exists() && legacy != path {
            let mut legacy_cfg = read_json(&legacy);
            if let Some(mcp) = legacy_cfg.get_mut("mcp").and_then(|v| v.as_object_mut()) {
                mcp.remove("ax");
                write_json(&legacy, &legacy_cfg)?;
            }
        }
    }
    Ok(())
}

fn uninstall_opencode_mcp() -> Result<(), String> {
    let path = opencode_config_path()?;
    if path.exists() {
        let mut config = read_json(&path);
        if let Some(mcp) = config.get_mut("mcp").and_then(|v| v.as_object_mut()) {
            mcp.remove("ax");
        }
        write_json(&path, &config)?;
    }
    Ok(())
}

fn install_gemini_mcp(project_root: &Path) -> Result<(), String> {
    let dir = home_dir()?.join(".gemini");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("settings.json");
    upsert_mcp_servers(&path, project_root)
}

fn uninstall_gemini_mcp() -> Result<(), String> {
    let path = home_dir()?.join(".gemini").join("settings.json");
    remove_mcp_servers(&path)
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

fn install_antigravity_mcp(project_root: &Path) -> Result<(), String> {
    let path = antigravity_mcp_path()?;
    let mut config = read_json(&path);
    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }
    config["mcpServers"]["ax"] = antigravity_mcp_entry(project_root);
    write_json(&path, &config)
}

fn uninstall_antigravity_mcp() -> Result<(), String> {
    for path in [
        home_dir()?.join(".gemini").join("config").join("mcp_config.json"),
        home_dir()?.join(".gemini").join("antigravity").join("mcp_config.json"),
    ] {
        remove_mcp_servers(&path)?;
    }
    Ok(())
}

fn install_kiro_mcp(project_root: &Path) -> Result<(), String> {
    let path = home_dir()?.join(".kiro").join("settings").join("mcp.json");
    upsert_mcp_servers(&path, project_root)
}

fn uninstall_kiro_mcp() -> Result<(), String> {
    let path = home_dir()?.join(".kiro").join("settings").join("mcp.json");
    remove_mcp_servers(&path)
}

fn hermes_config_path() -> Result<PathBuf, String> {
    let home = match std::env::var("HERMES_HOME") {
        Ok(s) if !s.trim().is_empty() => PathBuf::from(s),
        _ => home_dir()?.join(".hermes"),
    };
    Ok(home.join("config.yaml"))
}

fn install_hermes_mcp(_project_root: &Path) -> Result<(), String> {
    let path = hermes_config_path()?;
    let bin = ax_bin();
    let block = format!(
        "mcp_servers:\n  ax:\n    command: {bin}\n    args:\n      - serve\n      - --mcp\n    timeout: 120\n    connect_timeout: 60\n    enabled: true\nplatform_toolsets:\n  cli:\n    - mcp-ax\n"
    );
    let content = if path.exists() {
        fs::read_to_string(&path).unwrap_or_default()
    } else {
        String::new()
    };
    if content.contains("  ax:") && content.contains("mcp_servers:") {
        return Ok(());
    }
    let mut out = content;
    if !out.ends_with('\n') && !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&block);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&path, out).map_err(|e| e.to_string())?;
    Ok(())
}

fn uninstall_hermes_mcp() -> Result<(), String> {
    let path = hermes_config_path()?;
    if !path.exists() {
        return Ok(());
    }
    let content = fs::read_to_string(&path).unwrap_or_default();
    if !content.contains("  ax:") {
        return Ok(());
    }
    let filtered: Vec<&str> = content
        .lines()
        .filter(|l| !l.contains("mcp-ax") && l.trim() != "  ax:" && !l.trim().starts_with("command:") && !l.trim().starts_with("- serve") && !l.trim().starts_with("- --mcp") && l.trim() != "timeout: 120" && l.trim() != "connect_timeout: 60" && l.trim() != "enabled: true")
        .collect();
    fs::write(&path, filtered.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok(())
}