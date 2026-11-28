//! Shared framework extraction types.

use ax_types::{Edge, Node, UnresolvedReference};

#[derive(Default)]
pub struct FrameworkExtractResult {
    pub nodes: Vec<Node>,
    pub references: Vec<UnresolvedReference>,
    pub edges: Vec<Edge>,
}

impl FrameworkExtractResult {
    /// Drop nodes whose id already appeared, keeping the first. Two nodes with
    /// one id in a single upsert would silently overwrite each other.
    pub fn dedup_nodes(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.nodes.retain(|n| seen.insert(n.id.clone()));
    }
}

pub fn stable_node_id(file_path: &str, qualified_name: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    file_path.hash(&mut hasher);
    qualified_name.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn language_for_path(file_path: &str) -> ax_types::Language {
    if file_path.ends_with(".ts") || file_path.ends_with(".tsx") {
        ax_types::Language::Typescript
    } else if file_path.ends_with(".js")
        || file_path.ends_with(".jsx")
        || file_path.ends_with(".mjs")
    {
        ax_types::Language::Javascript
    } else {
        ax_types::Language::Unknown
    }
}

pub fn is_js_family(path: &str) -> bool {
    path.ends_with(".ts")
        || path.ends_with(".tsx")
        || path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.ends_with(".mjs")
        || path.ends_with(".cjs")
}

#[cfg(test)]
mod tests {
    #[test]
    fn dedup_nodes_keeps_the_first_node_per_id() {
        let mut out = crate::frameworks::go::extract_file(
            "main.go",
            "package main\nfunc r(e *gin.Engine) {\n\te.GET(\"/a\", A)\n\te.GET(\"/a\", A)\n\te.POST(\"/a\", B)\n}\n",
        );
        assert_eq!(out.nodes.len(), 3);
        out.dedup_nodes();
        let names: Vec<_> = out.nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["GET /a", "POST /a"]);
    }
}
