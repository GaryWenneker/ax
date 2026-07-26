//! Domain-tagged verbose log lines for Logging / Quality (v4+).
//!
//! Lines look like: `[ax] plugin extract name=echo ok`
//! so the Logging page `classify()` can map prefixes to TraceKinds.

use std::path::Path;

use crate::mcp_verbose_log::append_verbose_log;

/// Append one domain event to the project verbose log (no-op if root missing).
pub fn log_domain_event(project_root: Option<&Path>, domain: &str, message: impl AsRef<str>) {
    let line = format!("[ax] {domain} {}", message.as_ref());
    append_verbose_log(&[line], project_root);
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
