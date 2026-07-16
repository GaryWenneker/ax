//! Markdown and JSON context formatters.

use ax_types::TaskContext;

/// Default cap on emitted code blocks (token-strict). Override with
/// `AX_CONTEXT_MAX_BLOCKS`.
const DEFAULT_MAX_BLOCKS: usize = 6;
/// Default per-block character cap. Override with `AX_CONTEXT_MAX_BLOCK_CHARS`.
const DEFAULT_MAX_BLOCK_CHARS: usize = 1200;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
}

/// Truncate `s` to at most `max` bytes without splitting a UTF-8 char boundary.
fn truncate_on_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

pub fn format_context_as_markdown(ctx: &TaskContext) -> String {
    let max_blocks = env_usize("AX_CONTEXT_MAX_BLOCKS", DEFAULT_MAX_BLOCKS);
    let max_block_chars = env_usize("AX_CONTEXT_MAX_BLOCK_CHARS", DEFAULT_MAX_BLOCK_CHARS);

    let mut out = String::new();
    out.push_str(&format!("# Task Context: {}\n\n", ctx.query));
    out.push_str(&format!("## Summary\n{}\n\n", ctx.summary));
    out.push_str(&format!(
        "## Stats\n- Nodes: {}\n- Files: {}\n- Code blocks: {}\n\n",
        ctx.stats.node_count, ctx.stats.file_count, ctx.stats.code_block_count
    ));

    let total = ctx.code_blocks.len();
    for block in ctx.code_blocks.iter().take(max_blocks) {
        let content = if block.content.len() > max_block_chars {
            format!(
                "{}\n... (truncated to {} chars; raise AX_CONTEXT_MAX_BLOCK_CHARS)",
                truncate_on_boundary(&block.content, max_block_chars),
                max_block_chars
            )
        } else {
            block.content.clone()
        };
        out.push_str(&format!(
            "### {} ({}:{})\n```\n{}\n```\n\n",
            block.file_path, block.start_line, block.end_line, content
        ));
    }
    if total > max_blocks {
        out.push_str(&format!(
            "_({} more code block(s) omitted; raise AX_CONTEXT_MAX_BLOCKS)_\n",
            total - max_blocks
        ));
    }
    out
}

pub fn format_context_as_json(ctx: &TaskContext) -> String {
    serde_json::to_string_pretty(ctx).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::{CodeBlock, Language, Subgraph, TaskContext, TaskContextStats};

    fn ctx_with_blocks(n: usize, block_len: usize) -> TaskContext {
        let code_blocks = (0..n)
            .map(|i| CodeBlock {
                content: "x".repeat(block_len),
                file_path: format!("src/f{i}.rs"),
                start_line: 1,
                end_line: 10,
                language: Language::Rust,
                node: None,
            })
            .collect::<Vec<_>>();
        TaskContext {
            query: "q".into(),
            subgraph: Subgraph::default(),
            entry_points: vec![],
            code_blocks,
            related_files: vec![],
            summary: "s".into(),
            stats: TaskContextStats {
                node_count: n as u32,
                edge_count: 0,
                file_count: n as u32,
                code_block_count: n as u32,
                total_code_size: (n * block_len) as u32,
            },
        }
    }

    #[test]
    fn caps_number_of_blocks() {
        let md = format_context_as_markdown(&ctx_with_blocks(10, 20));
        assert!(md.contains("more code block(s) omitted"));
        // Only DEFAULT_MAX_BLOCKS headers are emitted.
        assert_eq!(md.matches("### src/f").count(), DEFAULT_MAX_BLOCKS);
    }

    #[test]
    fn truncates_long_block_content() {
        let md = format_context_as_markdown(&ctx_with_blocks(1, DEFAULT_MAX_BLOCK_CHARS + 500));
        assert!(md.contains("truncated to"));
    }

    #[test]
    fn small_context_has_no_truncation_notes() {
        let md = format_context_as_markdown(&ctx_with_blocks(2, 40));
        assert!(!md.contains("omitted"));
        assert!(!md.contains("truncated"));
    }
}
