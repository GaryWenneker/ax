//! Opt-in business-domain overlay stored beside the deterministic graph.
//!
//! The structural index (`ax.db`) is never modified. Agents (or a PUT) write
//! `.ax/domain-graph.json`; Command Center Graph → Domain renders it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};

use crate::workspace_state::WebHub;
use crate::ApiError;

pub const DOMAIN_GRAPH_FILE: &str = "domain-graph.json";
const MAX_NODES: usize = 2_000;
const MAX_EDGES: usize = 5_000;

const NODE_KINDS: &[&str] = &["domain", "flow", "step"];
const EDGE_KINDS: &[&str] = &["contains_flow", "flow_step", "cross_domain"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DomainNode {
    pub id: String,
    pub kind: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub code_node_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DomainEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DomainGraph {
    #[serde(default = "default_version")]
    pub version: u32,
    #[serde(default)]
    pub nodes: Vec<DomainNode>,
    #[serde(default)]
    pub edges: Vec<DomainEdge>,
}

fn default_version() -> u32 {
    1
}

impl DomainGraph {
    pub fn empty() -> Self {
        Self {
            version: 1,
            nodes: Vec::new(),
            edges: Vec::new(),
        }
    }
}

pub fn domain_graph_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join(DOMAIN_GRAPH_FILE)
}

pub fn validate(graph: &DomainGraph) -> Result<(), String> {
    if graph.nodes.len() > MAX_NODES {
        return Err(format!(
            "too many domain nodes ({}); max is {MAX_NODES}",
            graph.nodes.len()
        ));
    }
    if graph.edges.len() > MAX_EDGES {
        return Err(format!(
            "too many domain edges ({}); max is {MAX_EDGES}",
            graph.edges.len()
        ));
    }

    let mut ids = HashSet::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        if node.id.trim().is_empty() {
            return Err("domain node id must not be empty".into());
        }
        if node.name.trim().is_empty() {
            return Err(format!("domain node {} has an empty name", node.id));
        }
        if !NODE_KINDS.contains(&node.kind.as_str()) {
            return Err(format!(
                "unsupported domain node kind '{}' on {} (expected domain|flow|step)",
                node.kind, node.id
            ));
        }
        if !ids.insert(node.id.clone()) {
            return Err(format!("duplicate domain node id {}", node.id));
        }
    }

    for edge in &graph.edges {
        if !EDGE_KINDS.contains(&edge.kind.as_str()) {
            return Err(format!(
                "unsupported domain edge kind '{}' (expected contains_flow|flow_step|cross_domain)",
                edge.kind
            ));
        }
        if !ids.contains(&edge.source) {
            return Err(format!("domain edge source {} is not a node", edge.source));
        }
        if !ids.contains(&edge.target) {
            return Err(format!("domain edge target {} is not a node", edge.target));
        }
        if edge.source == edge.target {
            return Err(format!("domain edge {} → {} is a self-loop", edge.source, edge.target));
        }
    }
    Ok(())
}

