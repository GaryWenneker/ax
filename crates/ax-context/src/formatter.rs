//! Markdown and JSON context formatters.

use ax_types::TaskContext;

pub fn format_context_as_markdown(ctx: &TaskContext) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Task Context: {}\n\n", ctx.query));
    out.push_str(&format!("## Summary\n{}\n\n", ctx.summary));
    out.push_str(&format!("## Stats\n- Nodes: {}\n- Files: {}\n- Code blocks: {}\n\n", ctx.stats.node_count, ctx.stats.file_count, ctx.stats.code_block_count));
    for block in &ctx.code_blocks {
        out.push_str(&format!("### {} ({}:{})\n```\n{}\n```\n\n", block.file_path, block.start_line, block.end_line, block.content));
    }
    out
}

pub fn format_context_as_json(ctx: &TaskContext) -> String {
    serde_json::to_string_pretty(ctx).unwrap_or_default()
}
