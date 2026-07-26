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
    let mut report = EnrichReport::default();
    let refs = queries.get_unresolved_refs().await?;
    let mut by_file: HashMap<String, Vec<ax_types::UnresolvedReference>> = HashMap::new();
    for r in refs.into_iter().take(limit) {
        let fp = r.file_path.clone().unwrap_or_default().replace('\\', "/");
        if fp.is_empty() {
            continue;
        }
        by_file.entry(fp).or_default().push(r);
    }

    for (rel, file_refs) in by_file {
        report.examined += file_refs.len() as u32;
        let ext = Path::new(&rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        let Some(spec) = spec_for_extension(ext) else {
            report.skipped_no_server += file_refs.len() as u32;
            continue;
        };
        if !crate::servers::server_available(spec) {
            report.skipped_no_server += file_refs.len() as u32;
            continue;
        }
        let full = project_root.join(&rel);
        let content = match std::fs::read_to_string(&full) {
            Ok(c) => c,
            Err(e) => {
                report.errors.push(format!("{rel}: {e}"));
                continue;
            }
        };
        let root = project_root.to_path_buf();
        let language_id = enrich::language_id(ext).to_string();
        let file_refs_owned = file_refs;
        let partial = tokio::task::spawn_blocking(move || {
            enrich::enrich_file_blocking(
                &root,
                &full,
                &language_id,
                spec,
                &content,
                &file_refs_owned,
            )
        })
        .await
        .map_err(|e| AxError::Other(e.to_string()))?;

        match partial {
            Ok(mut p) => {
                for edge in &mut p.edges {
                    remap_one_edge(queries, edge).await?;
                }
                for edge in &p.edges {
                    queries.upsert_edge(edge).await?;
                }
                for r in &p.resolved_refs {
                    let _ = queries.delete_unresolved_ref(r).await;
                }
                report.resolved += p.resolved;
                report.skipped_no_definition += p.skipped_no_definition;
                report.errors.extend(p.errors);
            }
            Err(e) => report.errors.push(format!("{rel}: {e}")),
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
