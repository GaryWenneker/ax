//! C/C++ function-pointer registration synthesis (CodeGraph c-fnptr-synthesizer MVP).

use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_types::{Edge, EdgeKind, NodeKind, Provenance};
use regex::Regex;

const C_CPP_EXT: &[&str] = &[
    ".c", ".h", ".cc", ".cpp", ".cxx", ".hpp", ".hh", ".hxx", ".cppm", ".ipp", ".inl", ".tcc",
];

pub struct CFnptrSynthesizer;

impl CFnptrSynthesizer {
    pub fn new() -> Self {
        Self
    }

    pub async fn synthesize(
        &self,
        project_root: &Path,
        queries: &QueryBuilder,
    ) -> Result<(), ax_utils::errors::AxError> {
        let assign_re = Regex::new(r"(?:\.(\w+)|(\w+))\s*=\s*(\w+)\s*;").unwrap();
        let files = queries.get_all_files().await?;
        for file in files {
            if !is_c_cpp(&file.path) {
                continue;
            }
            let full = project_root.join(&file.path);
            let content = std::fs::read_to_string(&full).unwrap_or_default();
            if content.is_empty() {
                continue;
            }
            let file_nodes = queries.get_nodes_by_file(&file.path).await.unwrap_or_default();
            let func_ids: std::collections::HashMap<String, String> = file_nodes
                .iter()
                .filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method))
                .map(|n| (n.name.clone(), n.id.clone()))
                .collect();
            if func_ids.is_empty() {
                continue;
            }
            let file_id = stable_file_id(&file.path);
            for cap in assign_re.captures_iter(&content) {
                let rhs = cap.get(3).map(|m| m.as_str()).unwrap_or("");
                if rhs.is_empty() || !func_ids.contains_key(rhs) {
                    continue;
                }
                let line = content[..cap.get(0).unwrap().start()]
                    .chars()
                    .filter(|c| *c == '\n')
                    .count() as i32
                    + 1;
                let from_id = enclosing_node_at_line(&file_nodes, line).unwrap_or_else(|| file_id.clone());
                let target_id = func_ids[rhs].clone();
                let edge = Edge {
                    source: from_id,
                    target: target_id,
                    kind: EdgeKind::References,
                    metadata: None,
                    line: Some(line),
                    column: None,
                    provenance: Some(Provenance::Heuristic),
                };
                queries.upsert_edge(&edge).await?;
            }
        }
        Ok(())
    }
}

fn is_c_cpp(path: &str) -> bool {
    C_CPP_EXT.iter().any(|ext| path.ends_with(ext))
}

fn stable_file_id(file_path: &str) -> String {
    crate::frameworks::extract::stable_node_id(file_path, file_path)
}

fn enclosing_node_at_line(nodes: &[ax_types::Node], line: i32) -> Option<String> {
    nodes
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Function | NodeKind::Method | NodeKind::Class))
        .filter(|n| line >= n.start_line && line <= n.end_line)
        .max_by_key(|n| n.start_line)
        .map(|n| n.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_cpp_extension_gate() {
        assert!(is_c_cpp("src/main.c"));
        assert!(is_c_cpp("hdr.hpp"));
        assert!(!is_c_cpp("main.ts"));
    }
}
