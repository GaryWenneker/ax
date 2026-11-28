//! Callback / EventEmitter edge synthesis (emit/on pairs).

use std::collections::HashMap;
use std::path::Path;

use regex::Regex;

use ax_db::queries::QueryBuilder;
use ax_types::{Edge, EdgeKind, Node, NodeKind, Provenance};

use crate::frameworks::extract::is_js_family;
use crate::resolver::ResolutionScope;

const EVENT_FANOUT_CAP: usize = 6;

pub struct CallbackSynthesizer;

impl Default for CallbackSynthesizer {
    fn default() -> Self {
        Self::new()
    }
}

impl CallbackSynthesizer {
    pub fn new() -> Self {
        Self
    }

    pub async fn synthesize(
        &self,
        project_root: &Path,
        queries: &QueryBuilder,
        scope: ResolutionScope<'_>,
    ) -> Result<(), ax_utils::errors::AxError> {
        let on_re = Regex::new(
            r#"(?:\.|^)\s*(?:on|once|addListener)\(\s*['"]([^'"]+)['"]\s*,\s*(?:function\s+(\w+)|(?:this\.)?(\w+))"#,
        )
        .expect("on regex");
        let emit_re = Regex::new(r#"(?:\.|^)\s*(?:emit|fire|dispatchEvent)\(\s*['"]([^'"]+)['"]"#)
            .expect("emit regex");

        let files = queries.get_all_files().await?;
        let scoped: std::collections::HashSet<&str> = match scope {
            Some(paths) => paths.iter().map(|s| s.as_str()).collect(),
            None => std::collections::HashSet::new(),
        };
        for file in files {
            if !scoped.is_empty() && !scoped.contains(file.path.as_str()) {
                continue;
            }
            if !is_js_family(&file.path) {
                continue;
            }
            let full = project_root.join(&file.path);
            let content = std::fs::read_to_string(&full).unwrap_or_default();
            if content.is_empty() {
                continue;
            }

            let nodes = queries.get_nodes_by_file(&file.path).await?;
            if nodes.is_empty() {
                continue;
            }

            let mut handlers: HashMap<String, Vec<(String, i32)>> = HashMap::new();
            for cap in on_re.captures_iter(&content) {
                let event = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                if event.is_empty() {
                    continue;
                }
                let name = cap
                    .get(2)
                    .or_else(|| cap.get(3))
                    .map(|m| m.as_str())
                    .unwrap_or("");
                if name.is_empty() {
                    continue;
                }
                let line = content[..cap.get(0).unwrap().start()].matches('\n').count() as i32 + 1;
                handlers
                    .entry(event.to_string())
                    .or_default()
                    .push((name.to_string(), line));
            }

            for list in handlers.values_mut() {
                if list.len() > EVENT_FANOUT_CAP {
                    list.clear();
                }
            }
            handlers.retain(|_, list| !list.is_empty());

            for cap in emit_re.captures_iter(&content) {
                let event = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                if event.is_empty() {
                    continue;
                }
                let handlers_for = handlers.get(event);
                if handlers_for.is_none() || handlers_for.unwrap().is_empty() {
                    continue;
                }
                let emit_line =
                    content[..cap.get(0).unwrap().start()].matches('\n').count() as i32 + 1;
                let dispatcher = enclosing_fn(&nodes, emit_line);
                if dispatcher.is_none() {
                    continue;
                }
                let dispatcher = dispatcher.unwrap();
                for (handler_name, _line) in handlers_for.unwrap() {
                    let target = nodes
                        .iter()
                        .find(|n| n.name == *handler_name && is_callable(n));
                    if target.is_none() {
                        continue;
                    }
                    let edge = Edge {
                        source: dispatcher.id.clone(),
                        target: target.unwrap().id.clone(),
                        kind: EdgeKind::Calls,
                        metadata: None,
                        line: Some(emit_line),
                        column: None,
                        provenance: Some(Provenance::Heuristic),
                        confidence: Some(ax_types::EdgeConfidence::Inferred),
                    };
                    queries.upsert_edge(&edge).await?;
                }
            }
        }
        self.synthesize_jsx_render(project_root, queries, scope)
            .await?;
        Ok(())
    }

    /// CG callback-synthesizer.ts `reactJsxChildEdges` (phase 5).
    async fn synthesize_jsx_render(
        &self,
        project_root: &Path,
        queries: &QueryBuilder,
        scope: ResolutionScope<'_>,
    ) -> Result<(), ax_utils::errors::AxError> {
        const MAX_JSX_CHILDREN: usize = 30;
        let tag_re = Regex::new(r"<([A-Z][A-Za-z0-9_]*)[\s/>]").expect("jsx tag");

        let files = queries.get_all_files().await?;
        let scoped: std::collections::HashSet<&str> = match scope {
            Some(paths) => paths.iter().map(|s| s.as_str()).collect(),
            None => std::collections::HashSet::new(),
        };
        // Project-wide lookups, shared by every file: name -> the only
        // component with that name, or None when there are zero or several.
        let mut unique_by_name: HashMap<String, Option<String>> = HashMap::new();
        for file in files {
            if !scoped.is_empty() && !scoped.contains(file.path.as_str()) {
                continue;
            }
            if !is_js_family(&file.path) {
                continue;
            }
            let content = file_source(queries, project_root, &file.path).await?;
            if content.is_empty() || (!content.contains("</") && !content.contains("/>")) {
                continue;
            }

            let nodes = queries.get_nodes_by_file(&file.path).await?;
            let parents: Vec<&Node> = nodes.iter().filter(|n| is_jsx_parent(n)).collect();
            if parents.is_empty() {
                continue;
            }
            let scope_names = file_component_scope(queries, &nodes).await?;

            for parent in parents {
                let src = slice_lines(&content, parent.start_line, parent.end_line);
                if src.is_empty() || (!src.contains("</") && !src.contains("/>")) {
                    continue;
                }
                let mut added = 0;
                let mut seen = std::collections::HashSet::new();
                for cap in tag_re.captures_iter(&src) {
                    if added >= MAX_JSX_CHILDREN {
                        break;
                    }
                    let name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
                    if name.is_empty() || !seen.insert(name.to_string()) {
                        continue;
                    }
                    let child_id = match scope_names.get(name) {
                        Some(id) => Some(id.clone()),
                        None => {
                            if !unique_by_name.contains_key(name) {
                                let found = queries.get_nodes_by_name(name).await?;
                                let mut ids = found.iter().filter(|n| is_jsx_child(n));
                                let only = match (ids.next(), ids.next()) {
                                    (Some(n), None) => Some(n.id.clone()),
                                    _ => None,
                                };
                                unique_by_name.insert(name.to_string(), only);
                            }
                            unique_by_name[name].clone()
                        }
                    };
                    let Some(child_id) = child_id else {
                        continue;
                    };
                    if child_id == parent.id {
                        continue;
                    }
                    let edge = Edge {
                        source: parent.id.clone(),
                        target: child_id,
                        kind: EdgeKind::Calls,
                        metadata: None,
                        line: Some(parent.start_line),
                        column: None,
                        provenance: Some(Provenance::Heuristic),
                        confidence: Some(ax_types::EdgeConfidence::Inferred),
                    };
                    queries.upsert_edge(&edge).await?;
                    added += 1;
                }
            }
        }
        Ok(())
    }
}

/// Source of an indexed file: the stored copy, else the file on disk.
async fn file_source(
    queries: &QueryBuilder,
    project_root: &Path,
    rel: &str,
) -> Result<String, ax_utils::errors::AxError> {
    if let Some(stored) = queries.get_file_content(rel).await? {
        return Ok(stored.content);
    }
    Ok(std::fs::read_to_string(project_root.join(rel)).unwrap_or_default())
}

/// Components a file can name in JSX: its own definitions, then what it imports.
/// A same-file definition wins over an import with the same name.
async fn file_component_scope(
    queries: &QueryBuilder,
    nodes: &[Node],
) -> Result<HashMap<String, String>, ax_utils::errors::AxError> {
    let mut scope: HashMap<String, String> = HashMap::new();
    let mut imported = Vec::new();
    for file_node in nodes.iter().filter(|n| n.kind == NodeKind::File) {
        for e in queries
            .get_outgoing_edges(&file_node.id, Some(&[EdgeKind::Imports]))
            .await?
        {
            imported.push(e.target);
        }
    }
    if !imported.is_empty() {
        for n in queries.get_nodes_by_ids(&imported).await? {
            if is_jsx_child(&n) {
                scope.insert(n.name.clone(), n.id.clone());
            }
        }
    }
    for n in nodes.iter().filter(|n| is_jsx_child(n)) {
        scope.insert(n.name.clone(), n.id.clone());
    }
    Ok(scope)
}

fn is_jsx_child(n: &Node) -> bool {
    matches!(
        n.kind,
        NodeKind::Component | NodeKind::Function | NodeKind::Class
    )
}

fn is_jsx_parent(n: &Node) -> bool {
    matches!(
        n.kind,
        NodeKind::Function | NodeKind::Method | NodeKind::Component
    )
}

fn slice_lines(content: &str, start_line: i32, end_line: i32) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let start = (start_line as usize).saturating_sub(1);
    let end = (end_line as usize).min(lines.len());
    if start >= lines.len() {
        return String::new();
    }
    lines[start..end].join("\n")
}

fn is_callable(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Function | NodeKind::Method)
}

fn enclosing_fn(nodes: &[Node], line: i32) -> Option<&Node> {
    nodes
        .iter()
        .filter(|n| is_callable(n) && line >= n.start_line && line <= n.end_line)
        .max_by_key(|n| n.start_line)
}
