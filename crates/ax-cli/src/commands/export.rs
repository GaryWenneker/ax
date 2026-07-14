use std::collections::HashMap;
use std::path::PathBuf;

use ax_graph::analysis::is_semantic_edge;
use serde_json::json;

use crate::commands::resolve_path;
use crate::ui::SpinnerGuard;

pub async fn run_graph_html(
    path: Option<String>,
    out: Option<String>,
    resolution: f64,
    limit: usize,
) -> Result<(), String> {
    let root = resolve_path(path);
    let resolution = if resolution > 0.0 { resolution } else { 1.0 };
    let limit = limit.max(1);

    let (payload_json, project_name) = {
        let _spinner = SpinnerGuard::new("Exporting graph to HTML...".to_string(), false);
        let ax = ax_core::Ax::open(&root).await.map_err(|e| e.to_string())?;
        // Compute + persist community assignments.
        ax.insights(resolution, 25, 25).await.map_err(|e| e.to_string())?;

        let nodes = ax.queries().get_all_nodes().await.map_err(|e| e.to_string())?;
        let edges = ax.queries().get_all_edges().await.map_err(|e| e.to_string())?;
        let communities = ax
            .queries()
            .get_node_communities()
            .await
            .map_err(|e| e.to_string())?;

        let community_map: HashMap<String, (i64, Option<String>)> = communities
            .into_iter()
            .map(|(id, cid, label)| (id, (cid, label)))
            .collect();

        // Degree over semantic edges (matches the analysis god-node ranking).
        let mut degree: HashMap<&str, usize> = HashMap::new();
        for e in &edges {
            if !is_semantic_edge(e.kind) {
                continue;
            }
            *degree.entry(e.source.as_str()).or_insert(0) += 1;
            *degree.entry(e.target.as_str()).or_insert(0) += 1;
        }

        // Top-N nodes by degree.
        let mut ranked: Vec<&ax_types::Node> = nodes.iter().collect();
        ranked.sort_by(|a, b| {
            degree
                .get(b.id.as_str())
                .unwrap_or(&0)
                .cmp(degree.get(a.id.as_str()).unwrap_or(&0))
                .then_with(|| a.name.cmp(&b.name))
        });
        ranked.truncate(limit);

        let selected: std::collections::HashSet<&str> =
            ranked.iter().map(|n| n.id.as_str()).collect();

        let json_nodes: Vec<serde_json::Value> = ranked
            .iter()
            .map(|n| {
                let (cid, label) = community_map
                    .get(&n.id)
                    .cloned()
                    .unwrap_or((-1, None));
                json!({
                    "id": n.id,
                    "name": n.name,
                    "kind": n.kind.as_str(),
                    "file": n.file_path,
                    "community": cid,
                    "label": label,
                    "degree": degree.get(n.id.as_str()).copied().unwrap_or(0),
                })
            })
            .collect();

        let json_edges: Vec<serde_json::Value> = edges
            .iter()
            .filter(|e| {
                is_semantic_edge(e.kind)
                    && selected.contains(e.source.as_str())
                    && selected.contains(e.target.as_str())
            })
            .map(|e| {
                json!({
                    "s": e.source,
                    "t": e.target,
                    "kind": e.kind.as_str(),
                    "confidence": e.effective_confidence().as_str(),
                })
            })
            .collect();

        let project_name = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        let payload = json!({
            "nodes": json_nodes,
            "edges": json_edges,
            "totalNodes": nodes.len(),
        });
        (serde_json::to_string(&payload).unwrap_or_else(|_| "{}".to_string()), project_name)
    };

    // Prevent premature </script> termination from any embedded string.
    let safe_json = payload_json.replace("</", "<\\/");
    let html = render_html(&project_name, &safe_json);

    let out_path = out
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("graph.html"));
    std::fs::write(&out_path, html)
        .map_err(|e| format!("failed to write {}: {e}", out_path.display()))?;
    println!("Wrote interactive graph to {}", out_path.display());
    Ok(())
}

fn render_html(project_name: &str, data_json: &str) -> String {
    // Single self-contained file: inline data + a small canvas force layout.
    let template = include_str!("export_graph.html");
    template
        .replace("__PROJECT_NAME__", &html_escape(project_name))
        .replace("\"__GRAPH_DATA__\"", data_json)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
