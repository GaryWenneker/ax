//! ax user-visible log branding — axe icon on logging and chat output.

/// Unicode axe (U+1FA93) prefix for ax logging and chat output.
pub const AX_LOG_ICON: &str = "🪓";

fn already_branded(rest: &str) -> bool {
    rest.starts_with(AX_LOG_ICON)
}

/// `[ax] 🪓 …` line for domain logs, token-budget hints, and stderr chat output.
pub fn format_ax_tagged(message: impl AsRef<str>) -> String {
    let msg = message.as_ref().trim_start();
    if let Some(rest) = msg.strip_prefix("[ax]") {
        let rest = rest.trim_start();
        if already_branded(rest) {
            return msg.to_string();
        }
        return format!("[ax] {AX_LOG_ICON} {rest}");
    }
    format!("[ax] {AX_LOG_ICON} {msg}")
}

/// `[ax-mcp] 🪓 …` verbose MCP trace line.
pub fn format_ax_mcp_trace(message: impl AsRef<str>) -> String {
    let msg = message.as_ref().trim_start();
    if let Some(rest) = msg.strip_prefix("[ax-mcp]") {
        let rest = rest.trim_start();
        if already_branded(rest) {
            return msg.to_string();
        }
        return format!("[ax-mcp] {AX_LOG_ICON} {rest}");
    }
    format!("[ax-mcp] {AX_LOG_ICON} {msg}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_ax_tagged_adds_icon() {
        assert_eq!(
            format_ax_tagged("token budget: narrow query"),
            "[ax] 🪓 token budget: narrow query"
        );
        assert_eq!(
            format_ax_tagged("[ax] memory remember ok"),
            "[ax] 🪓 memory remember ok"
        );
    }

    #[test]
    fn format_ax_tagged_idempotent() {
        let once = format_ax_tagged("hello");
        assert_eq!(format_ax_tagged(&once), once);
    }

    #[test]
    fn format_ax_mcp_trace_adds_icon() {
        assert_eq!(
            format_ax_mcp_trace("inbound tool=ax_preflight args={}"),
            "[ax-mcp] 🪓 inbound tool=ax_preflight args={}"
        );
    }
}
