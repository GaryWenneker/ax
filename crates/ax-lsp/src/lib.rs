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
pub use servers::{discover_servers, server_available, ServerStatus, SERVERS};

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
        let Some(spec) = spec_for_extension(ext) else {
            report.examined += file_refs.len() as u32;
            report.skipped_no_server += file_refs.len() as u32;
            continue;
        };
        if !crate::servers::server_available(spec) {
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

    let root = project_root.to_path_buf();
    let (mut batch_report, partials) = tokio::task::spawn_blocking(move || {
        enrich::enrich_files_blocking(&root, jobs)
    })
    .await
    .map_err(|e| AxError::Other(e.to_string()))?;

    report.examined += batch_report.examined;
    report.resolved += batch_report.resolved;
    report.skipped_no_server += batch_report.skipped_no_server;
    report.skipped_no_definition += batch_report.skipped_no_definition;
    report.errors.append(&mut batch_report.errors);

    for mut p in partials {
        for edge in &mut p.edges {
            remap_one_edge(queries, edge).await?;
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

async fn remap_one_edge(queries: &QueryBuilder, edge: &mut Edge) -> Result<(), AxError> {
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
    let nodes = queries.get_nodes_by_file(path).await?;
    if let Some(node) = nodes.iter().find(|n| {
        n.kind != ax_types::NodeKind::File && n.start_line <= line && n.end_line >= line
    }) {
        edge.target = node.id.clone();
    } else if let Some(file_node) = nodes.iter().find(|n| n.kind == ax_types::NodeKind::File) {
        edge.target = file_node.id.clone();
    }
    Ok(())
}
