//! CLI command implementations.

pub mod offload;
pub mod telemetry;
pub mod savings;
pub mod upgrade;
pub mod affected;
pub mod diff;
pub mod export;
pub mod test_impact;
pub mod ship;
pub mod callers;
pub mod callees;
pub mod context;
pub mod cursor;
pub mod explore;
pub mod files;
pub mod impact;
pub mod insights;
pub mod index;
pub mod report;
pub mod init;
pub mod install;
pub mod memory;
pub mod mcp;
pub mod node;
pub mod query;
pub mod status;
pub mod sync;
pub mod uninit;
pub mod uninstall;
pub mod daemon;
pub mod policy;
pub mod prompt_hook;
pub mod session_hook;
pub mod stop_hook;
pub mod unlock;
pub mod web;

use std::path::{Path, PathBuf};

pub fn resolve_path(path: Option<String>) -> PathBuf {
    path.map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

pub fn check_unsafe_root(path: &Path) -> Result<(), String> {
    if let Some(reason) = ax_context::unsafe_index_root_reason(path) {
        return Err(reason);
    }
    Ok(())
}
