//! Shared AI agent MCP installer — used by ax-cli and ax-web.

pub mod ax_command;
pub mod cli_catalog;
pub mod cli_install;
pub mod cli_only;
pub mod command_center;
pub mod detect;
pub mod hooks;
pub mod ide_choice;
pub mod report;
pub mod targets;

use std::path::Path;

pub use cli_catalog::CliInstallMethod;
pub use cli_catalog::{
    auth_command, build_child_env, catalog, catalog_entry, detect_cli_available, headless_command,
    resolve_cli_spawn, CliSpawn, CommandSpec,
};
pub use cli_install::{
    cli_bin_name, cli_install_plan, cli_installable, ensure_agent_ready, install_cli,
    install_cli_targets, is_cli_on_path, resolve_cli_path, CliInstallOutcome, CliInstallPlan,
};
pub use cli_only::install_cli_only;
pub use ide_choice::{
    ide_defaults, ide_groups, ides_to_remove, known_ides, pack_refresh_targets, parse_ide_choice,
};
pub use report::{FileAction, InstallSummary, TargetReport};
pub use targets::{
    agent_status, catalog_with_status, display_name, install_targets, is_detected, uninstall_all,
    uninstall_targets, AgentTargetStatus, TARGETS,
};

/// Ensure `~/.ax/config.json` exists with index/policy scaffolds. A config
/// that does not parse is left untouched and named in a warning.
pub fn ensure_global_config() {
    let Some(home) = ax_utils::paths::home_dir() else {
        return;
    };
    let ax_dir = home.join(".ax");
    if std::fs::create_dir_all(&ax_dir).is_err() {
        return;
    }
    if let Err(error) = ensure_global_config_at(&ax_dir.join("config.json")) {
        eprintln!("warning: {error}");
    }
}

/// Adds the missing scaffolds; returns whether the file changed. Only a
/// missing or empty file starts from `{}`: unparsable JSON or a non-object
/// root is an error and the file is not written.
fn ensure_global_config_at(path: &Path) -> Result<bool, String> {
    use std::io;
    ax_utils::update_locked(path, |current| {
        let mut root = match current {
            Some(bytes) if !bytes.iter().all(u8::is_ascii_whitespace) => {
                match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(value) if value.is_object() => value,
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!("{} is not a JSON object; left unchanged", path.display()),
                        ))
                    }
                    Err(error) => {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidData,
                            format!(
                                "{} is not valid JSON ({error}); left unchanged",
                                path.display()
                            ),
                        ))
                    }
                }
            }
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
        if !changed {
            return Ok(None);
        }
        let text = serde_json::to_string_pretty(&root).map_err(io::Error::other)? + "\n";
        Ok(Some(text.into_bytes()))
    })
    .map_err(|e| e.to_string())
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

#[cfg(test)]
mod global_config_tests {
    use super::*;

    #[test]
    fn a_missing_config_gets_the_scaffolds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        assert!(ensure_global_config_at(&path).unwrap());
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["policy"]["storage"], "files");
        assert!(root["index"]["exclude"].is_array());
        assert!(!ensure_global_config_at(&path).unwrap());
    }

    #[test]
    fn existing_keys_are_kept_when_scaffolds_are_added() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(&path, r#"{"share":{"microsoftClientId":"abc"}}"#).unwrap();
        assert!(ensure_global_config_at(&path).unwrap());
        let root: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["share"]["microsoftClientId"], "abc");
        assert_eq!(root["policy"]["storage"], "files");
    }

    #[test]
    fn an_unparsable_config_is_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        for broken in ["{\"index\": {\"exclude\": [\"target\"],},}\n", "[1, 2]\n"] {
            std::fs::write(&path, broken).unwrap();
            let error = ensure_global_config_at(&path).unwrap_err();
            assert!(error.contains("config.json"), "{error}");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
        }
    }
}
