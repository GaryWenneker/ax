//! Task context builder.

use std::collections::HashSet;

use ax_db::queries::QueryBuilder;
use ax_graph::GraphTraverser;
use ax_types::{
    BuildContextOptions, CodeBlock, ContextFormat, SearchOptions, TaskContext, TaskContextStats,
    TaskInput,
};

use crate::formatter::{format_context_as_json, format_context_as_markdown};
use crate::markers::LOW_CONFIDENCE_MARKER;
use crate::source_store::{
    resolve_source, slice_lines, stale_note, unavailable_reason, ResolvedSource,
};

/// Builds task context from the graph alone.
///
/// Holds no project root on purpose — see [`crate::source_store`]. Code blocks
/// come from the source store, so this type cannot reach the working tree.
pub struct ContextBuilder {
    queries: QueryBuilder,
    traverser: GraphTraverser,
}

impl ContextBuilder {
    pub fn new(queries: QueryBuilder, traverser: GraphTraverser) -> Self {
        Self { queries, traverser }
    }

    pub async fn build_context(
        &self,
        input: TaskInput,
        opts: BuildContextOptions,
    ) -> Result<TaskContext, ax_utils::errors::AxError> {
        let query = match input {
            TaskInput::Text(s) => s,
            TaskInput::Structured { title, description } => {
                if let Some(d) = description {
                    format!("{}: {}", title, d)
                } else {
                    title
                }
            }
        };

        let search_opts = SearchOptions {
            limit: opts.search_limit.or(Some(5)),
            ..Default::default()
        };
        let results = self.queries.search_nodes(&query, &search_opts).await?;
        let entry_points: Vec<_> = results.into_iter().map(|r| r.node).collect();

        let depth = opts.traversal_depth.unwrap_or(2);
        let mut related_files = HashSet::new();
        let mut subgraph = ax_types::Subgraph::default();
        let mut edge_count = 0u32;
        let mut code_blocks = Vec::new();
        let max_blocks = opts.max_code_blocks.unwrap_or(6) as usize;
        let max_size = opts.max_code_block_size.unwrap_or(1200) as usize;

        for node in &entry_points {
            related_files.insert(node.file_path.clone());
            if opts.include_code.unwrap_or(true) && code_blocks.len() < max_blocks {
                // Source store only — a context block that silently read the
                // working tree would break the graph-only guarantee.
                let resolved = resolve_source(&self.queries, &node.file_path).await?;
                let text = match &resolved {
                    ResolvedSource::Fresh(content) => Some(slice_lines(
                        content,
                        node.start_line,
                        node.end_line,
                        max_size,
                    )),
                    ResolvedSource::Stale(content) => Some(format!(
                        "{}\n{}",
                        stale_note(&node.file_path),
                        slice_lines(content, node.start_line, node.end_line, max_size)
                    )),
                    // Emit the marker as the block body: an agent must see that
                    // the source is missing, not silently get fewer blocks.
                    _ => unavailable_reason(&resolved, &node.file_path),
                };
                if let Some(content) = text {
                    code_blocks.push(CodeBlock {
                        content,
                        file_path: node.file_path.clone(),
                        start_line: node.start_line,
                        end_line: node.end_line,
                        language: node.language,
                        node: Some(node.clone()),
                    });
                }
            }
        }

        if let Some(first) = entry_points.first() {
            if let Ok(sg) = self.traverser.get_impact_subgraph(&first.id, depth).await {
                edge_count = sg.edges.len() as u32;
                for n in sg.nodes.values() {
                    related_files.insert(n.file_path.clone());
                }
                subgraph = sg;
            }
        }

        let summary = if entry_points.is_empty() {
            format!("No matching symbols for: {}", query)
        } else {
            format!("Found {} entry points for: {}", entry_points.len(), query)
        };

        let entry_count = entry_points.len() as u32;
        let code_block_count = code_blocks.len() as u32;
        let file_count = related_files.len() as u32;
        let total_code_size = code_blocks.iter().map(|b| b.content.len() as u32).sum();
        let ctx = TaskContext {
            query,
            subgraph,
            entry_points,
            code_blocks,
            related_files: related_files.into_iter().collect(),
            summary,
            stats: TaskContextStats {
                node_count: entry_count,
                edge_count,
                file_count,
                code_block_count,
                total_code_size,
            },
        };

        Ok(ctx)
    }

    pub fn format(&self, ctx: &TaskContext, format: ContextFormat) -> String {
        match format {
            ContextFormat::Markdown => format_context_as_markdown(ctx),
            ContextFormat::Json => format_context_as_json(ctx),
        }
    }

    pub fn low_confidence_marker(&self) -> &'static str {
        LOW_CONFIDENCE_MARKER
    }
}