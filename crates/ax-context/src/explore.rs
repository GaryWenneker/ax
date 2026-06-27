//! Rich explore: search hits, numbered source snippets, caller/callee spines.

use std::collections::HashSet;
use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_graph::query_parser::parse_query;
use ax_graph::query_utils::matches_parsed_query;
use ax_graph::GraphTraverser;
use ax_types::{ExploreEntry, ExploreOptions, ExploreResult, Node, SearchOptions};

pub struct ExploreBuilder {
    queries: QueryBuilder,
    traverser: GraphTraverser,
    project_root: std::path::PathBuf,
}

impl ExploreBuilder {
    pub fn new(
        queries: QueryBuilder,
        traverser: GraphTraverser,
        project_root: std::path::PathBuf,
    ) -> Self {
        Self {
            queries,
            traverser,
            project_root,
        }
    }

    pub async fn explore(
        &self,
        query: &str,
        opts: ExploreOptions,
    ) -> Result<ExploreResult, ax_utils::errors::AxError> {
        let limit = opts.limit.unwrap_or(5);
        let depth = opts.depth.unwrap_or(2);
        let include_code = opts.include_code.unwrap_or(true);
        let max_lines = opts.max_lines_per_snippet.unwrap_or(80) as usize;
        let max_source_chars = opts.max_source_chars.unwrap_or(4000) as usize;

        let parsed = parse_query(query);
        let mut search_opts = SearchOptions {
            limit: Some(limit),
            ..Default::default()
        };
        if !parsed.kinds.is_empty() {
            search_opts.kinds = Some(parsed.kinds.clone());
        }
        if !parsed.languages.is_empty() {
            search_opts.languages = Some(parsed.languages.clone());
        }
        if !parsed.path_filters.is_empty() {
            search_opts.include_patterns = Some(parsed.path_filters.clone());
        }
        let hits = self.queries.search_nodes(&parsed.text, &search_opts).await?;
        let hits: Vec<_> = hits
            .into_iter()
            .filter(|h| matches_parsed_query(&h.node, &parsed))
            .collect();

        let mut entries = Vec::new();
        let mut files_seen = HashSet::new();
        let mut total_callers = 0;
        let mut total_callees = 0;

        for hit in hits {
            let node = hit.node;
            files_seen.insert(node.file_path.clone());

            let callers = self.traverser.get_callers(&node.id, depth).await?;
            let callees = self.traverser.get_callees(&node.id, depth).await?;
            total_callers += callers.len();
            total_callees += callees.len();
            for c in &callers {
                files_seen.insert(c.file_path.clone());
            }
            for c in &callees {
                files_seen.insert(c.file_path.clone());
            }

            let source = if include_code {
                Some(numbered_snippet(
                    &self.project_root,
                    &node,
                    max_lines,
                    max_source_chars,
                ))
            } else {
                None
            };

            entries.push(ExploreEntry {
                node,
                score: hit.score,
                source,
                callers,
                callees,
            });
        }

        let blast_radius = if entries.is_empty() {
            format!("No symbols matching '{}'", query)
        } else {
            format!(
                "{} entry point(s); {} caller(s), {} callee(s) across {} file(s)",
                entries.len(),
                total_callers,
                total_callees,
                files_seen.len()
            )
        };

        let summary = if entries.is_empty() {
            blast_radius.clone()
        } else {
            format!("Found {} entry point(s) for '{}'", entries.len(), query)
        };

        Ok(ExploreResult {
            query: query.to_string(),
            summary,
            blast_radius,
            entries,
        })
    }
}

fn numbered_snippet(root: &Path, node: &Node, max_lines: usize, max_chars: usize) -> String {
    numbered_snippet_with_sep(root, node, max_lines, max_chars, '\t')
}

fn numbered_snippet_with_sep(
    root: &Path,
    node: &Node,
    max_lines: usize,
    max_chars: usize,
    sep: char,
) -> String {
    let full = root.join(&node.file_path);
    let content = std::fs::read_to_string(&full).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let start = (node.start_line as usize).saturating_sub(1);
    let end = node.end_line as usize;
    let slice = lines.get(start..end.min(lines.len())).unwrap_or(&[]);
    let truncated_lines = slice.len() > max_lines;
    let out = slice
        .iter()
        .take(max_lines)
        .enumerate()
        .map(|(i, line)| format!("{}{}{}", start + i + 1, sep, line))
        .collect::<Vec<_>>()
        .join("\n");
    let result = if out.len() > max_chars {
        format!(
            "{}\n...(truncated to {} chars; increase maxSourceChars)",
            &out[..max_chars],
            max_chars
        )
    } else if truncated_lines {
        format!(
            "{}\n...(truncated to {} lines; off-spine signatures omitted (adaptive skeleton); increase maxLinesPerSnippet)",
            out,
            max_lines
        )
    } else {
        out
    };
    result
}

#[cfg(test)]
mod snippet_tests {
    use super::*;
    use ax_types::{Language, Node, NodeKind};

    #[test]
    fn numbered_snippet_truncation_hint() {
        let dir = std::env::temp_dir().join("ax-explore-snippet-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("lines.ts");
        let body: String = (1..=20).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        std::fs::write(&file_path, body).unwrap();
        let node = Node {
            id: "n1".into(),
            kind: NodeKind::Function,
            name: "f".into(),
            qualified_name: "f".into(),
            file_path: file_path.to_string_lossy().into_owned(),
            language: Language::Typescript,
            start_line: 1,
            end_line: 20,
            start_column: 0,
            end_column: 0,
            docstring: None,
            signature: None,
            visibility: None,
            is_exported: None,
            is_async: None,
            is_static: None,
            is_abstract: None,
            decorators: None,
            type_parameters: None,
            return_type: None,
            updated_at: 0,
        };
        let text = numbered_snippet(&dir, &node, 3, 4000);
        assert!(text.contains("truncated to 3 lines"));
        assert!(text.contains("1\tline1"));
    }
}
