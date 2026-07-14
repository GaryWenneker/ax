//! Rich explore: search hits, numbered source snippets, caller/callee spines.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_graph::query_parser::parse_query;
use ax_graph::query_utils::matches_parsed_query;
use ax_graph::GraphTraverser;
use ax_types::{
    CallNeighbor, EdgeConfidence, EdgeKind, ExploreEntry, ExploreOptions, ExploreResult, Node,
    SearchOptions,
};

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

            // Two batched edge queries give the direct (depth-1) edge kind +
            // confidence for each neighbor; transitive neighbors stay untagged.
            let incoming = self.queries.get_incoming_edges(&node.id).await?;
            let outgoing = self.queries.get_outgoing_edges(&node.id, None).await?;
            let caller_conf: HashMap<String, (EdgeKind, EdgeConfidence)> = incoming
                .iter()
                .map(|e| (e.source.clone(), (e.kind, e.effective_confidence())))
                .collect();
            let callee_conf: HashMap<String, (EdgeKind, EdgeConfidence)> = outgoing
                .iter()
                .map(|e| (e.target.clone(), (e.kind, e.effective_confidence())))
                .collect();
            let callers = to_neighbors(callers, &caller_conf);
            let callees = to_neighbors(callees, &callee_conf);

            let source = if include_code {
                // File IO off the async runtime so slow disks don't stall
                // other in-flight MCP queries.
                let root = self.project_root.clone();
                let snippet_node = node.clone();
                Some(
                    tokio::task::spawn_blocking(move || {
                        numbered_snippet(&root, &snippet_node, max_lines, max_source_chars)
                    })
                    .await
                    .unwrap_or_default(),
                )
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

/// Attach the direct edge kind + confidence (if known) to each neighbor node.
fn to_neighbors(
    nodes: Vec<Node>,
    conf: &HashMap<String, (EdgeKind, EdgeConfidence)>,
) -> Vec<CallNeighbor> {
    nodes
        .into_iter()
        .map(|node| {
            let (edge_kind, confidence) = match conf.get(&node.id) {
                Some((k, c)) => (Some(*k), Some(*c)),
                None => (None, None),
            };
            CallNeighbor {
                node,
                edge_kind,
                confidence,
            }
        })
        .collect()
}

fn numbered_snippet_with_sep(
    root: &Path,
    node: &Node,
    max_lines: usize,
    max_chars: usize,
    sep: char,
) -> String {
    let full = root.join(&node.file_path);
    let content = match std::fs::read_to_string(&full) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("explore: cannot read {}: {e}", full.display());
            return format!("(source unavailable: {})", node.file_path);
        }
    };
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
