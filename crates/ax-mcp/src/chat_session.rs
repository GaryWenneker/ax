//! Which chat a tool call belongs to.
//!
//! Cursor sends no conversation id to MCP servers, so the agent carries the id that
//! preflight printed. Preflight without one starts a new chat; other tools fall back
//! to the last session of their connection.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    Preflight,
    Tool,
}

/// A new chat session id: `axs_` and 16 hex characters.
pub fn mint_session() -> String {
    format!("axs_{}", &uuid::Uuid::new_v4().simple().to_string()[..16])
}

pub fn resolve_session(
    kind: CallKind,
    arg: Option<String>,
    hook: Option<String>,
    connection: Option<&str>,
    mint: impl FnOnce() -> String,
) -> String {
    if let Some(id) = arg.or(hook) {
        return id;
    }
    match (kind, connection) {
        (CallKind::Tool, Some(id)) => id.to_string(),
        _ => mint(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minted() -> String {
        "axs_minted".to_string()
    }

    fn some(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    #[test]
    fn the_argument_wins_over_hook_and_connection() {
        for kind in [CallKind::Preflight, CallKind::Tool] {
            assert_eq!(resolve_session(kind, some("axs_arg"), some("hook"), Some("conn"), minted), "axs_arg");
        }
    }

    #[test]
    fn a_recent_hook_id_wins_over_the_connection() {
        for kind in [CallKind::Preflight, CallKind::Tool] {
            assert_eq!(resolve_session(kind, None, some("hook"), Some("conn"), minted), "hook");
        }
    }

    #[test]
    fn preflight_without_an_id_starts_a_new_chat() {
        assert_eq!(resolve_session(CallKind::Preflight, None, None, Some("conn"), minted), "axs_minted");
        assert_eq!(resolve_session(CallKind::Preflight, None, None, None, minted), "axs_minted");
    }

    #[test]
    fn a_tool_without_an_id_uses_its_connection_then_mints() {
        assert_eq!(resolve_session(CallKind::Tool, None, None, Some("conn"), minted), "conn");
        assert_eq!(resolve_session(CallKind::Tool, None, None, None, minted), "axs_minted");
    }

    #[test]
    fn minted_sessions_are_axs_with_16_hex_and_unique() {
        let a = mint_session();
        let b = mint_session();
        for id in [&a, &b] {
            let hex = id.strip_prefix("axs_").unwrap_or_else(|| panic!("{id}"));
            assert_eq!(hex.len(), 16, "{id}");
            assert!(hex.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{id}");
        }
        assert_ne!(a, b);
    }

    #[test]
    fn mint_runs_only_when_needed() {
        let panics = || -> String { panic!("minted although an id was known") };
        resolve_session(CallKind::Tool, some("a"), None, None, panics);
        resolve_session(CallKind::Tool, None, some("h"), None, panics);
        resolve_session(CallKind::Tool, None, None, Some("c"), panics);
    }
}
