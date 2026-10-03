//! CLI command implementations.

pub mod affected;
pub mod agent;
pub mod api;
pub mod bootstrap;
pub mod budget_cmd;
pub mod callees;
pub mod callers;
pub mod context;
pub mod costs;
pub mod cursor;
pub mod cycles;
pub mod daemon;
pub mod desktop;
pub mod diff;
pub mod docs_catalog;
pub mod explore;
pub mod export;
pub mod export_concepts;
pub mod export_okf;
pub mod files;
pub mod global;
pub mod impact;
pub mod index;
pub mod init;
pub mod insights;
pub mod install;
pub mod lsp;
pub mod mcp;
pub mod memory;
pub mod node;
pub mod offload;
pub mod path;
pub mod policy;
pub mod policy_share;
pub mod pricing;
pub mod prompt_hook;
pub mod query;
pub mod read_guard;
pub mod report;
pub mod savings;
pub mod session_hook;
pub mod share;
pub mod ship;
pub mod status;
pub mod stop_hook;
pub mod sync;
pub mod telemetry;
pub mod test_impact;
pub mod turn_hook;
pub mod uninit;
pub mod uninstall;
pub mod unlock;
pub mod upgrade;
pub mod validate;
pub mod web;

use std::path::{Path, PathBuf};

pub fn resolve_path(path: Option<String>) -> PathBuf {
    path.map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

/// Git hooks call `--quiet` commands in every checkout; one that was never initialized
/// (a fresh worktree) is not an error for them, and must not get a `.ax/` either.
pub fn quiet_and_uninitialized(root: &Path, quiet: bool) -> bool {
    quiet && !ax_context::directory::is_initialized(root)
}

pub fn check_unsafe_root(path: &Path) -> Result<(), String> {
    if let Some(reason) = ax_context::unsafe_index_root_reason(path) {
        return Err(reason);
    }
    Ok(())
}
