//! Django URL routing extraction + view/model resolution — CG: frameworks/python.ts djangoResolver.

use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_types::{Language, Node, NodeKind, ReferenceKind, UnresolvedReference};

use crate::strip_comments::strip_comments_for_regex;
use crate::types::UnresolvedRef;

use super::extract::{now_ms, stable_node_id, FrameworkExtractResult};

const MODEL_DIRS: &[&str] = &["models", "app/models", "src/models"];
const VIEW_DIRS: &[&str] = &["views", "app/views", "src/views", "api/views"];
const FORM_DIRS: &[&str] = &["forms", "app/forms", "src/forms"];

pub fn detect(project_root: &Path) -> bool {
    if project_root.join("manage.py").exists() {
        return true;
    }
    for file in ["requirements.txt", "setup.py", "pyproject.toml"] {
        let path = project_root.join(file);
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content.to_lowercase().contains("django") {
                return true;
            }
        }
    }
    false
}

pub fn claims_reference(name: &str) -> bool {
    name == "_iterable_class" || name.ends_with(".urls")
}

pub fn extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    if !file_path.ends_with(".py") {
        return FrameworkExtractResult::default();
    }

    let safe = strip_comments_for_regex(content, Language::Python);
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();

    let route_re = regex::Regex::new(
        r#"\b(path|re_path|url)\s*\(\s*r?['"]([^'"]+)['"]\s*,\s*([\w.]+(?:\s*\([^)]*\))?)"#,
    )
    .expect("django route re");

    for cap in route_re.captures_iter(&safe) {
        let url_path = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let handler_expr = cap.get(3).map(|m| m.as_str()).unwrap_or("");
        let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = safe[..match_start].matches('\n').count() as i32 + 1;
        let qualified = format!("{}::route:{}", file_path, url_path);
        let route_id = stable_node_id(file_path, &qualified);

        out.nodes.push(Node {
            id: route_id.clone(),
            kind: NodeKind::Route,
            name: url_path.to_string(),
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

        if let Some(target) = resolve_handler_name(handler_expr) {
            out.references.push(make_ref(
                &route_id,
                &target.name,
                target.kind,
                line,
                file_path,
            ));
        }
    }

    let router_re = regex::Regex::new(r#"\.register\s*\(\s*r?['"]([^'"]+)['"]\s*,\s*([\w.]+)"#)
        .expect("drf router re");
    for cap in router_re.captures_iter(&safe) {
        let prefix = cap
            .get(1)
            .map(|m| m.as_str())
            .unwrap_or("")
            .trim_start_matches('^')
            .trim_end_matches('/')
            .trim_end_matches('$');
        let viewset = cap
            .get(2)
            .map(|m| m.as_str())
            .unwrap_or("")
            .split('.')
            .last()
            .unwrap_or("");
        if !viewset.ends_with("View") && !viewset.ends_with("ViewSet") {
            continue;
        }
        let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = safe[..match_start].matches('\n').count() as i32 + 1;
        let qualified = format!("{}::route:VIEWSET:{}", file_path, prefix);
        let route_id = stable_node_id(file_path, &qualified);

        out.nodes.push(Node {
            id: route_id.clone(),
            kind: NodeKind::Route,
            name: format!("VIEWSET /{}", prefix),
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

        out.references.push(make_ref(
            &route_id,
            viewset,
            ReferenceKind::References,
            line,
            file_path,
        ));
    }

    out
}

pub async fn try_resolve_target(
    queries: &QueryBuilder,
    ref_: &UnresolvedRef,
) -> Option<(Node, f64)> {
    let name = &ref_.reference_name;

    if name == "_iterable_class" {
        if let Some(node) = resolve_model_iterable_iter(queries).await {
            return Some((node, 0.7));
        }
    }

    if name.ends_with("Model") || is_short_pascal(name) {
        if let Some(n) = resolve_by_dirs(queries, name, &[NodeKind::Class], MODEL_DIRS).await {
            return Some((n, 0.8));
        }
    }

    if name.ends_with("View") || name.ends_with("ViewSet") {
        let kinds = [NodeKind::Class, NodeKind::Function];
        if let Some(n) = resolve_by_dirs(queries, name, &kinds, VIEW_DIRS).await {
            return Some((n, 0.8));
        }
    }

    if name.ends_with("Form") {
        if let Some(n) = resolve_by_dirs(queries, name, &[NodeKind::Class], FORM_DIRS).await {
            return Some((n, 0.8));
        }
    }

    None
}

struct HandlerTarget {
    name: String,
    kind: ReferenceKind,
}

fn resolve_handler_name(expr: &str) -> Option<HandlerTarget> {
    let trimmed = expr.trim();
    let include_re = regex::Regex::new(r#"^include\s*\(\s*['"]([^'"]+)['"]"#).expect("include");
    if let Some(cap) = include_re.captures(trimmed) {
        return Some(HandlerTarget {
            name: cap.get(1)?.as_str().to_string(),
            kind: ReferenceKind::Imports,
        });
    }

    let mut head = trimmed.to_string();
    let as_view_re = regex::Regex::new(r"\.as_view\s*\([^)]*\)\s*$").expect("as_view");
    head = as_view_re.replace(&head, "").to_string();
    let method_call_re = regex::Regex::new(r"\.\w+\s*\([^)]*\)\s*$").expect("method call");
    head = method_call_re.replace(&head, "").to_string();

    let last = head.split('.').filter(|s| !s.is_empty()).last()?;
    if !regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$")
        .expect("ident")
        .is_match(last)
    {
        return None;
    }

    Some(HandlerTarget {
        name: last.to_string(),
        kind: ReferenceKind::References,
    })
}

async fn resolve_by_dirs(
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
    let preferred: Vec<Node> = filtered
        .iter()
        .filter(|n| dirs.iter().any(|d| n.file_path.contains(d)))
        .cloned()
        .collect();
    preferred.first().or_else(|| filtered.first()).cloned()
}

async fn resolve_model_iterable_iter(queries: &QueryBuilder) -> Option<Node> {
    let classes = queries.get_nodes_by_name("ModelIterable").await.unwrap_or_default();
    let class = classes.iter().find(|n| n.kind == NodeKind::Class)?;
    let iters = queries.get_nodes_by_name("__iter__").await.unwrap_or_default();
    iters
        .iter()
        .find(|n| {
            n.file_path == class.file_path
                && n.start_line >= class.start_line
                && n.start_line <= class.end_line
        })
        .cloned()
}

fn is_short_pascal(name: &str) -> bool {
  let mut chars = name.chars();
    let first = chars.next();
    first.map(|c| c.is_uppercase() && c.is_alphabetic()).unwrap_or(false)
        && chars.all(|c| c.is_alphanumeric())
        && name.len() <= 32
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
        language: Some(Language::Python),
        candidates: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_to_view_function() {
        let content = "urlpatterns = [path('users/', views.user_list),]";
        let r = extract_file("urls.py", content);
        assert_eq!(r.nodes.len(), 1);
        assert_eq!(r.nodes[0].name, "users/");
        assert_eq!(r.references[0].reference_name, "user_list");
    }

    #[test]
    fn path_to_class_as_view() {
        let content = "path('login/', LoginView.as_view(), name='login'),";
        let r = extract_file("urls.py", content);
        assert_eq!(r.references[0].reference_name, "LoginView");
    }

    #[test]
    fn path_include_urls_module() {
        let content = "path('api/', include('myapp.urls')),";
        let r = extract_file("urls.py", content);
        assert_eq!(r.references[0].reference_name, "myapp.urls");
        assert_eq!(r.references[0].reference_kind, ReferenceKind::Imports);
    }

    #[test]
    fn drf_router_register() {
        let content = "router.register(r'articles', ArticleViewSet, basename='article')";
        let r = extract_file("urls.py", content);
        assert!(r.nodes.iter().any(|n| n.name.contains("VIEWSET")));
        assert!(r.references.iter().any(|ref_| ref_.reference_name == "ArticleViewSet"));
    }

    #[test]
    fn claims_reference_urls_module() {
        assert!(claims_reference("myapp.urls"));
        assert!(claims_reference("_iterable_class"));
    }
}