pub fn load(project_root: &Path) -> Result<DomainGraph, String> {
    let path = domain_graph_path(project_root);
    if !path.exists() {
        return Ok(DomainGraph::empty());
    }
    let bytes = std::fs::read(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    if bytes.is_empty() {
        return Ok(DomainGraph::empty());
    }
    let graph: DomainGraph = serde_json::from_slice(&bytes)
        .map_err(|e| format!("parse {}: {e}", path.display()))?;
    validate(&graph)?;
    Ok(graph)
}

pub fn save(project_root: &Path, graph: &DomainGraph) -> Result<(), String> {
    validate(graph)?;
    let ax_dir = project_root.join(".ax");
    std::fs::create_dir_all(&ax_dir).map_err(|e| format!("create {}: {e}", ax_dir.display()))?;
    let path = domain_graph_path(project_root);
    let tmp = ax_dir.join(format!("{DOMAIN_GRAPH_FILE}.tmp"));
    let json = serde_json::to_vec_pretty(graph).map_err(|e| format!("serialize domain graph: {e}"))?;
    std::fs::write(&tmp, json).map_err(|e| format!("write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("rename {} → {}: {e}", tmp.display(), path.display()))?;
    Ok(())
}

pub async fn handle_get(State(hub): State<WebHub>) -> impl IntoResponse {
    let ws = hub.read().await;
    match load(&ws.project_root) {
        Ok(graph) => (StatusCode::OK, Json(graph)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError { error: e }),
        )
            .into_response(),
    }
}

pub async fn handle_put(State(hub): State<WebHub>, Json(graph): Json<DomainGraph>) -> impl IntoResponse {
    if hub.readonly {
        return (
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "read-only mode (AX_WEB_READONLY=1)".into(),
            }),
        )
            .into_response();
    }
    let ws = hub.read().await;
    match save(&ws.project_root, &graph) {
        Ok(()) => (StatusCode::OK, Json(graph)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError { error: e }),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample() -> DomainGraph {
        DomainGraph {
            version: 1,
            nodes: vec![
                DomainNode {
                    id: "domain:auth".into(),
                    kind: "domain".into(),
                    name: "Authentication".into(),
                    summary: Some("Sign-in and sessions".into()),
                    code_node_ids: vec!["fn:login".into()],
                },
                DomainNode {
                    id: "flow:login".into(),
                    kind: "flow".into(),
                    name: "Login".into(),
                    summary: None,
                    code_node_ids: vec![],
                },
                DomainNode {
                    id: "step:verify".into(),
                    kind: "step".into(),
                    name: "Verify token".into(),
                    summary: None,
                    code_node_ids: vec!["fn:verify".into()],
                },
            ],
            edges: vec![
                DomainEdge {
                    source: "domain:auth".into(),
                    target: "flow:login".into(),
                    kind: "contains_flow".into(),
                    order: None,
                },
                DomainEdge {
                    source: "flow:login".into(),
                    target: "step:verify".into(),
                    kind: "flow_step".into(),
                    order: Some(1.0),
                },
            ],
        }
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = tempdir().unwrap();
        let graph = load(dir.path()).unwrap();
        assert_eq!(graph, DomainGraph::empty());
    }

    #[test]
    fn roundtrip_save_load() {
        let dir = tempdir().unwrap();
        let graph = sample();
        save(dir.path(), &graph).unwrap();
        let loaded = load(dir.path()).unwrap();
        assert_eq!(loaded, graph);
        assert!(domain_graph_path(dir.path()).exists());
    }

    #[test]
    fn rejects_unknown_node_kind() {
        let mut graph = sample();
        graph.nodes[0].kind = "service".into();
        let err = validate(&graph).unwrap_err();
        assert!(err.contains("unsupported domain node kind"), "{err}");
    }

    #[test]
    fn rejects_dangling_edge() {
        let mut graph = sample();
        graph.edges[0].target = "flow:missing".into();
        let err = validate(&graph).unwrap_err();
        assert!(err.contains("is not a node"), "{err}");
    }

    #[test]
    fn rejects_duplicate_id() {
        let mut graph = sample();
        graph.nodes.push(graph.nodes[0].clone());
        let err = validate(&graph).unwrap_err();
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn rejects_unknown_edge_kind() {
        let mut graph = sample();
        graph.edges[0].kind = "calls".into();
        let err = validate(&graph).unwrap_err();
        assert!(err.contains("unsupported domain edge kind"), "{err}");
    }

    #[test]
    fn empty_json_file_is_empty_graph() {
        let dir = tempdir().unwrap();
        let ax = dir.path().join(".ax");
        std::fs::create_dir_all(&ax).unwrap();
        std::fs::write(ax.join(DOMAIN_GRAPH_FILE), b"").unwrap();
        assert_eq!(load(dir.path()).unwrap(), DomainGraph::empty());
    }
}
