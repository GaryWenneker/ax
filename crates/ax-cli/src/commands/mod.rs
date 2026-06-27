//! CLI command implementations.

pub mod offload;
pub mod telemetry;
pub mod upgrade;
pub mod affected;
pub mod callers;
pub mod callees;
pub mod context;
pub mod explore;
pub mod files;
pub mod impact;
pub mod index;
pub mod init;
pub mod install;
pub mod node;
pub mod query;
pub mod status;
pub mod sync;
pub mod uninit;
pub mod uninstall;
pub mod daemon;
pub mod prompt_hook;
pub mod unlock;

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
