//! Svelte / SvelteKit route extraction + component resolution - CG: frameworks/svelte.ts.

use std::collections::HashMap;
use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_types::{Language, Node, NodeKind, ReferenceKind};

use crate::types::UnresolvedRef;

use super::extract::{is_js_family, now_ms, stable_node_id, FrameworkExtractResult};

const SVELTE_RUNES: &[&str] = &[
    "$state", "$state.raw", "$state.snapshot", "$derived", "$derived.by",
    "$effect", "$effect.pre", "$effect.root", "$effect.tracking",
    "$props", "$bindable", "$inspect", "$host",
];

const SVELTEKIT_MODULE_PREFIXES: &[&str] = &[
    "$app/navigation", "$app/stores", "$app/environment", "$app/forms", "$app/paths",
    "$env/static/private", "$env/static/public", "$env/dynamic/private", "$env/dynamic/public",
];

fn sveltekit_route_files() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("+page.svelte", "page"),
        ("+page.ts", "page-load"),
        ("+page.js", "page-load"),
        ("+page.server.ts", "page-server-load"),
        ("+page.server.js", "page-server-load"),
        ("+layout.svelte", "layout"),
        ("+layout.ts", "layout-load"),
        ("+layout.js", "layout-load"),
        ("+layout.server.ts", "layout-server-load"),
        ("+layout.server.js", "layout-server-load"),
        ("+server.ts", "api-endpoint"),
        ("+server.js", "api-endpoint"),
        ("+error.svelte", "error-page"),
    ])
}

pub fn detect(project_root: &Path, file_paths: &[String]) -> bool {
    let pkg_path = project_root.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            let mut keys = Vec::new();
            if let Some(d) = json.get("dependencies").and_then(|v| v.as_object()) {
                keys.extend(d.keys());
            }
            if let Some(d) = json.get("devDependencies").and_then(|v| v.as_object()) {
                keys.extend(d.keys());
            }
            if keys.iter().any(|k| *k == "svelte" || *k == "@sveltejs/kit") {
                return true;
            }
        }
    }
    file_paths.iter().any(|f| f.ends_with(".svelte"))
}

pub fn extract_file(file_path: &str, _content: &str) -> FrameworkExtractResult {
    let mut out = FrameworkExtractResult::default();
    let file_name = file_path.rsplit('/').next().unwrap_or(file_path);
    if !sveltekit_route_files().contains_key(file_name) {
        return out;
    }
    let route_path = file_path_to_sveltekit_route(file_path);
    if route_path.is_none() {
        return out;
    }
    let route_path = route_path.unwrap();
    let now = now_ms();
    let normalized = file_path.replace('\\', "/");
    let lang = if normalized.ends_with(".svelte") { Language::Svelte }
        else if is_js_family(file_path) { Language::Typescript }
        else { Language::Unknown };
    let qualified = format!("{}::route:{}", file_path, route_path);
    out.nodes.push(Node {
        id: stable_node_id(file_path, &qualified),
        kind: NodeKind::Route,
        name: route_path,
        qualified_name: qualified,
        file_path: file_path.to_string(),
        language: lang,
        start_line: 1, end_line: 1, start_column: 0, end_column: 0,
        docstring: None, signature: None, visibility: None, is_exported: None,
        is_async: None, is_static: None, is_abstract: None, decorators: None,
        type_parameters: None, return_type: None, updated_at: now,
    });
    out
}

pub async fn try_resolve_target(
    queries: &QueryBuilder, project_root: &Path, ref_: &UnresolvedRef,
) -> Option<(ax_types::Node, f64)> {
    let name = &ref_.reference_name;

    if is_rune_reference(name) {
        return resolve_self_node(queries, ref_, 1.0).await;
    }

    if name.starts_with('$') && !name.starts_with("$$") {
        let store_name = name.trim_start_matches('$');
        let candidates = queries.get_nodes_by_name(store_name).await.unwrap_or_default();
        if let Some(n) = candidates.into_iter().find(|n| {
            n.kind == NodeKind::Variable || n.kind == NodeKind::Constant
        }) {
            return Some((n, 0.85));
        }
    }

    if ref_.reference_kind == ReferenceKind::Imports && name.starts_with('$') {
        if name.starts_with("$lib/") {
            if let Some(node) = resolve_lib_import(project_root, queries, name).await {
                return Some((node, 0.9));
            }
        }
        if SVELTEKIT_MODULE_PREFIXES.iter().any(|p| name.starts_with(p)) {
            return resolve_self_node(queries, ref_, 1.0).await;
        }
    }

    if ref_.reference_kind == ReferenceKind::Calls && is_pascal_case(name) {
        if let Some(node) = resolve_component(queries, name, &ref_.file_path).await {
            return Some((node, 0.8));
        }
    }

    None
}

