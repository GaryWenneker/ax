//! Flask + FastAPI route extraction — CG: frameworks/python.ts flaskResolver / fastapiResolver.

use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_types::{Language, Node, NodeKind, ReferenceKind, UnresolvedReference};

use crate::strip_comments::strip_comments_for_regex;
use crate::types::UnresolvedRef;

use super::extract::{now_ms, stable_node_id, FrameworkExtractResult};

const ROUTER_DIRS: &[&str] = &["/routers/", "/api/", "/routes/", "/endpoints/"];
const DEP_DIRS: &[&str] = &["/dependencies/", "/deps/", "/core/"];

pub fn flask_detect(project_root: &Path, file_paths: &[String]) -> bool {
    for file in ["requirements.txt", "pyproject.toml", "Pipfile", "setup.py"] {
        let path = project_root.join(file);
        if let Ok(c) = std::fs::read_to_string(&path) {
            if regex::Regex::new(r"\bflask\b").unwrap().is_match(&c.to_lowercase()) {
                return true;
            }
        }
    }
    let entry_re = regex::Regex::new(r"(?:^|/)(app|application|main|wsgi|__init__)\.py$").expect("entry");
    for fp in file_paths.iter().take(50) {
        if !entry_re.is_match(fp) {
            continue;
        }
        let path = project_root.join(fp);
        if let Ok(c) = std::fs::read_to_string(&path) {
            if c.contains("Flask(")
                && (c.contains("import flask") || c.contains("from flask"))
            {
                return true;
            }
        }
    }
    false
}

pub fn fastapi_detect(project_root: &Path) -> bool {
    for file in ["requirements.txt", "pyproject.toml"] {
        let path = project_root.join(file);
        if let Ok(c) = std::fs::read_to_string(&path) {
            if regex::Regex::new(r"\bfastapi\b").unwrap().is_match(&c.to_lowercase()) {
                return true;
            }
        }
    }
    for file in ["app.py", "main.py", "api.py"] {
        let path = project_root.join(file);
        if let Ok(c) = std::fs::read_to_string(&path) {
            if c.contains("FastAPI(") {
                return true;
            }
        }
    }
    false
}

pub fn flask_extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    if !file_path.ends_with(".py") {
        return FrameworkExtractResult::default();
    }
    let safe = strip_comments_for_regex(content, Language::Python);
    let mut out = extract_decorator_routes(
        file_path,
        &safe,
        r#"@(\w+)\.route\s*\(\s*['"]([^'"]*)['"](?:\s*,\s*methods\s*=\s*(?:\[|\()([^\])]+)(?:\]|\)))?\s*\)"#,
        "GET",
        true,
        DecoratorMode::FlaskMethods,
    );
    merge_extract(&mut out, extract_flask_restful(file_path, &safe));
    out
}

pub fn fastapi_extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    if !file_path.ends_with(".py") {
        return FrameworkExtractResult::default();
    }
    let safe = strip_comments_for_regex(content, Language::Python);
    extract_decorator_routes(
        file_path,
        &safe,
        r#"@(\w+)\.(get|post|put|patch|delete|options|head)\s*\(\s*['"]([^'"]*)['"]"#,
        "",
        true,
        DecoratorMode::FastapiMethod,
    )
}

pub async fn try_resolve_target(queries: &QueryBuilder, ref_: &UnresolvedRef) -> Option<(Node, f64)> {
    let name = &ref_.reference_name;
    if name.ends_with("_bp") || name.ends_with("_blueprint") {
        if let Some(n) = resolve_by_kind(queries, name, &[NodeKind::Variable], &[]).await {
            return Some((n, 0.8));
        }
    }
    if name.ends_with("_router") || name == "router" {
        if let Some(n) = resolve_by_kind(queries, name, &[NodeKind::Variable], ROUTER_DIRS).await {
            return Some((n, 0.8));
        }
    }
    if name.starts_with("get_") || name.starts_with("Depends") {
        if let Some(n) = resolve_by_kind(queries, name, &[NodeKind::Function], DEP_DIRS).await {
            return Some((n, 0.75));
        }
    }
    None
}

enum DecoratorMode {
    FlaskMethods,
    FastapiMethod,
}

