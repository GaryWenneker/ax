//! Vue / Nuxt route extraction + component resolution - CG: frameworks/vue.ts.

use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_types::{Language, Node, NodeKind, ReferenceKind};

use crate::types::UnresolvedRef;

use super::extract::{is_js_family, now_ms, stable_node_id, FrameworkExtractResult};

const VUE_COMPILER_MACROS: &[&str] = &[
    "defineProps", "defineEmits", "defineExpose", "defineOptions",
    "defineSlots", "defineModel", "withDefaults",
];

const NUXT_AUTO_IMPORTS: &[&str] = &[
    "useRoute", "useRouter", "navigateTo", "useFetch", "useAsyncData",
    "useState", "useHead", "useRuntimeConfig", "useNuxtApp",
    "definePageMeta", "defineNuxtConfig",
];

const NUXT_VIRTUAL_PREFIXES: &[&str] = &["#imports", "#components", "#app", "#build", "#head"];

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
            if keys.iter().any(|k| *k == "vue" || *k == "nuxt" || *k == "@nuxt/kit") {
                return true;
            }
        }
    }
    file_paths.iter().any(|f| f.ends_with(".vue"))
}

pub fn extract_file(file_path: &str, _content: &str) -> FrameworkExtractResult {
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();
    let normalized = file_path.replace('\\', "/");

    let pages_re = regex::Regex::new(r"(?:^|/)pages/").expect("pages segment");
    if let Some(m) = pages_re.find(&normalized) {
        if normalized.ends_with(".vue") {
            let after_pages = m.end();
            if let Some(route_path) = file_path_to_nuxt_route(&normalized, after_pages) {
                let qualified = format!("{}::route:{}", file_path, route_path);
                out.nodes.push(make_route_node(
                    file_path, &stable_node_id(file_path, &qualified),
                    &route_path, &qualified, Language::Vue, now,
                ));
            }
        }
    }

    let api_re = regex::Regex::new(r"(?:^|/)server/api/").expect("api segment");
    if let Some(m) = api_re.find(&normalized) {
        let after_api = &normalized[m.end()..];
        let mut route_name = after_api
            .rsplit_once('.').map(|(s, _)| s).unwrap_or(after_api).to_string();
        if let Some((base, method)) = route_name.rsplit_once('.') {
            if matches!(method, "get" | "post" | "put" | "delete" | "patch" | "head" | "options") {
                route_name = base.to_string();
            }
        }
        route_name = route_name.trim_end_matches("/index").to_string();
        let api_route = format!("/api/{}", route_name);
        let qualified = format!("{}::route:{}", file_path, api_route);
        let lang = if normalized.ends_with(".vue") { Language::Vue }
            else if is_js_family(file_path) { language_for_vue_sidecar(file_path) }
            else { Language::Unknown };
        out.nodes.push(make_route_node(
            file_path, &stable_node_id(file_path, &qualified),
            &api_route, &qualified, lang, now,
        ));
    }

    let mw_re = regex::Regex::new(r"(?:^|/)middleware/").expect("middleware segment");
    if let Some(m) = mw_re.find(&normalized) {
        let after = &normalized[m.end()..];
        let name = after.rsplit_once('.').map(|(s, _)| s).unwrap_or(after);
        let qualified = format!("{}::middleware:{}", file_path, name);
        let lang = if normalized.ends_with(".vue") { Language::Vue }
            else if is_js_family(file_path) { language_for_vue_sidecar(file_path) }
            else { Language::Unknown };
        out.nodes.push(Node {
            id: stable_node_id(file_path, &qualified),
            kind: NodeKind::Function, name: name.to_string(),
            qualified_name: qualified, file_path: file_path.to_string(),
            language: lang, start_line: 1, end_line: 1,
            start_column: 0, end_column: 0, docstring: None, signature: None,
            visibility: None, is_exported: None, is_async: None, is_static: None,
            is_abstract: None, decorators: None, type_parameters: None,
            return_type: None, updated_at: now,
        });
    }
    out
}

