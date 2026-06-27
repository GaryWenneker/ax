//! Rust web route extraction (Actix/Rocket attrs, Axum, Actix builder) — CG: frameworks/rust.ts

use regex::Regex;

use ax_db::queries::QueryBuilder;
use ax_types::{Language, Node, NodeKind, ReferenceKind, UnresolvedReference};

use crate::strip_comments::strip_comments_for_regex;
use crate::types::UnresolvedRef;

use super::extract::{now_ms, stable_node_id, FrameworkExtractResult};

const HANDLER_DIRS: &[&str] = &["/handlers/", "/handler/", "/api/", "/routes/", "/controllers/"];
const SERVICE_DIRS: &[&str] = &["/services/", "/service/", "/repository/", "/domain/"];
const MODEL_DIRS: &[&str] = &["/models/", "/model/", "/entities/", "/entity/", "/domain/", "/types/"];

pub fn detect(project_root: &std::path::Path) -> bool {
    project_root.join("Cargo.toml").exists()
}

pub fn is_active_for_ref(ref_: &UnresolvedRef) -> bool {
    ref_.language == Language::Rust
}

pub fn extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    if !file_path.ends_with(".rs") {
        return FrameworkExtractResult::default();
    }

    let safe = strip_comments_for_regex(content, Language::Rust);
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();

    let attr_re = Regex::new(
        r#"#\[(get|post|put|patch|delete|head|options)\s*\(\s*['"]([^'"]+)['"][^\]]*\)\]"#,
    )
    .expect("rust attr route");

    for cap in attr_re.captures_iter(&safe) {
        let method = cap.get(1).map(|m| m.as_str()).unwrap_or("get");
        let route_path = cap.get(2).map(|m| m.as_str()).unwrap_or("");
        let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = safe[..match_start].matches('\n').count() as i32 + 1;
        let upper = method.to_uppercase();
        push_route(
            &mut out,
            file_path,
            &upper,
            route_path,
            line,
            cap.get(0).map(|m| m.as_str().len() as i32).unwrap_or(0),
            now,
        );

        let tail = safe.get(match_start + cap.get(0).map(|m| m.len()).unwrap_or(0)..).unwrap_or("");
        let fn_re = Regex::new(r"\n\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").expect("fn after attr");
        if let Some(fn_cap) = fn_re.captures(tail) {
            if let Some(handler) = fn_cap.get(1) {
                if let Some(route_id) = out.nodes.last().map(|n| n.id.clone()) {
                    push_handler_ref(&mut out, &route_id, handler.as_str(), line, file_path);
                }
            }
        }
    }

    let route_open = Regex::new(r"\.route\s*\(").expect("axum route open");
    for m in route_open.find_iter(&safe) {
        let open_idx = m.end() - 1;
        let close_idx = find_matching_paren(&safe, open_idx);
        if close_idx < 0 {
            continue;
        }
        let close = close_idx as usize;
        let args = &safe[open_idx + 1..close];
        let path_re = Regex::new(r#"^\s*"([^"]+)"\s*,"#).expect("route path");
        let path_match = path_re.captures(args);
        if let Some(pm) = path_match {
            let route_path = pm.get(1).map(|p| p.as_str()).unwrap_or("");
            let line = safe[..m.start()].matches('\n').count() as i32 + 1;
            let prefix_len = pm.get(0).map(|p| p.as_str()).unwrap_or("").len();
            let method_body = &args[prefix_len..];
            let mh_re =
                Regex::new(r"\b(get|post|put|patch|delete|head|options|trace)\s*\(\s*([A-Za-z_][\w:]*)")
                    .expect("axum method handler");
            for mh in mh_re.captures_iter(method_body) {
                let upper = mh.get(1).map(|x| x.as_str().to_uppercase()).unwrap_or_default();
                let handler = mh
                    .get(2)
                    .map(|x| x.as_str())
                    .and_then(|h| h.split("::").last())
                    .unwrap_or("");
                if handler.is_empty() {
                    continue;
                }
                push_route(&mut out, file_path, &upper, route_path, line, 0, now);
                if let Some(route_id) = out.nodes.last().map(|n| n.id.clone()) {
                    push_handler_ref(&mut out, &route_id, handler, line, file_path);
                }
            }
        }
    }


    let resource_re = Regex::new(r#"web::resource\s*\(\s*"([^"]+)"\s*\)"#).expect("actix resource");
    for cap in resource_re.captures_iter(&safe) {
        let route_path = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let start_line = safe[..match_start].matches('\n').count() as i32 + 1;
        let after = match_start + cap.get(0).map(|m| m.len()).unwrap_or(0);
        let next_res = safe[after..].find("web::resource");
        let end = if let Some(n) = next_res {
            after + n.min(500)
        } else {
            safe.len().min(after + 500)
        };
        let chain = &safe[after..end];
        let method_to =
            Regex::new(r#"web::(get|post|put|patch|delete|head)\s*\(\s*\)\s*\.to\s*\(\s*([A-Za-z_][\w:]*)"#)
                .expect("actix method to");
        let mut found = false;
        for m2 in method_to.captures_iter(chain) {
            let method = m2.get(1).map(|x| x.as_str()).unwrap_or("ANY");
            let handler_expr = m2.get(2).map(|x| x.as_str()).unwrap_or("");
            let handler = handler_expr.split("::").last().unwrap_or(handler_expr);
            if handler.is_empty() {
                continue;
            }
            let m_line = start_line + chain[..m2.get(0).unwrap().start()].matches('\n').count() as i32;
            let upper = method.to_uppercase();
            push_route(&mut out, file_path, &upper, route_path, m_line, 0, now);
            if let Some(route_id) = out.nodes.last().map(|n| n.id.clone()) {
                push_handler_ref(&mut out, &route_id, handler, m_line, file_path);
            }
            found = true;
        }
        if !found {
            let direct_re = Regex::new(r#"^\s*\.to\s*\(\s*([A-Za-z_][\w:]*)"#).expect("direct to");
            if let Some(d) = direct_re.captures(chain) {
                let handler_expr = d.get(1).map(|x| x.as_str()).unwrap_or("");
                let handler = handler_expr.split("::").last().unwrap_or(handler_expr);
                if !handler.is_empty() {
                    push_route(&mut out, file_path, "ANY", route_path, start_line, 0, now);
                    if let Some(route_id) = out.nodes.last().map(|n| n.id.clone()) {
                        push_handler_ref(&mut out, &route_id, handler, start_line, file_path);
                    }
                }
            }
        }
    }
  let app_route = Regex::new(
        r#"\.route\s*\(\s*"([^"]+)"\s*,\s*web::(get|post|put|patch|delete|head)\s*\(\s*\)\s*\.to\s*\(\s*([A-Za-z_][\w:]*)"#,
    )
    .expect("actix app route");
    for cap in app_route.captures_iter(&safe) {
        let route_path = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        let method = cap.get(2).map(|m| m.as_str()).unwrap_or("ANY");
        let handler_expr = cap.get(3).map(|m| m.as_str()).unwrap_or("");
        let line = safe[..cap.get(0).unwrap().start()].matches('\n').count() as i32 + 1;
        let handler = handler_expr.split("::").last().unwrap_or(handler_expr);
        if handler.is_empty() {
            continue;
        }
        let upper_method = method.to_uppercase();
        push_route(&mut out, file_path, &upper_method, route_path, line, 0, now);
        if let Some(route_id) = out.nodes.last().map(|n| n.id.clone()) {
            push_handler_ref(&mut out, &route_id, handler, line, file_path);
        }
    }

    out
}

pub async fn try_resolve_target(
    queries: &QueryBuilder,
    project_root: &std::path::Path,
    ref_: &UnresolvedRef,
) -> Option<(Node, f64)> {
    let name = &ref_.reference_name;

    if name.ends_with("_handler") || name.starts_with("handle_") {
        if let Some(n) = resolve_by_kinds(
            queries,
            name,
            &[NodeKind::Function],
            HANDLER_DIRS,
        )
        .await
        {
            return Some((n, 0.8));
        }
    }

    if name.ends_with("Service") || name.ends_with("Repository") {
        let kinds = [NodeKind::Struct, NodeKind::Trait];
        if let Some(n) = resolve_by_kinds(queries, name, &kinds, SERVICE_DIRS).await {
            return Some((n, 0.8));
        }
    }

    if is_pascal_case(name) {
        if let Some(n) = resolve_by_kinds(queries, name, &[NodeKind::Struct], MODEL_DIRS).await {
            return Some((n, 0.7));
        }
    }

    if name.chars().all(|c| c.is_ascii_lowercase() || c == '_') && !name.is_empty() {
        if let Some((n, from_ws)) = resolve_module(queries, project_root, name).await {
            let conf = if from_ws { 0.95 } else { 0.6 };
            return Some((n, conf));
        }
    }

    None
}

async fn resolve_module(
    queries: &QueryBuilder,
    project_root: &std::path::Path,
    name: &str,
) -> Option<(Node, bool)> {
    let paths = [format!("src/{name}.rs"), format!("src/{name}/mod.rs")];
    for path in paths {
        let nodes = queries.get_nodes_by_file(&path).await.unwrap_or_default();
        if nodes.is_empty() {
            continue;
        }
        let mod_node = nodes.iter().find(|n| n.kind == NodeKind::Module);
        if let Some(n) = mod_node {
            return Some((n.clone(), false));
        }
        return Some((nodes[0].clone(), false));
    }

    let map = super::cargo_workspace::load_crate_map(project_root);
    if let Some(member_path) = map.get(name) {
        let ws_paths = [
            format!("{member_path}/src/lib.rs"),
            format!("{member_path}/src/main.rs"),
        ];
        for path in ws_paths {
            let nodes = queries.get_nodes_by_file(&path).await.unwrap_or_default();
            if nodes.is_empty() {
                continue;
            }
            let mod_node = nodes.iter().find(|n| n.kind == NodeKind::Module);
            if let Some(n) = mod_node {
                return Some((n.clone(), true));
            }
            return Some((nodes[0].clone(), true));
        }
    }
    None
}

async fn resolve_by_kinds(
    queries: &QueryBuilder,
    name: &str,
    kinds: &[NodeKind],
    preferred_dirs: &[&str],
) -> Option<Node> {
    let nodes = queries.get_nodes_by_name(name).await.unwrap_or_default();
    let filtered: Vec<Node> = nodes
        .into_iter()
        .filter(|n| kinds.contains(&n.kind))
        .collect();
    if filtered.is_empty() {
        return None;
    }
    let preferred: Vec<Node> = filtered
        .iter()
        .filter(|n| preferred_dirs.iter().any(|d| n.file_path.contains(d)))
        .cloned()
        .collect();
    preferred.first().or(filtered.first()).cloned()
}

fn push_route(
    out: &mut FrameworkExtractResult,
    file_path: &str,
    method: &str,
    route_path: &str,
    line: i32,
    end_col: i32,
    now: i64,
) {
    let qualified = format!("{}::route:{}", file_path, route_path);
    let route_id = stable_node_id(file_path, &qualified);
    out.nodes.push(Node {
        id: route_id,
        kind: NodeKind::Route,
        name: format!("{} {}", method, route_path),
        qualified_name: qualified,
        file_path: file_path.to_string(),
        language: Language::Rust,
        start_line: line,
        end_line: line,
        start_column: 0,
        end_column: end_col,
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

fn push_handler_ref(
    out: &mut FrameworkExtractResult,
    route_id: &str,
    handler: &str,
    line: i32,
    file_path: &str,
) {
    out.references.push(UnresolvedReference {
        from_node_id: route_id.to_string(),
        reference_name: handler.to_string(),
        reference_kind: ReferenceKind::References,
        line,
        column: 0,
        file_path: Some(file_path.to_string()),
        language: Some(Language::Rust),
        candidates: None,
    });
}

fn find_matching_paren(s: &str, open_idx: usize) -> i32 {
    let bytes = s.as_bytes();
    if open_idx >= bytes.len() || bytes[open_idx] != b'(' {
        return -1;
    }
    let mut depth = 0;
    let mut i = open_idx;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' || b == b'\'' {
            i += 1;
            while i < bytes.len() && bytes[i] != b {
                if bytes[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
            continue;
        }
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 {
                return i as i32;
            }
        }
        i += 1;
    }
    -1
}

fn is_pascal_case(name: &str) -> bool {
    let mut chars = name.chars();
    let first = chars.next();
    first.map(|c| c.is_uppercase() && c.is_alphabetic()).unwrap_or(false)
        && chars.all(|c| c.is_alphanumeric())
}