fn extract_decorator_routes(
    file_path: &str,
    safe: &str,
    pattern: &str,
    default_method: &str,
    find_handler: bool,
    mode: DecoratorMode,
) -> FrameworkExtractResult {
    let re = regex::Regex::new(pattern).expect("decorator re");
    let def_re = regex::Regex::new(r"\n\s*(?:async\s+)?def\s+(\w+)").expect("def re");
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();

    for cap in re.captures_iter(safe) {
        let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = safe[..match_start].matches('\n').count() as i32 + 1;

        let (method, route_path) = match mode {
            DecoratorMode::FlaskMethods => {
                let path = cap.get(2).map(|m| m.as_str()).unwrap_or("");
                let mut method = default_method.to_string();
                if let Some(mg) = cap.get(3) {
                    let method_re = regex::Regex::new(r#"['"]([A-Z]+)['"]"#).expect("method");
                    if let Some(m) = method_re.captures(mg.as_str()) {
                        method = m.get(1).map(|x| x.as_str().to_uppercase()).unwrap_or(method);
                    }
                }
                (method, path.to_string())
            }
            DecoratorMode::FastapiMethod => {
                let method = cap
                    .get(2)
                    .map(|m| m.as_str().to_uppercase())
                    .unwrap_or_default();
                let path = cap.get(3).map(|m| m.as_str()).unwrap_or("").to_string();
                (method, path)
            }
        };

        let name = if method.is_empty() {
            route_path.clone()
        } else {
            format!("{} {}", method, route_path)
        };
        let qualified = format!("{}::{}:{}", file_path, method, route_path);
        let route_id = stable_node_id(file_path, &qualified);

        out.nodes.push(Node {
            id: route_id.clone(),
            kind: NodeKind::Route,
            name,
            qualified_name: qualified,
            file_path: file_path.to_string(),
            language: Language::Python,
            start_line: line,
            end_line: line,
            start_column: 0,
            end_column: cap.get(0).map(|m| m.as_str().len() as i32).unwrap_or(0),
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

        if find_handler {
            let tail = &safe[match_start + cap.get(0).map(|m| m.len()).unwrap_or(0)..];
            if let Some(def_cap) = def_re.captures(tail) {
                let handler = def_cap.get(1).map(|m| m.as_str()).unwrap_or("");
                if !handler.is_empty() {
                    out.references.push(make_ref(&route_id, handler, line, file_path));
                }
            }
        }
    }

    out
}

fn extract_flask_restful(file_path: &str, safe: &str) -> FrameworkExtractResult {
    let re = regex::Regex::new(r#"\.add\w*[Rr]esource\s*\(\s*(\w+)\s*,\s*((?:['"][^'"]+['"]\s*,?\s*)+)"#)
        .expect("restful re");
    let path_re = regex::Regex::new(r#"['"]([^'"]+)['"]"#).expect("path re");
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();

    for cap in re.captures_iter(safe) {
        let class_name = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let paths_blob = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = safe[..match_start].matches('\n').count() as i32 + 1;

        for path_cap in path_re.captures_iter(paths_blob) {
            let route_path = path_cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let qualified = format!("{}::ANY:{}", file_path, route_path);
            let route_id = stable_node_id(file_path, &qualified);

            out.nodes.push(Node {
                id: route_id.clone(),
                kind: NodeKind::Route,
                name: format!("ANY {}", route_path),
                qualified_name: qualified,
                file_path: file_path.to_string(),
                language: Language::Python,
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

            out.references.push(make_ref(&route_id, class_name, line, file_path));
        }
    }

    out
}

async fn resolve_by_kind(
    queries: &QueryBuilder,
    name: &str,
    kinds: &[NodeKind],
    dirs: &[&str],
) -> Option<Node> {
    let nodes = queries.get_nodes_by_name(name).await.unwrap_or_default();
    let filtered: Vec<Node> = nodes.into_iter().filter(|n| kinds.contains(&n.kind)).collect();
    if filtered.is_empty() {
        return None;
    }
    if dirs.is_empty() {
        return filtered.first().cloned();
    }
    let preferred: Vec<Node> = filtered
        .iter()
        .filter(|n| dirs.iter().any(|d| n.file_path.contains(d)))
        .cloned()
        .collect();
    preferred.first().or_else(|| filtered.first()).cloned()
}

fn merge_extract(dst: &mut FrameworkExtractResult, src: FrameworkExtractResult) {
    dst.nodes.extend(src.nodes);
    dst.references.extend(src.references);
    dst.edges.extend(src.edges);
}

fn make_ref(from_node_id: &str, reference_name: &str, line: i32, file_path: &str) -> UnresolvedReference {
    UnresolvedReference {
        from_node_id: from_node_id.to_string(),
        reference_name: reference_name.to_string(),
        reference_kind: ReferenceKind::References,
        line,
        column: 0,
        file_path: Some(file_path.to_string()),
        language: Some(Language::Python),
        candidates: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flask_route_decorator() {
        let content = "@app.route('/hello', methods=['GET'])\ndef hello(): pass";
        let r = flask_extract_file("app.py", content);
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].name, "GET /hello");
        assert_eq!(r.references[0].reference_name, "hello");
    }

    #[test]
    fn fastapi_get_decorator() {
        let content = "@router.get('/items')\nasync def list_items(): pass";
        let r = fastapi_extract_file("api.py", content);
        assert_eq!(r.nodes[0].name, "GET /items");
        assert_eq!(r.references[0].reference_name, "list_items");
    }

    #[test]
    fn flask_restful_add_resource() {
        let content = "api.add_resource(UserResource, '/users', '/users/<id>')";
        let r = flask_extract_file("app.py", content);
        assert_eq!(r.nodes.len(), 2);
        assert!(r.references.iter().all(|ref_| ref_.reference_name == "UserResource"));
    }
}