async fn resolve_self_node(
    queries: &QueryBuilder, ref_: &UnresolvedRef, conf: f64,
) -> Option<(ax_types::Node, f64)> {
    if let Ok(nodes) = queries.get_nodes_by_file(&ref_.file_path).await {
        if let Some(n) = nodes.iter().find(|n| n.id == ref_.from_node_id) {
            return Some((n.clone(), conf));
        }
    }
    None
}

fn is_rune_reference(name: &str) -> bool {
    SVELTE_RUNES.contains(&name) || name == "$state" || name == "$derived" || name == "$effect"
}

fn file_path_to_sveltekit_route(file_path: &str) -> Option<String> {
    let normalized = file_path.replace('\\', "/");
    let routes_re = regex::Regex::new(r"(?:^|/)routes/").expect("routes segment");
    let m = routes_re.find(&normalized)?;
    let after_routes = &normalized[m.end()..];
    let dir_path = after_routes.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let catch_all = regex::Regex::new(r"\[\.\.\.([^\]]+)\]").expect("catch-all");
    let optional = regex::Regex::new(r"\[\[([^\]]+)\]\]").expect("optional");
    let dynamic = regex::Regex::new(r"\[([^\]]+)\]").expect("dynamic");
    let mut route = format!("/{}", dir_path);
    route = catch_all.replace_all(&route, "*$1").to_string();
    route = optional.replace_all(&route, ":$1?").to_string();
    route = dynamic.replace_all(&route, ":$1").to_string();
    if route == "/" { return Some("/".to_string()); }
    route.trim_end_matches('/').to_string().into()
}

async fn resolve_lib_import(
    project_root: &Path, queries: &QueryBuilder, reference_name: &str,
) -> Option<ax_types::Node> {
    if !reference_name.starts_with("$lib/") { return None; }
    let lib_path = reference_name.replacen("$lib/", "src/lib/", 1);
    for ext in ["", ".ts", ".js", ".svelte", "/index.ts", "/index.js"] {
        let full_path = format!("{}{}", lib_path, ext);
        if !project_root.join(&full_path).exists() { continue; }
        if let Ok(nodes) = queries.get_nodes_by_file(&full_path).await {
            if let Some(n) = nodes.first() { return Some(n.clone()); }
        }
    }
    None
}

async fn resolve_component(
    queries: &QueryBuilder, name: &str, from_file: &str,
) -> Option<ax_types::Node> {
    let candidates = queries.get_nodes_by_name(name).await.unwrap_or_default();
    let components: Vec<_> = candidates.into_iter()
        .filter(|n| n.kind == NodeKind::Component)
        .collect();
    if components.is_empty() { return None; }
    let from_dir = from_file.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let same_dir: Vec<_> = components.iter().filter(|n| n.file_path.starts_with(from_dir)).collect();
    if let Some(n) = same_dir.first() { return Some((*n).clone()); }
    if components.len() == 1 { return Some(components[0].clone()); }
    None
}

fn is_pascal_case(s: &str) -> bool {
    let mut chars = s.chars();
    let first = chars.next();
    first.map(|c| c.is_uppercase() && c.is_alphabetic()).unwrap_or(false)
        && chars.all(|c| c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sveltekit_page_route() {
        let r = extract_file("src/routes/blog/+page.svelte", "");
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].name, "/blog");
    }
    #[test]
    fn sveltekit_dynamic_route() {
        let r = extract_file("src/routes/blog/[slug]/+page.svelte", "");
        assert_eq!(r.nodes[0].name, "/blog/:slug");
    }
    #[test]
    fn sveltekit_root_route() {
        let r = extract_file("src/routes/+page.svelte", "");
        assert_eq!(r.nodes[0].name, "/");
    }
    #[test]
    fn sveltekit_catch_all_route() {
        assert_eq!(
            file_path_to_sveltekit_route("src/routes/docs/[...path]/+page.svelte"),
            Some("/docs/*path".to_string())
        );
    }
}