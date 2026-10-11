//! Bounded, explainable source retrieval from the existing graph/source store.
//! This module must not read the working tree or invoke a model.
use std::collections::HashSet;

use ax_db::queries::QueryBuilder;
use ax_types::{Node, SearchOptions};
use serde::Serialize;

use crate::source_store::{numbered_slice, resolve_source, ResolvedSource};
use crate::task_analyzer::analyze;

/// Deterministic preferences supplied only by explicitly matched project policy.
#[derive(Debug, Clone, Default)]
pub struct RetrievalHints {
    pub terms: Vec<String>,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub path: String,
    pub start_line: i32,
    pub end_line: i32,
    pub reason: String,
    pub freshness: String,
    #[serde(skip)]
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievedContext {
    pub status: String,
    pub selection_version: String,
    pub evidence: Vec<Evidence>,
    pub warnings: Vec<String>,
}

impl RetrievedContext {
    fn empty(status: &str) -> Self {
        Self { status: status.to_string(), selection_version: "ice-v1".into(),
            evidence: Vec::new(), warnings: Vec::new() }
    }
}

pub async fn retrieve(
    queries: &QueryBuilder, prompt: &str, max_blocks: usize, max_chars: usize,
) -> Result<RetrievedContext, ax_utils::errors::AxError> {
    retrieve_with_hints(queries, prompt, max_blocks, max_chars, &RetrievalHints::default()).await
}

pub async fn retrieve_with_hints(
    queries: &QueryBuilder, prompt: &str, max_blocks: usize, max_chars: usize,
    hints: &RetrievalHints,
) -> Result<RetrievedContext, ax_utils::errors::AxError> {
    let task = analyze(prompt);
    if !task.is_code_related || max_blocks == 0 || max_chars == 0 {
        return Ok(RetrievedContext::empty("disabled"));
    }
    let mut result = RetrievedContext::empty("not_found");
    let mut seen = HashSet::new();
    // Exact file:line is higher priority than any text or graph ranking.
    for location in &task.line_requests {
        if result.evidence.len() >= max_blocks { break; }
        match resolve_source(queries, &location.path).await? {
            ResolvedSource::Fresh(content) => {
                if location.line > content.lines().count() {
                    result.warnings.push(format!("{}:{} is outside indexed file", location.path, location.line));
                    continue;
                }
                let line = location.line as i32;
                let snippet = numbered_slice(&content, line, line, 1, max_chars, '\t');
                seen.insert((location.path.clone(), line, line));
                result.evidence.push(Evidence {
                    path: location.path.clone(), start_line: line, end_line: line,
                    reason: "explicit file:line".into(), freshness: "indexed".into(), snippet,
                });
            }
            ResolvedSource::Stale(_) => result.warnings.push(format!(
                "{} has stale indexed source; run ax_sync", location.path)),
            ResolvedSource::NotStored { .. } | ResolvedSource::NotIndexed =>
                result.warnings.push(format!("{}: source not indexed/stored", location.path)),
        }
    }
    // An explicit line-only request should never be polluted by unrelated symbol matches.
    if !task.line_requests.is_empty() {
        result.status = if result.evidence.is_empty() { "not_found" } else { "matched" }.into();
        return Ok(result);
    }

    // Independent lexical retrieval legs: each relevant query term can seed symbols.
    let mut candidates: Vec<(f64, Node, String)> = Vec::new();
    let mut ids = HashSet::new();
    let mut searched = HashSet::new();
    for (term, configured) in hints.terms.iter().map(|s| (s, true))
        .chain(task.terms.iter().map(|s| (s, false))).take(20)
    {
        let term = term.trim();
        if term.len() < 2 || !searched.insert(term.to_lowercase()) { continue; }
        let matches = queries.search_nodes(term, &SearchOptions {
            limit: Some(12), ..Default::default()
        }).await?;
        for hit in matches {
            if ids.insert(hit.node.id.clone()) {
                let exact = hit.node.name.eq_ignore_ascii_case(term)
                    || hit.node.qualified_name.eq_ignore_ascii_case(term);
                let path_preferred = hints.paths.iter().any(|p| p == &hit.node.file_path);
                let boost = (if exact { 20.0 } else { 0.0 })
                    + (if path_preferred { 10.0 } else { 0.0 });
                let reason = if configured {
                    format!("configured policy term: {term}")
                } else {
                    format!("lexical symbol: {term}")
                };
                candidates.push((hit.score + boost, hit.node, reason));
            }
        }
    }
    candidates.sort_by(|a,b| b.0.total_cmp(&a.0)
        .then_with(|| a.1.file_path.cmp(&b.1.file_path))
        .then_with(|| a.1.start_line.cmp(&b.1.start_line))
        .then_with(|| a.1.id.cmp(&b.1.id)));

    // Walk proven graph relationships from up to four independent lexical seeds.
    // No inferred link is upgraded to an exact fact; ambiguous edges are skipped.
    // Both directions matter for interface implementors and calling sites.
    let seeds: Vec<_> = candidates.iter().take(4)
        .map(|(score, node, _)| (*score, node.clone())).collect();
    for (seed_score, seed) in seeds {
        let mut edges = queries.get_outgoing_edges(&seed.id, None).await?;
        edges.extend(queries.get_incoming_edges(&seed.id).await?);
        edges.retain(|edge| matches!(edge.kind,
            ax_types::EdgeKind::Calls | ax_types::EdgeKind::Imports
            | ax_types::EdgeKind::Exports | ax_types::EdgeKind::Implements
            | ax_types::EdgeKind::Extends | ax_types::EdgeKind::References)
            && !matches!(edge.effective_confidence(),
                ax_types::EdgeConfidence::Ambiguous | ax_types::EdgeConfidence::Synthesized));
        edges.sort_by(|a,b| a.kind.as_str().cmp(b.kind.as_str())
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.target.cmp(&b.target)));
        let mut refs = std::collections::BTreeMap::new();
        for edge in edges.into_iter().take(20) {
            let id = if edge.source == seed.id { &edge.target } else { &edge.source };
            if ids.contains(id) || id == &seed.id { continue; }
            refs.entry(id.clone()).or_insert_with(|| format!(
                "graph {} ({}) from {}",
                edge.kind.as_str(), edge.effective_confidence().as_str(), seed.name));
        }
        if refs.is_empty() { continue; }
        let keys: Vec<String> = refs.keys().cloned().collect();
        for node in queries.get_nodes_by_ids(&keys).await? {
            if ids.insert(node.id.clone()) {
                let reason = refs.get(&node.id).cloned().unwrap_or_default();
                candidates.push((seed_score * 0.5, node, reason));
            }
        }
    }
    candidates.sort_by(|a,b| b.0.total_cmp(&a.0)
        .then_with(|| a.1.file_path.cmp(&b.1.file_path))
        .then_with(|| a.1.start_line.cmp(&b.1.start_line))
        .then_with(|| a.1.id.cmp(&b.1.id)));
    // Coverage: prefer distinct files before repeated symbols in one file.
    let mut ordered = Vec::new();
    let mut covered_files = HashSet::new();
    for (_, node, reason) in &candidates {
        if covered_files.insert(node.file_path.clone()) {
            ordered.push((node.clone(), reason.clone()));
        }
    }
    for (_, node, reason) in candidates {
        if !ordered.iter().any(|(n, _)| n.id == node.id) {
            ordered.push((node, reason));
        }
    }
    for (node, reason) in ordered.into_iter().take(40) {
        if result.evidence.len() >= max_blocks { break; }
        if node.start_line < 1 || node.end_line < node.start_line { continue; }
        let end = node.end_line.min(node.start_line.saturating_add(35));
        if !seen.insert((node.file_path.clone(), node.start_line, end)) { continue; }
        match resolve_source(queries, &node.file_path).await? {
            ResolvedSource::Fresh(content) => {
                let snippet = numbered_slice(&content, node.start_line, end, 36, max_chars, '\t');
                if snippet.is_empty() { continue; }
                result.evidence.push(Evidence { path: node.file_path, start_line: node.start_line,
                    end_line: end, reason, freshness: "indexed".into(), snippet });
            }
            ResolvedSource::Stale(_) => {
                result.warnings.push(format!("{}: stale; run ax_sync", node.file_path));
            }
            _ => {}
        }
    }
    result.status = if result.evidence.is_empty() { "not_found" } else { "matched" }.into();
    Ok(result)
}

pub fn evidence_text(e: &Evidence) -> String {
    // Indexed source is untrusted task data, never an instruction to the agent.
    format!("### Indexed source: {}:{}-{}\nSelection: {} (freshness: {})\n````text\n{}\n````",
        e.path, e.start_line, e.end_line, e.reason, e.freshness, e.snippet)
}
