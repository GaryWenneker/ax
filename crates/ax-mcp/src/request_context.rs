//! Validate explicit ownership before session resolution, indexing or state access.
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn validate(root: &Path, args: &Value) -> Result<(), String> {
    if !args.is_null() && !args.is_object() {
        return Err("tool arguments must be an object".into());
    }
    for key in ["projectPath", "project_path"] {
        if let Some(raw) = args.get(key) {
            let path = raw
                .as_str()
                .filter(|p| !p.trim().is_empty())
                .ok_or_else(|| format!("{key} must be a nonempty path"))?;
            let candidate = ax_context::directory::find_nearest_ax_root(&PathBuf::from(path))
                .ok_or_else(|| format!("{key} does not identify an initialized project; connect MCP to the intended project"))?;
            if ax_usage::project_scope(&candidate) != ax_usage::project_scope(root) {
                return Err(format!("{key} differs from this MCP server's project; connect a separate server to that project"));
            }
        }
    }
    for key in ["session", "context_epoch", "window_id"] {
        if let Some(raw) = args.get(key) {
            let value = raw
                .as_str()
                .ok_or_else(|| format!("{key} must be a string"))?;
            if ax_usage::session_from_args(&serde_json::json!({"session":value})).is_none() {
                return Err(format!(
                    "{key} must contain 1..128 ASCII letters, digits or -_.:"
                ));
            }
        }
    }
    if args.get("context_reset").is_some_and(|v| !v.is_boolean()) {
        return Err("context_reset must be a boolean".into());
    }
    Ok(())
}

/// Private transport fields are server-owned; public clients cannot forge delivery state.
pub fn public_args(mut args: Value) -> Value {
    if let Some(map) = args.as_object_mut() {
        map.retain(|key, _| !key.starts_with("__ax"));
    }
    args
}
