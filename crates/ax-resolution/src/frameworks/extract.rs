//! Shared framework extraction types.

use ax_types::{Edge, Node, UnresolvedReference};

pub struct FrameworkExtractResult {
    pub nodes: Vec<Node>,
    pub references: Vec<UnresolvedReference>,
    pub edges: Vec<Edge>,
}

impl Default for FrameworkExtractResult {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            references: Vec::new(),
            edges: Vec::new(),
        }
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
    } else if file_path.ends_with(".js") || file_path.ends_with(".jsx") || file_path.ends_with(".mjs") {
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