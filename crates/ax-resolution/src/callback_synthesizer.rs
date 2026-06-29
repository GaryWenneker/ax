//! Callback / EventEmitter edge synthesis (emit/on pairs).

use std::collections::HashMap;
use std::path::Path;

use regex::Regex;

use ax_db::queries::QueryBuilder;
use ax_types::{Edge, EdgeKind, Node, NodeKind, Provenance};

use crate::frameworks::extract::is_js_family;

const EVENT_FANOUT_CAP: usize = 6;

pub struct CallbackSynthesizer;

impl CallbackSynthesizer {
    pub fn new() -> Self {
        Self
    }

    pub async fn synthesize(
        &self,
        project_root: &Path,
        queries: &QueryBuilder,
    ) -> Result<(), ax_utils::errors::AxError> {
        let on_re = Regex::new(
            r#"(?:\.|^)\s*(?:on|once|addListener)\(\s*['"]([^'"]+)['"]\s*,\s*(?:function\s+(\w+)|(?:this\.)?(\w+))"#,
        )
        .expect("on regex");
        let emit_re = Regex::new(r#"(?:\.|^)\s*(?:emit|fire|dispatchEvent)\(\s*['"]([^'"]+)['"]"#)
            .expect("emit regex");

        let files = queries.get_all_files().await?;
        for file in files {
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
                let line = content[..cap.get(0).unwrap().start()]
                    .matches('\n')
                    .count() as i32
                    + 1;
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
                let emit_line = content[..cap.get(0).unwrap().start()]
                    .matches('\n')
                    .count() as i32
                    + 1;
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
                    };
                    queries.upsert_edge(&edge).await?;
                }
            }
        }
        self.synthesize_jsx_render(project_root, queries).await?;
        Ok(())
    }

    /// CG callback-synthesizer.ts `reactJsxChildEdges` (phase 5).
    async fn synthesize_jsx_render(
        &self,
        project_root: &Path,
        queries: &QueryBuilder,
    ) -> Result<(), ax_utils::errors::AxError> {
        const MAX_JSX_CHILDREN: usize = 30;
        let tag_re = Regex::new(r"<([A-Z][A-Za-z0-9_]*)[\s/>]").expect("jsx tag");

        let files = queries.get_all_files().await?;
        for file in files {
            if !is_js_family(&file.path) {
                continue;
            }
            let full = project_root.join(&file.path);
            let content = std::fs::read_to_string(&full).unwrap_or_default();
            if content.is_empty() || (!content.contains("</") && !content.contains("/>")) {
                continue;
            }

            let nodes = queries.get_nodes_by_file(&file.path).await?;
            let parents: Vec<&Node> = nodes
                .iter()
                .filter(|n| is_jsx_parent(n))
                .collect();
            if parents.is_empty() {
                continue;
            }

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
                    if name.is_empty() {
                        continue;
                    }
                    let child_nodes = queries.get_nodes_by_name(name).await.unwrap_or_default();
                    let child = child_nodes
                        .iter()
                        .find(|n| matches!(n.kind, NodeKind::Component | NodeKind::Function | NodeKind::Class));
                    if child.is_none() || child.unwrap().id == parent.id {
                        continue;
                    }
                    let child = child.unwrap();
                    let key = format!("{}>{}", parent.id, child.id);
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.insert(key);
                    let edge = Edge {
                        source: parent.id.clone(),
                        target: child.id.clone(),
                        kind: EdgeKind::Calls,
                        metadata: None,
                        line: Some(parent.start_line),
                        column: None,
                        provenance: Some(Provenance::Heuristic),
                    };
                    queries.upsert_edge(&edge).await?;
                    added += 1;
                }
            }
        }
        Ok(())
    }
}

fn is_jsx_parent(n: &Node) -> bool {
    matches!(n.kind, NodeKind::Function | NodeKind::Method | NodeKind::Component)
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