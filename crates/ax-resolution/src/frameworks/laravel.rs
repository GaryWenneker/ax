//! Laravel route extraction + Controller@method resolution — CG: frameworks/laravel.ts

use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_types::{Language, Node, NodeKind, ReferenceKind, UnresolvedReference};

use crate::strip_comments::strip_comments_for_regex;
use crate::types::UnresolvedRef;

use super::extract::{now_ms, stable_node_id, FrameworkExtractResult};

pub fn detect(project_root: &Path) -> bool {
    project_root.join("artisan").exists() || project_root.join("app/Http/Kernel.php").exists()
}

pub fn claims_reference(name: &str) -> bool {
    let re = regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*Controller@\w+$").expect("controller re");
    re.is_match(name)
}

pub fn extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    if !file_path.ends_with(".php") {
        return FrameworkExtractResult::default();
    }

    let safe = strip_comments_for_regex(content, Language::Php);
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();

    let route_re = regex::Regex::new(
        r#"Route::(get|post|put|patch|delete|options|any)\s*\(\s*['"]([^'"]+)['"]\s*,\s*([^)]+)\)"#,
    )
    .expect("route re");
    for cap in route_re.captures_iter(&safe) {
        let method = cap.get(1).map(|m| m.as_str()).unwrap_or("get").to_uppercase();
        let route_path = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let handler_expr = cap.get(3).map(|m| m.as_str()).unwrap_or("");
        let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = safe[..match_start].matches('\n').count() as i32 + 1;
        let qualified = format!("{}::route:{}:{}", file_path, method, route_path);
        let route_id = stable_node_id(file_path, &qualified);

        out.nodes.push(Node {
            id: route_id.clone(),
            kind: NodeKind::Route,
            name: format!("{} {}", method, route_path),
            qualified_name: qualified,
            file_path: file_path.to_string(),
            language: Language::Php,
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

        if let Some(handler_name) = extract_laravel_handler(handler_expr) {
            out.references.push(make_ref(
                &route_id,
                &handler_name,
                ReferenceKind::References,
                line,
                file_path,
            ));
        }
    }

    let resource_re = regex::Regex::new(
        r#"Route::(resource|apiResource)\s*\(\s*['"]([^'"]+)['"]\s*(?:,\s*([^)]+))?\)"#,
    )
    .expect("resource re");
    for cap in resource_re.captures_iter(&safe) {
        let resource_name = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let handler_expr = cap.get(3).map(|m| m.as_str());
        let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = safe[..match_start].matches('\n').count() as i32 + 1;
        let qualified = format!("{}::route:RESOURCE:{}", file_path, resource_name);
        let route_id = stable_node_id(file_path, &qualified);

        out.nodes.push(Node {
            id: route_id.clone(),
            kind: NodeKind::Route,
            name: format!("resource:{}", resource_name),
            qualified_name: qualified,
            file_path: file_path.to_string(),
            language: Language::Php,
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

        if let Some(expr) = handler_expr {
            if let Some(controller_name) = extract_laravel_handler(expr) {
                out.references.push(make_ref(
                    &route_id,
                    &controller_name,
                    ReferenceKind::Imports,
                    line,
                    file_path,
                ));
            }
        }
    }

    out
}

pub async fn try_resolve_target(
    queries: &QueryBuilder,
    project_root: &Path,
    ref_: &UnresolvedRef,
) -> Option<(Node, f64)> {
    let name = &ref_.reference_name;

    if let Some((controller, method)) = parse_controller_method(name) {
        if let Some(node) = resolve_controller_method(queries, project_root, controller, method).await {
            return Some((node, 0.9));
        }
    }

    if let Some((class_name, method_name)) = parse_model_call(name) {
        if let Some(node) = resolve_model_call(queries, project_root, class_name, method_name).await {
            return Some((node, 0.85));
        }
    }

    None
}

fn parse_controller_method(name: &str) -> Option<(&str, &str)> {
    let re = regex::Regex::new(r"^([A-Z][a-zA-Z]+Controller)@(\w+)$").expect("controller@method");
    let cap = re.captures(name)?;
    Some((cap.get(1)?.as_str(), cap.get(2)?.as_str()))
}

fn parse_model_call(name: &str) -> Option<(&str, &str)> {
    let re = regex::Regex::new(r"^([A-Z][a-zA-Z]+)::(\w+)$").expect("model::method");
    let cap = re.captures(name)?;
    Some((cap.get(1)?.as_str(), cap.get(2)?.as_str()))
}

async fn resolve_controller_method(
    queries: &QueryBuilder,
    project_root: &Path,
    controller: &str,
    method: &str,
) -> Option<Node> {
    let controller_path = format!("app/Http/Controllers/{}.php", controller);
    if project_root.join(&controller_path).exists() {
        if let Ok(nodes) = queries.get_nodes_by_file(&controller_path).await {
            if let Some(n) = nodes.iter().find(|n| n.kind == NodeKind::Method && n.name == method) {
                return Some(n.clone());
            }
        }
    }

    if let Ok(candidates) = queries.get_nodes_by_name(controller).await {
        for ctrl in candidates {
            if ctrl.kind == NodeKind::Class && ctrl.file_path.contains("Controllers") {
                if let Ok(nodes) = queries.get_nodes_by_file(&ctrl.file_path).await {
                    if let Some(n) = nodes.iter().find(|n| n.kind == NodeKind::Method && n.name == method) {
                        return Some(n.clone());
                    }
                }
            }
        }
    }

    None
}

async fn resolve_model_call(
    queries: &QueryBuilder,
    project_root: &Path,
    class_name: &str,
    method_name: &str,
) -> Option<Node> {
    let paths = [
        format!("app/Models/{}.php", class_name),
        format!("app/{}.php", class_name),
    ];
    for model_path in paths {
        if !project_root.join(&model_path).exists() {
            continue;
        }
        if let Ok(nodes) = queries.get_nodes_by_file(&model_path).await {
            if let Some(n) = nodes.iter().find(|n| n.kind == NodeKind::Method && n.name == method_name) {
                return Some(n.clone());
            }
            if let Some(n) = nodes.iter().find(|n| n.kind == NodeKind::Class && n.name == class_name) {
                return Some(n.clone());
            }
        }
    }
    None
}

fn short_name(s: &str) -> &str {
    s.rsplit('\\').next().unwrap_or(s)
}

fn extract_laravel_handler(expr: &str) -> Option<String> {
    let trimmed = expr.trim();
    let tuple_re = regex::Regex::new(
        r#"^\[\s*([A-Za-z_\\][\w\\]*)::class\s*,\s*['"]([^'"]+)['"]\s*\]"#,
    )
    .expect("tuple");
    if let Some(cap) = tuple_re.captures(trimmed) {
        let class = short_name(cap.get(1)?.as_str());
        let method = cap.get(2)?.as_str();
        return Some(format!("{}@{}", class, method));
    }

    let at_re = regex::Regex::new(r#"^['"]([^'"]+)@([^'"]+)['"]$"#).expect("at");
    if let Some(cap) = at_re.captures(trimmed) {
        let controller = short_name(cap.get(1)?.as_str());
        let method = cap.get(2)?.as_str();
        return Some(format!("{}@{}", controller, method));
    }

    let class_re = regex::Regex::new(r"^([A-Za-z_\\][\w\\]*)::class").expect("class");
    if let Some(cap) = class_re.captures(trimmed) {
        return Some(short_name(cap.get(1)?.as_str()).to_string());
    }

    None
}

fn make_ref(
    from_node_id: &str,
    reference_name: &str,
    kind: ReferenceKind,
    line: i32,
    file_path: &str,
) -> UnresolvedReference {
    UnresolvedReference {
        from_node_id: from_node_id.to_string(),
        reference_name: reference_name.to_string(),
        reference_kind: kind,
        line,
        column: 0,
        file_path: Some(file_path.to_string()),
        language: Some(Language::Php),
        candidates: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_get_controller_at_method() {
        let content = "Route::get('/users', 'UserController@index');";
        let r = extract_file("routes/web.php", content);
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].name, "GET /users");
        assert_eq!(r.references.len(), 1);
        assert_eq!(r.references[0].reference_name, "UserController@index");
    }

    #[test]
    fn route_get_class_tuple() {
        let content = "Route::post('/users', [UserController::class, 'store']);";
        let r = extract_file("routes/web.php", content);
        assert_eq!(r.references[0].reference_name, "UserController@store");
    }

    #[test]
    fn route_resource_controller_class() {
        let content = "Route::resource('posts', PostController::class);";
        let r = extract_file("routes/web.php", content);
        assert!(r.nodes.iter().any(|n| n.name == "resource:posts"));
        assert!(r.references.iter().any(|r| r.reference_name == "PostController"));
    }

    #[test]
    fn extract_handler_namespaced_tuple() {
        assert_eq!(
            extract_laravel_handler("[App\\Http\\Controllers\\UserController::class, 'show']"),
            Some("UserController@show".to_string())
        );
    }

    #[test]
    fn claims_controller_reference() {
        assert!(claims_reference("UserController@index"));
        assert!(!claims_reference("index"));
    }
}