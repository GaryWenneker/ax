//! MCP `tools/list` allowlist via `AX_MCP_TOOLS`.
//!
//! Default (unset): lean surface — explore + preflight/capture (+ policy tools
//! when present). Set `AX_MCP_TOOLS=all` for the full menu, or a comma-separated
//! list of short names (`explore,node,search,callers`) / full names (`ax_explore`).

use std::collections::HashSet;
use std::sync::Mutex;

pub const MCP_TOOLS_ENV: &str = "AX_MCP_TOOLS";

/// Serialize env reads/writes so parallel tests don't race on `AX_MCP_TOOLS`.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

/// Always listed (when applicable) — not gated by the allowlist.
pub fn is_core_tool(name: &str) -> bool {
    matches!(
        name,
        "ax_explore"
            | "ax_preflight"
            | "ax_policy_capture"
            | "ax_rules"
            | "ax_skill"
            | "ax_guard"
    )
}

/// Parse `AX_MCP_TOOLS`. `None` = lean default (core only). `Some` with `"*"` = everything.
pub fn resolve_tool_allowlist() -> Option<HashSet<String>> {
    let _guard = env_lock().lock().unwrap_or_else(|e| e.into_inner());
    let raw = std::env::var(MCP_TOOLS_ENV).ok();
    resolve_tool_allowlist_from(raw.as_deref())
}

pub fn resolve_tool_allowlist_from(raw: Option<&str>) -> Option<HashSet<String>> {
    let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
        return None;
    };
    let lower = raw.to_ascii_lowercase();
    if lower.trim() == "all" || lower.trim() == "*" {
        return Some(HashSet::from(["*".to_string()]));
    }
    let mut set = HashSet::new();
    for part in raw.split(',') {
        let t = part.trim();
        if t.is_empty() {
            continue;
        }
        set.insert(canonicalize_tool_token(t));
    }
    Some(set)
}

fn canonicalize_tool_token(token: &str) -> String {
    token.trim().trim_start_matches("ax_").to_ascii_lowercase()
}

pub fn tool_allowed(name: &str, allowlist: &Option<HashSet<String>>) -> bool {
    if is_core_tool(name) {
        return true;
    }
    match allowlist {
        None => false,
        Some(set) if set.contains("*") => true,
        Some(set) => {
            let short = canonicalize_tool_token(name);
            set.contains(&short) || set.contains(&name.to_ascii_lowercase())
        }
    }
}

/// Filter a tools array Value in place for `tools/list`.
pub fn filter_tools_list(tools: &mut Vec<serde_json::Value>) {
    filter_tools_list_with(tools, resolve_tool_allowlist());
}

pub fn filter_tools_list_with(
    tools: &mut Vec<serde_json::Value>,
    allowlist: Option<HashSet<String>>,
) {
    tools.retain(|t| {
        t.get("name")
            .and_then(|v| v.as_str())
            .map(|n| tool_allowed(n, &allowlist))
            .unwrap_or(false)
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lean_default_keeps_core_only() {
        let allow = resolve_tool_allowlist_from(None);
        assert!(tool_allowed("ax_explore", &allow));
        assert!(tool_allowed("ax_preflight", &allow));
        assert!(tool_allowed("ax_guard", &allow));
        assert!(!tool_allowed("ax_search", &allow));
        assert!(!tool_allowed("ax_sync", &allow));
        assert!(!tool_allowed("ax_diagnostics", &allow));
    }

    #[test]
    fn all_exposes_everything() {
        let allow = resolve_tool_allowlist_from(Some("all"));
        assert!(tool_allowed("ax_search", &allow));
        assert!(tool_allowed("ax_ship", &allow));
    }

    #[test]
    fn comma_list_accepts_short_names() {
        let allow = resolve_tool_allowlist_from(Some("node,search,callers"));
        assert!(tool_allowed("ax_node", &allow));
        assert!(tool_allowed("ax_search", &allow));
        assert!(tool_allowed("ax_callers", &allow));
        assert!(!tool_allowed("ax_callees", &allow));
        assert!(tool_allowed("ax_explore", &allow));
    }
}
