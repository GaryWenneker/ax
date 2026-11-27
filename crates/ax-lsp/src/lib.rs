//! Optional Language Server Protocol bridge for ax.
//!
//! Discovers local language servers (`rust-analyzer`, `typescript-language-server`,
//! `pyright-langserver`, `gopls`) and uses `textDocument/definition` to enrich
//! unresolved references with [`ax_types::EdgeConfidence::Exact`] edges
//! ([`ax_types::Provenance::Lsp`]).

mod client;
mod enrich;
mod servers;

pub use enrich::EnrichReport;
pub use servers::{
    discover_servers, discover_servers_in, server_available, server_available_in, ServerStatus,
    SERVERS,
};

use std::collections::HashMap;
use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_types::Edge;
use ax_utils::errors::AxError;

use crate::servers::spec_for_extension;

/// Run LSP enrichment against unresolved refs and upsert Exact edges.
pub async fn enrich_project(
    project_root: &Path,
    queries: &QueryBuilder,
    limit: usize,
) -> Result<EnrichReport, AxError> {
    let refs = queries.get_unresolved_refs().await?;
    let mut by_file: HashMap<String, Vec<ax_types::UnresolvedReference>> = HashMap::new();
    for r in refs.into_iter().take(limit) {
        let fp = r.file_path.clone().unwrap_or_default().replace('\\', "/");
        if fp.is_empty() {
            continue;
        }
        by_file.entry(fp).or_default().push(r);
    }

    let mut jobs = Vec::new();
    let mut report = EnrichReport::default();
    for (rel, file_refs) in by_file {
        let ext = Path::new(&rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if spec_for_extension(ext).is_none() {
            report.examined += file_refs.len() as u32;
            report.skipped_no_server += file_refs.len() as u32;
            continue;
        }
        let full = project_root.join(&rel);
        if !full.is_file() {
            report.examined += file_refs.len() as u32;
            report.errors.push(format!("{rel}: file not found"));
            continue;
        }
        let language_id = enrich::language_id(ext).to_string();
        jobs.push((rel, full, language_id, file_refs));
    }

    // Server discovery spawns processes; keep it and all LSP work off the async runtime.
    let root = project_root.to_path_buf();
    let (mut batch_report, partials) = tokio::task::spawn_blocking(move || {
        let mut resolver = crate::servers::ServerResolver::for_project(Some(&root));
        let deadline = std::time::Instant::now() + enrich::ENRICH_DEADLINE;
        enrich::enrich_files_blocking(&root, jobs, &mut resolver, deadline)
    })
    .await
    .map_err(|e| AxError::Other(e.to_string()))?;

    report.examined += batch_report.examined;
    report.resolved += batch_report.resolved;
    report.skipped_no_server += batch_report.skipped_no_server;
    report.skipped_no_definition += batch_report.skipped_no_definition;
    report.skipped_deadline += batch_report.skipped_deadline;
    report.errors.append(&mut batch_report.errors);

    let mut nodes_by_path: HashMap<String, Vec<ax_types::Node>> = HashMap::new();
    for mut p in partials {
        for edge in &mut p.edges {
            remap_one_edge(queries, edge, &mut nodes_by_path).await?;
        }
        // Zip edges with the refs they resolved. Targets that stay `lsp:…`
        // point outside the graph (stdlib/deps) and cannot be upserted
        // (edges.target FK → nodes.id). Still drop the unresolved row.
        for (edge, r) in p.edges.iter().zip(p.resolved_refs.iter()) {
            if edge.target.starts_with("lsp:") {
                let _ = queries.delete_unresolved_ref(r).await;
                continue;
            }
            queries.upsert_edge(edge).await?;
            let _ = queries.delete_unresolved_ref(r).await;
        }
    }

    Ok(report)
}

async fn remap_one_edge(
    queries: &QueryBuilder,
    edge: &mut Edge,
    nodes_by_path: &mut HashMap<String, Vec<ax_types::Node>>,
) -> Result<(), AxError> {
    let Some(meta) = edge.metadata.as_ref() else {
        return Ok(());
    };
    let Some(target) = meta.get("lspTarget") else {
        return Ok(());
    };
    let path = target.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let line = target.get("line").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
    if path.is_empty() {
        return Ok(());
    }
    if !nodes_by_path.contains_key(path) {
        let nodes = queries.get_nodes_by_file(path).await?;
        nodes_by_path.insert(path.to_string(), nodes);
    }
    if let Some(node) = target_node_at(&nodes_by_path[path], line) {
        edge.target = node.id.clone();
    }
    Ok(())
}

/// The node an LSP definition at `line` points at: the narrowest symbol whose
/// range contains the line (a function or method wins a tie), else the file.
fn target_node_at(nodes: &[ax_types::Node], line: i32) -> Option<&ax_types::Node> {
    use ax_types::NodeKind;
    nodes
        .iter()
        .filter(|n| n.kind != NodeKind::File && n.start_line <= line && n.end_line >= line)
        .min_by_key(|n| {
            let callable = matches!(n.kind, NodeKind::Function | NodeKind::Method);
            (n.end_line - n.start_line, !callable)
        })
        .or_else(|| nodes.iter().find(|n| n.kind == NodeKind::File))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::{Language, Node, NodeKind};

    fn node(id: &str, kind: NodeKind, start: i32, end: i32) -> Node {
        Node {
            id: id.into(),
            kind,
            name: id.into(),
            qualified_name: id.into(),
            file_path: "src/store.ts".into(),
            language: Language::Typescript,
            start_line: start,
            end_line: end,
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
        }
    }

    #[test]
    fn definition_inside_a_method_targets_the_method_not_its_class() {
        // Classes are stored before their methods, so the class comes first.
        let nodes = vec![
            node("file", NodeKind::File, 1, 300),
            node("Store", NodeKind::Class, 100, 200),
            node("Store.load", NodeKind::Method, 105, 115),
            node("Store.save", NodeKind::Method, 118, 130),
        ];
        assert_eq!(
            target_node_at(&nodes, 120).map(|n| n.id.as_str()),
            Some("Store.save")
        );
        assert_eq!(
            target_node_at(&nodes, 150).map(|n| n.id.as_str()),
            Some("Store")
        );
        assert_eq!(
            target_node_at(&nodes, 250).map(|n| n.id.as_str()),
            Some("file")
        );
    }
}
