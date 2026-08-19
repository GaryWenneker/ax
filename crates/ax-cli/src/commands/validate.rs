//! Graph hygiene validator (`ax validate [--ci]`).

use std::collections::{HashMap, HashSet};

use ax_types::{EdgeKind, NodeKind};

use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ValidateReport {
    isolated: Vec<String>,
    dangling_edges: Vec<String>,
    orphan_docs: Vec<String>,
    ok: bool,
}

pub async fn run(path: Option<String>, ci: bool, json: bool) -> Result<(), String> {
    let root = resolve_path(path);
    ax_usage::log_cli(Some(&root), "cmd=validate start");
    let _spinner = SpinnerGuard::new("Validating graph hygiene...", false);
    let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
    let nodes = ax.queries().get_all_nodes().await.map_err(|e| e.to_string())?;
    let edges = ax.queries().get_all_edges().await.map_err(|e| e.to_string())?;

    let node_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let mut degree: HashMap<&str, usize> = HashMap::new();
    let mut dangling = Vec::new();

    for e in &edges {
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
    for n in &nodes {
        if matches!(n.kind, NodeKind::File | NodeKind::Doc) {
            continue;
        }
        if degree.get(n.id.as_str()).copied().unwrap_or(0) == 0 {
            isolated.push(n.qualified_name.clone());
        }
    }
    isolated.sort();
    if isolated.len() > 200 {
        isolated.truncate(200);
    }

    let mut orphan_docs = Vec::new();
    for n in &nodes {
        if n.kind != NodeKind::Doc {
            continue;
        }
        let has_link = edges.iter().any(|e| {
            (e.source == n.id || e.target == n.id)
                && matches!(e.kind, EdgeKind::Documents | EdgeKind::References)
        });
        if !has_link {
            orphan_docs.push(n.qualified_name.clone());
        }
    }
    orphan_docs.sort();
    if orphan_docs.len() > 100 {
        orphan_docs.truncate(100);
    }

    let report = ValidateReport {
        ok: dangling.is_empty() && isolated.is_empty(),
        dangling_edges: dangling,
        isolated,
        orphan_docs,
    };

    ax_usage::log_cli(
        Some(&root),
        format!(
            "cmd=validate ok={} dangling={} isolated={} orphan_docs={}",
            report.ok,
            report.dangling_edges.len(),
            report.isolated.len(),
            report.orphan_docs.len()
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
                    report.isolated.len()
                );
                for s in report.isolated.iter().take(40) {
                    println!("- {s}");
                }
                if report.isolated.len() > 40 {
                    println!("- … and {} more", report.isolated.len() - 40);
                }
                println!();
            }
            if !report.orphan_docs.is_empty() {
                println!(
                    "### Orphan docs (no documents/references edges) ({})",
                    report.orphan_docs.len()
                );
                for s in report.orphan_docs.iter().take(20) {
                    println!("- {s}");
                }
                if report.orphan_docs.len() > 20 {
                    println!("- … and {} more", report.orphan_docs.len() - 20);
                }
            }
        }
    }

    if ci && !report.ok {
        return Err(format!(
            "validate --ci failed: {} dangling edge(s), {} isolated symbol(s)",
            report.dangling_edges.len(),
            report.isolated.len()
        ));
    }
    Ok(())
}