pub async fn try_resolve_target(
    queries: &QueryBuilder, project_root: &Path, ref_: &UnresolvedRef,
) -> Option<(ax_types::Node, f64)> {
    let name = &ref_.reference_name;
    if VUE_COMPILER_MACROS.contains(&name.as_str()) || NUXT_AUTO_IMPORTS.contains(&name.as_str()) {
        return resolve_self_node(queries, ref_, 1.0).await;
    }
    if ref_.reference_kind == ReferenceKind::Imports && name.starts_with('#') {
        if NUXT_VIRTUAL_PREFIXES.iter().any(|p| name.starts_with(p)) {
            return resolve_self_node(queries, ref_, 1.0).await;
        }
    }
    if ref_.reference_kind == ReferenceKind::Imports {
        if let Some(node) = resolve_alias_import(project_root, queries, name, "@/").await {
            return Some((node, 0.9));
        }
        if let Some(node) = resolve_alias_import(project_root, queries, name, "~/").await {
            return Some((node, 0.9));
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

fn make_route_node(file_path: &str, id: &str, name: &str, qualified: &str, language: Language, now: i64) -> Node {
    Node {
        id: id.to_string(), kind: NodeKind::Route, name: name.to_string(),
        qualified_name: qualified.to_string(), file_path: file_path.to_string(),
        language, start_line: 1, end_line: 1, start_column: 0, end_column: 0,
        docstring: None, signature: None, visibility: None, is_exported: None,
        is_async: None, is_static: None, is_abstract: None, decorators: None,
        type_parameters: None, return_type: None, updated_at: now,
    }
}

fn language_for_vue_sidecar(file_path: &str) -> Language {
    if file_path.ends_with(".ts") || file_path.ends_with(".mts") { Language::Typescript }
    else { Language::Javascript }
}

fn file_path_to_nuxt_route(normalized: &str, after_pages_start: usize) -> Option<String> {
    let after_pages = normalized.get(after_pages_start..)?;
    let without_ext = after_pages.trim_end_matches(".vue");
    let without_index = if without_ext == "index" { "" } else { without_ext.trim_end_matches("/index") };
    let catch_all = regex::Regex::new(r"\[\.\.\.([^\]]+)\]").expect("catch-all");
    let optional = regex::Regex::new(r"\[\[([^\]]+)\]\]").expect("optional");
    let dynamic = regex::Regex::new(r"\[([^\]]+)\]").expect("dynamic");
    let mut route = format!("/{}", without_index);
    route = catch_all.replace_all(&route, "*$1").to_string();
    route = optional.replace_all(&route, ":$1?").to_string();
    route = dynamic.replace_all(&route, ":$1").to_string();
    if route == "/" { return Some("/".to_string()); }
    route.trim_end_matches('/').to_string().into()
}

async fn resolve_alias_import(
    project_root: &Path, queries: &QueryBuilder, reference_name: &str, prefix: &str,
) -> Option<ax_types::Node> {
    if !reference_name.starts_with(prefix) { return None; }
    let alias_path = reference_name.replacen(prefix, "src/", 1);
    for ext in ["", ".ts", ".js", ".vue", "/index.ts", "/index.js", "/index.vue"] {
        let full_path = format!("{}{}", alias_path, ext);
        if !project_root.join(&full_path).exists() { continue; }
        if let Ok(nodes) = queries.get_nodes_by_file(&full_path).await {
            if let Some(n) = nodes.first() { return Some(n.clone()); }
        }
    }
    None
}

async fn resolve_component(queries: &QueryBuilder, name: &str, from_file: &str) -> Option<ax_types::Node> {
    let candidates = queries.get_nodes_by_name(name).await.unwrap_or_default();
    let vue_components: Vec<_> = candidates.into_iter()
        .filter(|n| n.kind == NodeKind::Component && n.file_path.ends_with(".vue"))
        .collect();
    if vue_components.is_empty() { return None; }
    let from_dir = from_file.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let same_dir: Vec<_> = vue_components.iter().filter(|n| n.file_path.starts_with(from_dir)).collect();
    if let Some(n) = same_dir.first() { return Some((*n).clone()); }
    if vue_components.len() == 1 { return Some(vue_components[0].clone()); }
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
    fn nuxt_pages_index_route() {
        let r = extract_file("pages/index.vue", "");
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].name, "/");
    }
    #[test]
    fn nuxt_pages_dynamic_route() {
        let r = extract_file("pages/users/[id].vue", "");
        assert_eq!(r.nodes[0].name, "/users/:id");
    }
    #[test]
    fn nuxt_api_route() {
        let r = extract_file("server/api/hello.get.ts", "");
        assert!(r.nodes.iter().any(|n| n.name == "/api/hello"));
    }
    #[test]
    fn nuxt_catch_all_route() {
        let normalized = "pages/blog/[...slug].vue";
        let pages_re = regex::Regex::new(r"(?:^|/)pages/").unwrap();
        let after = pages_re.find(normalized).unwrap().end();
        assert_eq!(file_path_to_nuxt_route(normalized, after), Some("/blog/*slug".to_string()));
    }
}