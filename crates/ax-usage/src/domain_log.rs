//! Domain-tagged verbose log lines for Logging / Quality (v4+).
//!
//! Lines look like: `[ax] 🪓 plugin extract name=echo ok tool=plugin`
//! so the Logging page `classify()` can map prefixes to TraceKinds and
//! the TOOL column can show a stable label.
//! Writes are gated by `append_verbose_log` (verbose MCP must be on).

use std::path::Path;

use crate::log_brand::format_ax_tagged;
use crate::mcp_verbose_log::append_verbose_log;

/// Append one domain event to the project verbose log (no-op if root missing / verbose off).
pub fn log_domain_event(project_root: Option<&Path>, domain: &str, message: impl AsRef<str>) {
    let msg = message.as_ref();
    let tool = domain_tool_label(domain, msg);
    let line = if msg.contains("tool=") {
        format_ax_tagged(format!("{domain} {msg}"))
    } else {
        format_ax_tagged(format!("{domain} {msg} tool={tool}"))
    };
    append_verbose_log(&[line], project_root);
}

/// `tool=` value for Logging: `cli:<cmd>` when `cmd=` is present, else the domain.
fn domain_tool_label(domain: &str, message: &str) -> String {
    if domain == "cli" {
        if let Some(cmd) = message
            .split_whitespace()
            .find_map(|tok| tok.strip_prefix("cmd="))
        {
            let cmd = cmd.trim_matches(|c| c == '"' || c == '\'');
            if !cmd.is_empty() {
                return format!("cli:{cmd}");
            }
        }
    }
    domain.to_string()
}

pub fn log_plugin(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "plugin", message);
}

pub fn log_lsp(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "lsp", message);
}

pub fn log_ship_ci(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "ship-ci", message);
}

pub fn log_ship(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "ship", message);
}

pub fn log_share(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "share", message);
}

pub fn log_workspace(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "workspace", message);
}

pub fn log_embed(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "embed", message);
}

pub fn log_action(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "action", message);
}

pub fn log_memory(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "memory", message);
}

pub fn log_policy(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "policy", message);
}

/// Generic CLI readout (`explore`, `impact`, `status`, …).
pub fn log_cli(project_root: Option<&Path>, message: impl AsRef<str>) {
    log_domain_event(project_root, "cli", message);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_tool_label_cli_cmd() {
        assert_eq!(
            domain_tool_label("cli", "cmd=explore start q_len=3"),
            "cli:explore"
        );
        assert_eq!(domain_tool_label("memory", "remember ok"), "memory");
        assert_eq!(domain_tool_label("ship-ci", "status=passed"), "ship-ci");
    }
}
