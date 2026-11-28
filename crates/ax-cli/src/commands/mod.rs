//! CLI command implementations.

pub mod affected;
pub mod agent;
pub mod api;
pub(crate) mod ax_process;
pub mod bootstrap;
pub mod budget_cmd;
pub mod cache_cmd;
pub mod callees;
pub mod callers;
pub mod context;
pub mod costs;
pub mod cursor;
pub mod cycles;
pub mod daemon;
pub mod db;
pub mod desktop;
pub mod diff;
pub mod docs_catalog;
pub mod doctor;
pub mod durable_cmd;
pub mod explore;
pub mod export;
pub mod export_concepts;
pub mod export_okf;
pub mod files;
pub mod git_status;
pub mod global;
pub mod impact;
pub mod index;
pub mod init;
pub mod insights;
pub mod install;
pub mod lsp;
pub mod mcp;
pub mod memory;
pub mod neural;
pub mod node;
pub mod offload;
pub mod path;
pub mod plugins_cmd;
pub mod policy;
pub mod policy_pull;
pub mod policy_share;
pub mod preflight;
pub mod pricing;
pub mod prompt_hook;
pub mod query;
pub mod read_guard;
pub mod replay;
pub mod report;
pub mod savings;
pub mod session_cmd;
pub mod session_hook;
pub mod share;
pub mod ship;
pub mod skill_catalog;
pub mod spec;
pub mod status;
pub mod stop_hook;
pub mod sync;
pub mod telemetry;
pub mod test_impact;
pub mod turn_cmd;
pub mod turn_hook;
pub mod uninit;
pub mod uninstall;
pub mod unlock;
pub mod upgrade;
pub mod validate;
pub mod web;

use std::path::{Path, PathBuf};

/// Error for a command that the headless build (`--no-default-features`, see `slim/`) leaves out.
#[cfg(any(not(feature = "web"), not(feature = "desktop")))]
pub fn unavailable_in_headless_build(command: &str) -> String {
    format!("`{command}` is not included in this headless ax build; use the full ax distribution")
}

/// The directory a command that creates, indexes or removes a project acts on: `path` as
/// given, else the current directory. `ax init`, `install`, `uninit`, `index`, `sync`,
/// `ship` and `bootstrap` use it, so they never reach up into a parent project.
pub fn resolve_target_dir(path: Option<String>) -> PathBuf {
    path.map(PathBuf::from).unwrap_or_else(current_dir)
}

/// The project a command works on: `path` as given; otherwise the nearest ancestor of the
/// current directory that holds an initialized project (`.ax/ax.db`), never `$HOME` (whose
/// `.ax` is the global ax folder). With no such ancestor it is the current directory, so
/// the command reports "project not initialized" as before. Every command that reads or
/// updates an existing project and every hook use this one rule, which matches the MCP
/// server.
pub fn resolve_project_root(path: Option<String>) -> PathBuf {
    match path {
        Some(path) => PathBuf::from(path),
        None => {
            let cwd = current_dir();
            find_project_root(&cwd).unwrap_or(cwd)
        }
    }
}

/// The nearest directory at or above `start` with an initialized project, skipping `$HOME`.
/// The result is canonical when `start` exists.
pub fn find_project_root(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().unwrap_or_else(|_| start.to_path_buf());
    let home = ax_utils::paths::home_dir();
    let home_canonical = home.as_deref().and_then(|h| h.canonicalize().ok());
    start
        .ancestors()
        .filter(|dir| Some(*dir) != home.as_deref() && Some(*dir) != home_canonical.as_deref())
        .find(|dir| ax_context::directory::is_initialized(dir))
        .map(Path::to_path_buf)
}

fn current_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
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

pub(crate) fn stash_cli_graph(tool: &str, value: &impl serde::Serialize, text: &str) {
    let structured = serde_json::to_value(value).unwrap_or_else(|_| serde_json::json!({}));
    ax_usage::stash_cli_result(tool, &structured, text);
}
