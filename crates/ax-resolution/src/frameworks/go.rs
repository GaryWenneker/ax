//! Go HTTP route extraction (Gin, Chi, net/http) — CG: frameworks/go.ts

use regex::Regex;

use ax_db::queries::QueryBuilder;
use ax_types::{Language, Node, NodeKind, ReferenceKind, UnresolvedReference};

use crate::strip_comments::strip_comments_for_regex;
use crate::types::UnresolvedRef;

use super::extract::{now_ms, stable_node_id, FrameworkExtractResult};

const HANDLER_DIRS: &[&str] = &["handler", "handlers", "api", "routes", "controller", "controllers"];
const SERVICE_DIRS: &[&str] = &["service", "services", "repository", "store", "pkg"];
const MIDDLEWARE_DIRS: &[&str] = &["middleware", "middlewares"];
const MODEL_DIRS: &[&str] = &["model", "models", "entity", "entities", "domain", "pkg"];

pub fn detect(project_root: &std::path::Path, indexed_files: &[String]) -> bool {
    if project_root.join("go.mod").exists() {
        return true;
    }
    indexed_files.iter().any(|f| f.ends_with(".go"))
}

pub fn is_active_for_ref(ref_: &UnresolvedRef) -> bool {
    ref_.language == Language::Go
}

pub fn extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    if !file_path.ends_with(".go") {
        return FrameworkExtractResult::default();
    }

    let safe = strip_comments_for_regex(content, Language::Go);
    let route_re = Regex::new(
        r#"\b\w+\.(GET|POST|PUT|PATCH|DELETE|OPTIONS|HEAD|Get|Post|Put|Patch|Delete|Handle|HandleFunc)\s*\(\s*"([^"]+)"\s*,\s*([^)]+)\)"#,
    )
    .expect("go route regex");

    let mut out = FrameworkExtractResult::default();
    let now = now_ms();

    for cap in route_re.captures_iter(&safe) {
        let raw_method = cap.get(1).map(|m| m.as_str()).unwrap_or("GET");
        let route_path = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let handler_expr = cap.get(3).map(|m| m.as_str()).unwrap_or("");
        let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = safe[..match_start].matches('\n').count() as i32 + 1;

        let method = if raw_method == "Handle" || raw_method == "HandleFunc" {
            "ANY".to_string()
        } else {
            raw_method.to_uppercase()
        };

        let qualified = format!("{}::route:{}", file_path, route_path);
        let route_id = stable_node_id(file_path, &qualified);

        out.nodes.push(Node {
            id: route_id.clone(),
            kind: NodeKind::Route,
            name: format!("{} {}", method, route_path),
            qualified_name: qualified,
            file_path: file_path.to_string(),
            language: Language::Go,
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

        if let Some(handler_name) = extract_go_tail_ident(handler_expr) {
            out.references.push(UnresolvedReference {
                from_node_id: route_id,
                reference_name: handler_name,
                reference_kind: ReferenceKind::References,
                line,
                column: 0,
                file_path: Some(file_path.to_string()),
                language: Some(Language::Go),
                candidates: None,
            });
        }
    }

    out
}

pub async fn try_resolve_target(
    queries: &QueryBuilder,
    ref_: &UnresolvedRef,
) -> Option<(Node, f64)> {
    let name = &ref_.reference_name;

    if name.ends_with("Handler") || name.starts_with("Handle") {
        if let Some(n) = resolve_by_name_and_kind(
            queries,
            name,
            Some(NodeKind::Function),
            HANDLER_DIRS,
            None,
        )
        .await
        {
            return Some((n, 0.8));
        }
    }

    if name.ends_with("Service") || name.ends_with("Repository") || name.ends_with("Store") {
        let kinds = [NodeKind::Struct, NodeKind::Interface];
        if let Some(n) =
            resolve_by_name_and_kind(queries, name, None, SERVICE_DIRS, Some(&kinds)).await
        {
            return Some((n, 0.8));
        }
    }

    if name.ends_with("Middleware") || name.starts_with("Auth") || name.starts_with("Log") {
        if let Some(n) = resolve_by_name_and_kind(
            queries,
            name,
            Some(NodeKind::Function),
            MIDDLEWARE_DIRS,
            None,
        )
        .await
        {
            return Some((n, 0.75));
        }
    }

    if is_pascal_case(name) {
        if let Some(n) = resolve_by_name_and_kind(
            queries,
            name,
            Some(NodeKind::Struct),
            MODEL_DIRS,
            None,
        )
        .await
        {
            return Some((n, 0.7));
        }
    }

    None
}

async fn resolve_by_name_and_kind(
    queries: &QueryBuilder,
    name: &str,
    kind: Option<NodeKind>,
    preferred_dirs: &[&str],
    kinds: Option<&[NodeKind]>,
) -> Option<Node> {
    let nodes = queries.get_nodes_by_name(name).await.unwrap_or_default();
    let filtered: Vec<Node> = nodes
        .into_iter()
        .filter(|n| {
            if let Some(klist) = kinds {
                klist.contains(&n.kind)
            } else if let Some(k) = kind {
                n.kind == k
            } else {
                true
            }
        })
        .collect();
    if filtered.is_empty() {
        return None;
    }
    let preferred: Vec<Node> = filtered
        .iter()
        .filter(|n| preferred_dirs.iter().any(|d| n.file_path.contains(d)))
        .cloned()
        .collect();
    if let Some(n) = preferred.first() {
        return Some(n.clone());
    }
    filtered.first().cloned()
}

fn extract_go_tail_ident(expr: &str) -> Option<String> {
    let cleaned = expr.trim().replace(' ', "").replace("()", "");
    let re = Regex::new(r"(?:\.|^)([A-Za-z_][A-Za-z0-9_]*)$").expect("go tail ident");
    re.captures(&cleaned)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    let first = chars.next();
    first.map(|c| c.is_uppercase() && c.is_alphabetic()).unwrap_or(false)
        && chars.all(|c| c.is_alphanumeric())
}