//! React Router route extraction from JSX.

use regex::Regex;

use ax_db::queries::QueryBuilder;
use ax_types::{Language, Node, NodeKind, ReferenceKind, UnresolvedReference};

use crate::types::UnresolvedRef;

use super::extract::{language_for_path, now_ms, stable_node_id, FrameworkExtractResult};

pub fn detect(project_root: &std::path::Path) -> bool {
    let pkg_path = project_root.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            let deps = merge_deps(&json);
            if deps.get("react").is_some()
                || deps.get("next").is_some()
                || deps.get("react-router").is_some()
                || deps.get("react-router-dom").is_some()
            {
                return true;
            }
        }
    }
    false
}

pub fn extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    let is_jsx = file_path.ends_with(".tsx") || file_path.ends_with(".jsx");
    let is_script = file_path.ends_with(".ts")
        || file_path.ends_with(".js")
        || file_path.ends_with(".mts")
        || file_path.ends_with(".mjs")
        || file_path.ends_with(".cjs");
    if !is_jsx && !is_script {
        return FrameworkExtractResult::default();
    }

    let lang = language_for_path(file_path);
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();
    let tag_re = Regex::new(r"<Route\b").expect("route tag regex");
    let path_re = Regex::new(r#"\bpath\s*=\s*["']([^"']+)["']"#).expect("path regex");
    let comp_re = Regex::new(
        r"(?:component|element)\s*=\s*(?:<([A-Z][A-Za-z0-9_]*)|\{([A-Za-z_][A-Za-z0-9_]*)\})",
    )
    .expect("component regex");
    let element_jsx_re =
        Regex::new(r"\belement\s*=\s*\{\s*<\s*([A-Z][A-Za-z0-9_]*)").expect("element jsx regex");
    let data_router_re =
        Regex::new(r"\b(?:createBrowserRouter|createHashRouter|createMemoryRouter|createRoutesFromElements)\b")
            .expect("data router gate");
    let obj_path_re = Regex::new(r#"\bpath\s*:\s*['"]([^'"]*)['"]"#).expect("obj path");
    let data_element_re = Regex::new(r"\belement\s*:\s*<\s*([A-Z][A-Za-z0-9_]*)").expect("data element");
    let data_component_re = Regex::new(r"\bComponent\s*:\s*([A-Z][A-Za-z0-9_]*)").expect("data component");

    if is_jsx {
        for m in tag_re.find_iter(content) {
        let window = &content[m.start()..content.len().min(m.start() + 400)];
        let path_match = path_re.captures(window);
        if path_match.is_none() {
            continue;
        }
        let route_path = path_match.unwrap().get(1).map(|p| p.as_str()).unwrap_or("");
        let line = content[..m.start()].matches('\n').count() as i32 + 1;
        let qualified = format!("{}::ROUTE:{}", file_path, route_path);
        let route_id = stable_node_id(file_path, &qualified);

        out.nodes.push(Node {
            id: route_id.clone(),
            kind: NodeKind::Route,
            name: format!("ROUTE {}", route_path),
            qualified_name: qualified,
            file_path: file_path.to_string(),
            language: lang,
            start_line: line,
            end_line: line,
            start_column: 0,
            end_column: 0,
            docstring: None,
            signature: None,
            visibility: None,
            is_exported: None,
            is_async: None,
            is_static: None,
            is_abstract: None,
            decorators: None,
            type_parameters: None,
            return_type: None,
            updated_at: now,
        });

        let comp_name = comp_re
            .captures(&window)
            .and_then(|cm| cm.get(1).or_else(|| cm.get(2)).map(|c| c.as_str()))
            .or_else(|| {
                element_jsx_re
                    .captures(&window)
                    .and_then(|cm| cm.get(1).map(|c| c.as_str()))
            })
            .unwrap_or("");
        if !comp_name.is_empty() {
            out.references.push(UnresolvedReference {
                from_node_id: route_id,
                reference_name: comp_name.to_string(),
                reference_kind: ReferenceKind::References,
                line,
                column: 0,
                file_path: Some(file_path.to_string()),
                language: Some(lang),
                candidates: None,
            });
        }
        }
    }

    if data_router_re.is_match(content) {
        for cap in obj_path_re.captures_iter(content) {
            let route_path = cap.get(1).map(|m| m.as_str()).unwrap_or("/");
            let win_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
            let win = &content[win_start..content.len().min(win_start + 300)];
            let comp_name = data_element_re
                .captures(win)
                .and_then(|c| c.get(1).map(|m| m.as_str()))
                .or_else(|| {
                    data_component_re
                        .captures(win)
                        .and_then(|c| c.get(1).map(|m| m.as_str()))
                });
            if comp_name.is_none() {
                continue;
            }
            let line = content[..win_start].matches('\n').count() as i32 + 1;
            let qualified = format!("{}::route:{}", file_path, route_path);
            let route_id = stable_node_id(file_path, &qualified);
            out.nodes.push(Node {
                id: route_id.clone(),
                kind: NodeKind::Route,
                name: route_path.to_string(),
                qualified_name: qualified,
                file_path: file_path.to_string(),
                language: lang,
                start_line: line,
                end_line: line,
                start_column: 0,
                end_column: 0,
                docstring: None,
                signature: None,
                visibility: None,
                is_exported: None,
                is_async: None,
                is_static: None,
                is_abstract: None,
                decorators: None,
                type_parameters: None,
                return_type: None,
                updated_at: now,
            });
            out.references.push(UnresolvedReference {
                from_node_id: route_id,
                reference_name: comp_name.unwrap().to_string(),
                reference_kind: ReferenceKind::References,
                line,
                column: 0,
                file_path: Some(file_path.to_string()),
                language: Some(lang),
                candidates: None,
            });
        }
    }

    if is_next_pages_or_app_path(file_path) && content.contains("export default") {
        if let Some(route_path) = file_path_to_route(file_path) {
            let line = content
                .find("export default")
                .map(|i| content[..i].matches('\n').count() as i32 + 1)
                .unwrap_or(1);
            let qualified = format!("{}::route:{}", file_path, route_path);
            let route_id = stable_node_id(file_path, &qualified);
            out.nodes.push(Node {
                id: route_id,
                kind: NodeKind::Route,
                name: route_path,
                qualified_name: qualified,
                file_path: file_path.to_string(),
                language: lang,
                start_line: line,
                end_line: line,
                start_column: 0,
                end_column: 0,
                docstring: None,
                signature: None,
                visibility: None,
                is_exported: None,
                is_async: None,
                is_static: None,
                is_abstract: None,
                decorators: None,
                type_parameters: None,
                return_type: None,
                updated_at: now,
            });
        }
    }

    out
}

fn is_next_pages_or_app_path(file_path: &str) -> bool {
    let normalized = file_path.replace('\\', "/");
    let pages_re = Regex::new(r"(?:^|/)pages/").expect("pages segment");
    let app_re = Regex::new(r"(?:^|/)app/").expect("app segment");
    pages_re.is_match(&normalized) || app_re.is_match(&normalized)
}

/// CG: react.ts `filePathToRoute` (lines 325–369).
fn file_path_to_route(file_path: &str) -> Option<String> {
    let normalized = file_path.replace('\\', "/");
    let base = normalized.rsplit('/').next().unwrap_or("");
    let ext_ok = base.ends_with(".tsx")
        || base.ends_with(".ts")
        || base.ends_with(".jsx")
        || base.ends_with(".js");
    if !ext_ok || base.starts_with('_') || base.contains(".config.") {
        return None;
    }

    let bracket_dyn = Regex::new(r"\[([^\]]+)\]").expect("dynamic segment");
    let strip_ext = Regex::new(r"\.(tsx?|jsx?)$").expect("strip ext");
    let strip_index = Regex::new(r"/index\.(tsx?|jsx?)$").expect("strip index");

    let pages_re = Regex::new(r"(?:^|/)pages/").expect("pages segment");
    if pages_re.is_match(&normalized) {
        let mut route = Regex::new(r"^.*pages/").expect("pages prefix")
            .replace(&normalized, "/")
            .to_string();
        route = strip_index.replace(&route, "").to_string();
        route = strip_ext.replace(&route, "").to_string();
        route = bracket_dyn.replace_all(&route, ":$1").to_string();
        if route.is_empty() {
            return Some("/".to_string());
        }
        return Some(route);
    }

    let app_re = Regex::new(r"(?:^|/)app/").expect("app segment");
    if app_re.is_match(&normalized) {
        if !normalized.contains("page.") {
            return None;
        }
        let mut route = Regex::new(r"^.*app/").expect("app prefix")
            .replace(&normalized, "/")
            .to_string();
        let strip_page = Regex::new(r"/page\.(tsx?|jsx?)$").expect("strip page");
        route = strip_page.replace(&route, "").to_string();
        route = bracket_dyn.replace_all(&route, ":$1").to_string();
        if route.is_empty() {
            return Some("/".to_string());
        }
        return Some(route);
    }

    None
}

fn merge_deps(json: &serde_json::Value) -> std::collections::HashMap<String, serde_json::Value> {
    let mut out = std::collections::HashMap::new();
    if let Some(d) = json.get("dependencies").and_then(|v| v.as_object()) {
        for (k, v) in d {
            out.insert(k.clone(), v.clone());
        }
    }
    if let Some(d) = json.get("devDependencies").and_then(|v| v.as_object()) {
        for (k, v) in d {
            out.insert(k.clone(), v.clone());
        }
    }
    out
}
const BUILT_IN_TYPES: &[&str] = &[
    "Array", "Boolean", "Date", "Error", "Function", "JSON", "Math", "Number",
    "Object", "Promise", "RegExp", "String", "Symbol", "Map", "Set", "WeakMap", "WeakSet",
    "React", "Component", "Fragment", "Suspense", "StrictMode",
];

pub fn is_active_for_ref(ref_: &UnresolvedRef) -> bool {
    matches!(ref_.language, Language::Typescript | Language::Javascript)
}

pub async fn try_resolve_target(
    queries: &QueryBuilder,
    ref_: &UnresolvedRef,
) -> Option<(Node, f64)> {
    if (ref_.language == Language::Typescript || ref_.file_path.ends_with(".tsx") || ref_.file_path.ends_with(".jsx"))
        && is_pascal_case(&ref_.reference_name)
        && !BUILT_IN_TYPES.contains(&ref_.reference_name.as_str())
    {
        if let Some(node) = resolve_component(queries, &ref_.reference_name, &ref_.file_path).await {
            return Some((node, 0.8));
        }
    }

    if ref_.reference_name.starts_with("use") && ref_.reference_name.len() > 3 {
        if let Some(node) = resolve_hook(queries, &ref_.reference_name).await {
            return Some((node, 0.85));
        }
    }

    if ref_.reference_name.ends_with("Context") || ref_.reference_name.ends_with("Provider") {
        if let Some(node) = resolve_context(queries, &ref_.reference_name).await {
            return Some((node, 0.8));
        }
    }

    None
}

async fn resolve_component(
    queries: &QueryBuilder,
    name: &str,
    from_file: &str,
) -> Option<Node> {
    let nodes = queries.get_nodes_by_name(name).await.unwrap_or_default();
    let components: Vec<Node> = nodes
        .into_iter()
        .filter(|n| matches!(n.kind, NodeKind::Component | NodeKind::Function | NodeKind::Class))
        .collect();
    if components.is_empty() {
        return None;
    }

    let from_dir = from_file.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let same_dir: Vec<Node> = components
        .iter()
        .filter(|n| n.file_path.starts_with(from_dir))
        .cloned()
        .collect();
    if let Some(n) = same_dir.first() {
        return Some(n.clone());
    }

    const COMPONENT_DIRS: &[&str] = &[
        "/components/", "/src/components/", "/app/components/",
        "/pages/", "/src/pages/", "/views/", "/src/views/",
    ];
    let preferred: Vec<Node> = components
        .iter()
        .filter(|n| COMPONENT_DIRS.iter().any(|d| n.file_path.contains(d)))
        .cloned()
        .collect();
    if let Some(n) = preferred.first() {
        return Some(n.clone());
    }

    if components.len() == 1 {
        return Some(components[0].clone());
    }
    None
}

async fn resolve_hook(queries: &QueryBuilder, name: &str) -> Option<Node> {
    let nodes = queries.get_nodes_by_name(name).await.unwrap_or_default();
    let hooks: Vec<Node> = nodes
        .into_iter()
        .filter(|n| n.kind == NodeKind::Function && n.name.starts_with("use"))
        .collect();
    if hooks.is_empty() {
        return None;
    }
    const HOOK_DIRS: &[&str] = &["/hooks/", "/src/hooks/", "/lib/hooks/", "/utils/hooks/"];
    let preferred: Vec<Node> = hooks
        .iter()
        .filter(|n| HOOK_DIRS.iter().any(|d| n.file_path.contains(d)))
        .cloned()
        .collect();
    preferred.first().or(hooks.first()).cloned()
}

async fn resolve_context(queries: &QueryBuilder, name: &str) -> Option<Node> {
    let nodes = queries.get_nodes_by_name(name).await.unwrap_or_default();
  if nodes.is_empty() {
        let base = name.trim_end_matches("Context").trim_end_matches("Provider");
        if base != name {
            let base_nodes = queries.get_nodes_by_name(base).await.unwrap_or_default();
            if let Some(n) = base_nodes.first() {
                return Some(n.clone());
            }
        }
        return None;
    }
    const CONTEXT_DIRS: &[&str] = &[
        "/context/", "/contexts/", "/src/context/", "/src/contexts/",
        "/providers/", "/src/providers/",
    ];
    let preferred: Vec<Node> = nodes
        .iter()
        .filter(|n| CONTEXT_DIRS.iter().any(|d| n.file_path.contains(d)))
        .cloned()
        .collect();
    preferred.first().or(nodes.first()).cloned()
}

fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    let first = chars.next();
    first.map(|c| c.is_uppercase() && c.is_alphabetic()).unwrap_or(false)
        && chars.all(|c| c.is_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_app_router_dynamic_segment() {
        let r = extract_file(
            "app/blog/[slug]/page.tsx",
            "export default function BlogPost() { return null; }",
        );
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].name, "/blog/:slug");
    }

    #[test]
    fn next_pages_route() {
        let r = extract_file("pages/about.tsx", "export default function About() {}");
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].name, "/about");
    }

    #[test]
    fn react_data_router_route() {
        let content = r#"import { createBrowserRouter } from "react-router-dom";
export const router = createBrowserRouter([
  { path: "/settings", element: <Settings /> },
]);"#;
        let r = extract_file("router.tsx", content);
        assert!(r.nodes.iter().any(|n| n.name == "/settings"));
    }
}