//! Graph hygiene validator (`ax validate [--ci]`).

use std::collections::{HashMap, HashSet};

use ax_types::{Edge, EdgeKind, Node, NodeKind};

use crate::commands::resolve_project_root;
use crate::ui::SpinnerGuard;

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidateReport {
    isolated: Vec<String>,
    dangling_edges: Vec<String>,
    orphan_docs: Vec<String>,
    isolated_count: usize,
    orphan_docs_count: usize,
    ok: bool,
}

pub async fn run(path: Option<String>, ci: bool, json: bool) -> Result<(), String> {
    let root = resolve_project_root(path);
    ax_usage::log_cli(Some(&root), "cmd=validate start");
    let _spinner = SpinnerGuard::new("Validating graph hygiene...", false);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let nodes = ax
        .queries()
        .get_all_nodes()
        .await
        .map_err(|e| e.to_string())?;
    let edges = ax
        .queries()
        .get_all_edges()
        .await
        .map_err(|e| e.to_string())?;

    let report = build_report(&nodes, &edges);

    ax_usage::log_cli(
        Some(&root),
        format!(
            "cmd=validate ok={} dangling={} isolated={} orphan_docs={}",
            report.ok,
            report.dangling_edges.len(),
            report.isolated_count,
            report.orphan_docs_count
        ),
    );

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        println!("## ax validate\n");
        if report.ok && report.orphan_docs.is_empty() {
            println!("No issues found.");
        } else {
            if !report.dangling_edges.is_empty() {
                println!("### Dangling edges ({})", report.dangling_edges.len());
                for d in &report.dangling_edges {
                    println!("- {d}");
                }
                println!();
            }
            if !report.isolated.is_empty() {
                println!(
                    "### Isolated symbols (0 call/import edges) ({})",
                    report.isolated_count
                );
                for s in report.isolated.iter().take(40) {
                    println!("- {s}");
                }
                if report.isolated_count > 40 {
                    println!("- … and {} more", report.isolated_count - 40);
                }
                println!();
            }
            if !report.orphan_docs.is_empty() {
                println!(
                    "### Orphan docs (no documents/references edges) ({})",
                    report.orphan_docs_count
                );
                for s in report.orphan_docs.iter().take(20) {
                    println!("- {s}");
                }
                if report.orphan_docs_count > 20 {
                    println!("- … and {} more", report.orphan_docs_count - 20);
                }
            }
        }
    }

    if ci && !report.ok {
        return Err(ci_failure_message(&report));
    }
    Ok(())
}

fn ci_failure_message(report: &ValidateReport) -> String {
    format!(
        "validate --ci failed: {} dangling edge(s), {} isolated symbol(s)",
        report.dangling_edges.len(),
        report.isolated_count
    )
}

/// Most isolated symbols kept in the report list; the count is never capped.
const MAX_ISOLATED_LISTED: usize = 200;
/// Most orphan docs kept in the report list; the count is never capped.
const MAX_ORPHAN_DOCS_LISTED: usize = 100;

fn build_report(nodes: &[Node], edges: &[Edge]) -> ValidateReport {
    let node_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut degree: HashMap<&str, usize> = HashMap::new();
    let mut doc_linked: HashSet<&str> = HashSet::new();
    let mut dangling = Vec::new();

    for e in edges {
        if matches!(e.kind, EdgeKind::Documents | EdgeKind::References) {
            doc_linked.insert(e.source.as_str());
            doc_linked.insert(e.target.as_str());
        }
        let src_ok = node_ids.contains(e.source.as_str());
        let dst_ok = node_ids.contains(e.target.as_str());
        if !src_ok || !dst_ok {
            dangling.push(format!(
                "{} -[{:?}]-> {} (missing {})",
                e.source,
                e.kind,
                e.target,
                if !src_ok && !dst_ok {
                    "source+target"
                } else if !src_ok {
                    "source"
                } else {
                    "target"
                }
            ));
            continue;
        }
        if matches!(e.kind, EdgeKind::Contains) {
            continue;
        }
        *degree.entry(e.source.as_str()).or_default() += 1;
        *degree.entry(e.target.as_str()).or_default() += 1;
    }

    let mut isolated = Vec::new();
    let mut orphan_docs = Vec::new();
    for n in nodes {
        match n.kind {
            NodeKind::Doc => {
                if !doc_linked.contains(n.id.as_str()) {
                    orphan_docs.push(n.qualified_name.clone());
                }
            }
            NodeKind::File => {}
            _ => {
                if degree.get(n.id.as_str()).copied().unwrap_or(0) == 0 {
                    isolated.push(n.qualified_name.clone());
                }
            }
        }
    }
    let isolated_count = isolated.len();
    let orphan_docs_count = orphan_docs.len();
    isolated.sort();
    isolated.truncate(MAX_ISOLATED_LISTED);
    orphan_docs.sort();
    orphan_docs.truncate(MAX_ORPHAN_DOCS_LISTED);

    ValidateReport {
        ok: dangling.is_empty() && isolated_count == 0,
        isolated_count,
        orphan_docs_count,
        dangling_edges: dangling,
        isolated,
        orphan_docs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, kind: &str) -> Node {
        serde_json::from_value(serde_json::json!({
            "id": id, "kind": kind, "name": id, "qualifiedName": id,
            "filePath": "a.rs", "language": "rust", "startLine": 1, "endLine": 1,
            "startColumn": 0, "endColumn": 0, "updatedAt": 0
        }))
        .unwrap()
    }

    fn edge(source: &str, target: &str, kind: &str) -> Edge {
        serde_json::from_value(serde_json::json!({
            "source": source, "target": target, "kind": kind
        }))
        .unwrap()
    }

    #[test]
    fn counts_report_every_isolated_symbol_while_lists_stay_capped() {
        let nodes: Vec<Node> = (0..300)
            .map(|i| node(&format!("f{i:03}"), "function"))
            .collect();
        let report = build_report(&nodes, &[]);
        assert!(report.isolated.len() <= 200);
        assert_eq!(report.isolated_count, 300);
        let msg = ci_failure_message(&report);
        assert!(msg.contains("300 isolated symbol(s)"), "{msg}");
    }

    #[test]
    fn orphan_docs_are_found_through_documents_and_references_edges() {
        let nodes = vec![
            node("d1", "doc"),
            node("d2", "doc"),
            node("d3", "doc"),
            node("d4", "doc"),
            node("f", "function"),
        ];
        let edges = vec![
            edge("d1", "f", "documents"),
            edge("f", "d2", "references"),
            edge("d3", "f", "calls"),
        ];
        let report = build_report(&nodes, &edges);
        assert_eq!(report.orphan_docs, vec!["d3".to_string(), "d4".to_string()]);
        assert_eq!(report.orphan_docs_count, 2);
        assert!(report.dangling_edges.is_empty());
        assert!(report.isolated.is_empty());
    }
}
