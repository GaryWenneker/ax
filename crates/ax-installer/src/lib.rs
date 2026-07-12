//! Shared AI agent MCP installer — used by ax-cli and ax-web.

pub mod cli_catalog;
pub mod cli_install;
pub mod report;
pub mod targets;

use std::path::Path;

pub use cli_catalog::{
    auth_command, build_child_env, catalog, catalog_entry, detect_cli_available,
    headless_command, resolve_cli_spawn, CommandSpec, CliSpawn,
};
pub use cli_install::{
    cli_bin_name, cli_install_plan, cli_installable, ensure_agent_ready, install_cli,
    install_cli_targets, is_cli_on_path, resolve_cli_path, CliInstallOutcome,
    CliInstallPlan,
};
pub use cli_catalog::CliInstallMethod;
pub use report::{FileAction, InstallSummary, TargetReport};
pub use targets::{
    agent_status, catalog_with_status, display_name, install_targets, is_detected,
    uninstall_all, uninstall_targets, AgentTargetStatus, TARGETS,
};

/// Ensure `~/.ax/config.json` exists with index/policy scaffolds.
pub fn ensure_global_config() {
    let Some(home) = dirs::home_dir() else {
        return;
    };
    let ax_dir = home.join(".ax");
    if std::fs::create_dir_all(&ax_dir).is_err() {
        return;
    }
    let path = ax_dir.join("config.json");
    let mut root: serde_json::Value = match std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
    {
        Some(v) if v.is_object() => v,
        _ => serde_json::json!({}),
    };

    let mut changed = false;
    if root.get("index").is_none() {
        root["index"] = serde_json::json!({
            "extensions": {},
            "exclude": [],
            "includeIgnored": []
        });
        changed = true;
    }
    if root.get("policy").is_none() {
        root["policy"] = serde_json::json!({
            "storage": "files"
        });
        changed = true;
    }
    if changed {
        if let Ok(json) = serde_json::to_string_pretty(&root) {
            let _ = std::fs::write(&path, json + "\n");
        }
    }
}

pub fn install_detected(project_root: &Path, install_all: bool) -> Result<InstallSummary, String> {
    targets::install_detected(project_root, install_all)
}

pub fn install_selected(
    project_root: &Path,
    selected: &[String],
) -> Result<InstallSummary, String> {
    targets::install_targets(project_root, selected)
}
