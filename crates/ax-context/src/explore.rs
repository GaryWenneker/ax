//! Rich explore: search hits, numbered source snippets, caller/callee spines.

use std::collections::{HashMap, HashSet};

use ax_db::queries::QueryBuilder;
use ax_graph::query_parser::parse_query;
use ax_graph::query_utils::matches_parsed_query;
use ax_graph::GraphTraverser;
use ax_types::{
    CallNeighbor, EdgeConfidence, EdgeKind, ExploreEntry, ExploreOptions, ExploreResult, Node,
    SearchOptions,
};

use crate::source_store::{
    numbered_slice, resolve_source, stale_note, unavailable_reason, ResolvedSource,
    NOT_STORED_MARKER,
};

/// Builds explore results from the graph alone.
///
/// Deliberately holds no project root: without it this type *cannot* read the
/// working tree, so the graph-only guarantee is enforced by construction rather
/// than by reviewer vigilance.
pub struct ExploreBuilder {
    queries: QueryBuilder,
    traverser: GraphTraverser,
}

impl ExploreBuilder {
    pub fn new(queries: QueryBuilder, traverser: GraphTraverser) -> Self {
        Self { queries, traverser }
    }

    pub async fn explore(
        &self,
        query: &str,
        opts: ExploreOptions,
    ) -> Result<ExploreResult, ax_utils::errors::AxError> {
        let limit = opts.limit.unwrap_or(5);
        let depth = opts.depth.unwrap_or(2);
        let include_code = opts.include_code.unwrap_or(true);
        // Token-strict defaults; explicit params win, env overrides the default.
        let max_lines = opts
            .max_lines_per_snippet
            .map(|n| n as usize)
            .unwrap_or_else(|| env_usize("AX_EXPLORE_MAX_LINES", 40));
        let max_source_chars = opts
            .max_source_chars
            .map(|n| n as usize)
            .unwrap_or_else(|| env_usize("AX_EXPLORE_MAX_SOURCE_CHARS", 2000));

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
                Some(
                    self.graph_snippet(&node, max_lines, max_source_chars, '\t')
                        .await?,
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

    /// Snippet for one node, sourced from `ax.db` only.
    ///
    /// A hash mismatch is labelled rather than repaired here: this layer holds no
    /// index lock, so re-indexing is the caller's job (`Ax::explore` does one
    /// bounded attempt and retries the query).
    async fn graph_snippet(
        &self,
        node: &Node,
        max_lines: usize,
        max_chars: usize,
        sep: char,
    ) -> Result<String, ax_utils::errors::AxError> {
        let resolved = resolve_source(&self.queries, &node.file_path).await?;
        match &resolved {
            ResolvedSource::Fresh(content) => Ok(numbered_slice(
                content,
                node.start_line,
                node.end_line,
                max_lines,
                max_chars,
                sep,
            )),
            ResolvedSource::Stale(content) => {
                let body = numbered_slice(
                    content,
                    node.start_line,
                    node.end_line,
                    max_lines,
                    max_chars,
                    sep,
                );
                Ok(format!("{}\n{}", stale_note(&node.file_path), body))
            }
            _ => Ok(unavailable_reason(&resolved, &node.file_path)
                .unwrap_or_else(|| format!("({NOT_STORED_MARKER}: {})", node.file_path))),
        }
    }

    /// Files in this result whose stored source no longer matches the graph.
    pub async fn stale_files(
        &self,
        result: &ExploreResult,
    ) -> Result<Vec<String>, ax_utils::errors::AxError> {
        let paths: Vec<String> = result
            .entries
            .iter()
            .map(|e| e.node.file_path.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        self.queries.stale_source_paths(&paths).await
    }
}

/// Read a positive `usize` from `name`, falling back to `default`.
fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n| n > 0)
        .unwrap_or(default)
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

