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

pub struct ContextBuilder {
    queries: QueryBuilder,
    traverser: GraphTraverser,
    project_root: std::path::PathBuf,
}

impl ContextBuilder {
    pub fn new(queries: QueryBuilder, traverser: GraphTraverser, project_root: std::path::PathBuf) -> Self {
        Self {
            queries,
            traverser,
            project_root,
        }
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
        let max_blocks = opts.max_code_blocks.unwrap_or(10) as usize;
        let max_size = opts.max_code_block_size.unwrap_or(2000) as usize;

        for node in &entry_points {
            related_files.insert(node.file_path.clone());
            if opts.include_code.unwrap_or(true) && code_blocks.len() < max_blocks {
                let full = self.project_root.join(&node.file_path);
                if let Ok(content) = std::fs::read_to_string(&full) {
                    let lines: Vec<&str> = content.lines().collect();
                    let start = (node.start_line as usize).saturating_sub(1);
                    let end = node.end_line as usize;
                    let slice = lines.get(start..end.min(lines.len())).unwrap_or(&[]);
                    let block_content = slice.join("\n");
                    let truncated = if block_content.len() > max_size {
                        block_content[..max_size].to_string()
                    } else {
                        block_content
                    };
                    code_blocks.push(CodeBlock {
                        content: truncated,
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