//! MCP `tools/list` allowlist via `AX_MCP_TOOLS`.
//!
//! Default (unset): the turn contract (preflight/capture/policy) plus the whole
//! **graph read surface**, so an agent that trusts `tools/list` can follow the
//! `explore-before-grep` and `agent-workflow` rules without guessing at tool
//! names. Heavy ops (index, sync-adjacent rebuilds, lsp, ship, diagnostics,
//! policy re-index) stay opt-in: set `AX_MCP_TOOLS=all` for the full menu, or a
//! comma-separated list of short names (`ship,lsp`) / full names (`ax_ship`).
//!
//! Unlisted tools remain **callable** — `call_tool` is not filtered. The
//! allowlist only controls discovery.

use std::collections::HashSet;
use std::sync::Mutex;

pub const MCP_TOOLS_ENV: &str = "AX_MCP_TOOLS";

/// Always listed (when applicable) — not gated by the allowlist.
///
/// Two groups, and the split is deliberate:
/// 1. **Turn contract** — preflight, directive capture, policy delivery, guard.
/// 2. **Graph read surface** — every query tool the CRITICAL policy rules tell
///    agents to prefer over `Grep`/`Read`. Hiding these is what pushed agents
///    into filesystem sweeps; see `docs/audits/2026-08-19-preflight-graph-only/`.
///
/// Nothing here writes to the repo. `ax_sync` is the one entry that mutates the
/// index, and it is included because `prefer-mcp-ops` mandates it over
/// `ax sync` in the shell.
pub const CORE_TOOLS: &[&str] = &[
    // Turn contract
    "ax_preflight",
    "ax_policy_capture",
    "ax_rules",
    "ax_skill",
    "ax_guard",
    // Graph read surface
    "ax_explore",
    "ax_search",
    "ax_node",
    "ax_callers",
    "ax_callees",
    "ax_impact",
    "ax_path",
    "ax_cycles",
    "ax_api",
    "ax_context",
    "ax_affected",
    "ax_insights",
    "ax_report",
    "ax_status",
    "ax_sync",
    "ax_remember",
    "ax_recall",
];

/// Every `ax_*` tool named by the shipped policy rules and IDE bootstrap text
/// (`crates/ax-policy/templates/rules/`, `templates/skills/`, `src/ide_seed.rs`).
///
/// The coherence test below asserts each of these is classified — either
/// advertised in [`CORE_TOOLS`] or knowingly gated in [`GATED_BY_DESIGN`].
/// Naming a new tool in a rule without deciding its catalog status is the exact
/// drift that made `ax_search` invisible while policy demanded it.
pub const POLICY_REFERENCED_TOOLS: &[&str] = &[
    "ax_preflight",
    "ax_policy_capture",
    "ax_rules",
    "ax_skill",
    "ax_guard",
    "ax_explore",
    "ax_search",
    "ax_node",
    "ax_callers",
    "ax_callees",
    "ax_impact",
    "ax_affected",
    "ax_insights",
    "ax_report",
    "ax_context",
    "ax_status",
    "ax_sync",
    "ax_remember",
    "ax_recall",
    "ax_index",
    "ax_lsp",
    "ax_ship",
    "ax_diagnostics",
    "ax_policy_index",
];

/// Policy-referenced tools intentionally kept out of the default catalog.
///
/// These mutate the index, spawn language servers, or run the quality gate, so
/// they are opt-in via `AX_MCP_TOOLS`. They stay callable by name.
///
/// Known tension: `prefer-mcp-ops` (CRITICAL) tells agents to use `ax_index`,
/// `ax_lsp`, `ax_ship`, and `ax_policy_index` instead of the shell, but an agent
/// reading only `tools/list` will not see them. Documented in
/// `site/src/content/docs/reference/mcp-server.md` — resolve by promoting them
/// or by softening the rule, not by letting the two drift silently.
pub const GATED_BY_DESIGN: &[&str] = &[
    "ax_index",
    "ax_lsp",
    "ax_ship",
    "ax_diagnostics",
    "ax_policy_index",
];

/// Serialize env reads/writes so parallel tests don't race on `AX_MCP_TOOLS`.
fn env_lock() -> &'static Mutex<()> {
    static LOCK: Mutex<()> = Mutex::new(());
    &LOCK
}

/// Always listed (when applicable) — not gated by the allowlist.
pub fn is_core_tool(name: &str) -> bool {
    CORE_TOOLS.contains(&name)
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
    fn default_advertises_turn_contract_and_graph_reads() {
        let allow = resolve_tool_allowlist_from(None);
        for name in CORE_TOOLS {
            assert!(
                tool_allowed(name, &allow),
                "{name} is core and must be advertised by default"
            );
        }
        // The graph read surface the policy rules mandate — the audit's C6 gap.
        for name in ["ax_search", "ax_node", "ax_callers", "ax_impact", "ax_status", "ax_sync"] {
            assert!(tool_allowed(name, &allow), "{name} must be discoverable");
        }
    }

    #[test]
    fn default_still_gates_heavy_ops() {
        let allow = resolve_tool_allowlist_from(None);
        for name in GATED_BY_DESIGN {
            assert!(
                !tool_allowed(name, &allow),
                "{name} is opt-in; the allowlist must still gate it"
            );
        }
        // ax_files is superseded by graph queries and is not policy-referenced.
        assert!(!tool_allowed("ax_files", &allow));
    }

    #[test]
    fn every_policy_referenced_tool_is_classified() {
        for name in POLICY_REFERENCED_TOOLS {
            let core = CORE_TOOLS.contains(name);
            let gated = GATED_BY_DESIGN.contains(name);
            assert!(
                core || gated,
                "{name} is named by shipped policy but is neither advertised nor \
                 knowingly gated — classify it in CORE_TOOLS or GATED_BY_DESIGN"
            );
            assert!(
                !(core && gated),
                "{name} cannot be both advertised and gated"
            );
        }
    }

    #[test]
    fn core_and_gated_sets_are_disjoint_and_unique() {
        for name in CORE_TOOLS {
            assert!(
                !GATED_BY_DESIGN.contains(name),
                "{name} appears in both CORE_TOOLS and GATED_BY_DESIGN"
            );
        }
        let unique: HashSet<&&str> = CORE_TOOLS.iter().collect();
        assert_eq!(unique.len(), CORE_TOOLS.len(), "duplicate entry in CORE_TOOLS");
        let unique_gated: HashSet<&&str> = GATED_BY_DESIGN.iter().collect();
        assert_eq!(
            unique_gated.len(),
            GATED_BY_DESIGN.len(),
            "duplicate entry in GATED_BY_DESIGN"
        );
    }

    #[test]
    fn all_exposes_everything() {
        let allow = resolve_tool_allowlist_from(Some("all"));
        assert!(tool_allowed("ax_search", &allow));
        assert!(tool_allowed("ax_ship", &allow));
    }

    #[test]
    fn comma_list_accepts_short_and_full_names() {
        let allow = resolve_tool_allowlist_from(Some("lsp,ax_ship"));
        assert!(tool_allowed("ax_lsp", &allow), "short name should opt in");
        assert!(tool_allowed("ax_ship", &allow), "full name should opt in");
        assert!(!tool_allowed("ax_diagnostics", &allow), "unlisted stays gated");
        assert!(!tool_allowed("ax_index", &allow), "unlisted stays gated");
        // Core is unaffected by the allowlist.
        assert!(tool_allowed("ax_explore", &allow));
    }
}
