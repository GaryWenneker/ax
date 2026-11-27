//! Rust web route extraction (Actix/Rocket attrs, Axum, Actix builder) — CG: frameworks/rust.ts

use ax_utils::slice_bytes;
use regex::Regex;
use std::sync::OnceLock;

use ax_db::queries::QueryBuilder;
use ax_types::{Language, Node, NodeKind, ReferenceKind, UnresolvedReference};

use crate::strip_comments::strip_comments_for_regex;
use crate::types::UnresolvedRef;

use super::extract::{now_ms, stable_node_id, FrameworkExtractResult};

const HANDLER_DIRS: &[&str] = &[
    "/handlers/",
    "/handler/",
    "/api/",
    "/routes/",
    "/controllers/",
];
const SERVICE_DIRS: &[&str] = &["/services/", "/service/", "/repository/", "/domain/"];
const MODEL_DIRS: &[&str] = &[
    "/models/",
    "/model/",
    "/entities/",
    "/entity/",
    "/domain/",
    "/types/",
];

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

        let tail = safe
            .get(match_start + cap.get(0).map(|m| m.len()).unwrap_or(0)..)
            .unwrap_or("");
        let fn_re = fn_after_attr_re();
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
        let path_re = route_path_re();
        let path_match = path_re.captures(args);
        if let Some(pm) = path_match {
            let route_path = pm.get(1).map(|p| p.as_str()).unwrap_or("");
            let line = safe[..m.start()].matches('\n').count() as i32 + 1;
            let prefix_len = pm.get(0).map(|p| p.as_str()).unwrap_or("").len();
            let method_body = &args[prefix_len..];
            let mh_re = axum_method_re();
            for mh in mh_re.captures_iter(method_body) {
                let upper = mh
                    .get(1)
                    .map(|x| x.as_str().to_uppercase())
                    .unwrap_or_default();
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
        let chain = slice_bytes(&safe, after, end);
        let method_to = actix_method_to_re();
        let mut found = false;
        for m2 in method_to.captures_iter(chain) {
            let method = m2.get(1).map(|x| x.as_str()).unwrap_or("ANY");
            let handler_expr = m2.get(2).map(|x| x.as_str()).unwrap_or("");
            let handler = handler_expr.split("::").last().unwrap_or(handler_expr);
            if handler.is_empty() {
                continue;
            }
            let m_line =
                start_line + chain[..m2.get(0).unwrap().start()].matches('\n').count() as i32;
            let upper = method.to_uppercase();
            push_route(&mut out, file_path, &upper, route_path, m_line, 0, now);
            if let Some(route_id) = out.nodes.last().map(|n| n.id.clone()) {
                push_handler_ref(&mut out, &route_id, handler, m_line, file_path);
            }
            found = true;
        }
        if !found {
            let direct_re = direct_to_re();
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

    // Table names only come from string literals that are SQL statements,
    // so prose such as "failed to read from disk" adds nothing.
    let mut seen_tables = std::collections::HashSet::new();
    for (lit_start, lit) in string_literals(&safe) {
        // `\n`, `\r` and `\t` escapes separate SQL words; two spaces keep offsets.
        let text = lit
            .replace("\\n", "  ")
            .replace("\\r", "  ")
            .replace("\\t", "  ");
        if !sql_statement_re().is_match(&text) {
            continue;
        }
        for cap in sql_table_re().captures_iter(&text) {
            let table = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            if ["select", "where", "set", "values", "on"]
                .contains(&table.to_ascii_lowercase().as_str())
                || !seen_tables.insert(table.to_string())
            {
                continue;
            }
            let at = lit_start + cap.get(0).map(|m| m.start()).unwrap_or(0);
            let line = safe[..at].matches('\n').count() as i32 + 1;
            push_table(&mut out, file_path, table, line, now);
        }
    }

    out
}

/// A string literal whose text starts like an SQL statement: the statement
/// keyword plus the keyword that must follow it, so prose such as
/// "update the counter" or "select a file" does not count.
fn sql_statement_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?is)^\s*(?:SELECT\s+(?:\*|DISTINCT\b|COUNT\s*\(|[\w.]+\s*(?:,|\bAS\b|\bFROM\b))|INSERT\s+INTO\s|UPDATE\s+\w+\s+SET\s|DELETE\s+FROM\s|WITH\s+\w+\s+AS\s*\(|(?:REPLACE|MERGE)\s+INTO\s)",
        )
        .expect("sql statement")
    })
}

fn sql_table_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:FROM|INTO|UPDATE|JOIN)\s+([a-z_][a-z0-9_]*)\b").expect("sql table")
    })
}

/// The string literals of Rust source as (byte offset of the text, text).
/// Handles escapes, raw strings (`r#"…"#`), byte strings and char literals
/// such as `'"'`, so a quote inside a char does not open a string.
fn string_literals(src: &str) -> Vec<(usize, &str)> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' => {
                // Char literal ('x', '\n', '"') or a lifetime ('a): skip a
                // char literal whole, leave a lifetime alone.
                if b.get(i + 1) == Some(&b'\\') {
                    let mut j = i + 2;
                    while j < b.len() && b[j] != b'\'' && b[j] != b'\n' {
                        j += 1;
                    }
                    i = j + 1;
                } else if let Some(c) = src[i + 1..].chars().next() {
                    let after = i + 1 + c.len_utf8();
                    i = if b.get(after) == Some(&b'\'') {
                        after + 1
                    } else {
                        i + 1
                    };
                } else {
                    i += 1;
                }
            }
            b'r' if starts_raw_string(b, i) && matches!(b.get(i + 1), Some(b'"') | Some(b'#')) => {
                let mut j = i + 1;
                let mut hashes = 0;
                while b.get(j) == Some(&b'#') {
                    hashes += 1;
                    j += 1;
                }
                if b.get(j) != Some(&b'"') {
                    i += 1;
                    continue;
                }
                let start = j + 1;
                let mut closing = String::from("\"");
                closing.push_str(&"#".repeat(hashes));
                match src[start..].find(&closing) {
                    Some(len) => {
                        out.push((start, &src[start..start + len]));
                        i = start + len + closing.len();
                    }
                    None => break,
                }
            }
            b'"' => {
                let start = i + 1;
                let mut j = start;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                let end = j.min(b.len());
                out.push((start, &src[start..end]));
                i = end + 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn prev_is_ident(b: &[u8], i: usize) -> bool {
    i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_')
}

/// `r` at `i` opens a raw string (`r"`, `r#"`, `br"`), not an identifier's tail.
fn starts_raw_string(b: &[u8], i: usize) -> bool {
    !prev_is_ident(b, i) || (b[i - 1] == b'b' && !prev_is_ident(b, i - 1))
}

fn push_table(out: &mut FrameworkExtractResult, file_path: &str, table: &str, line: i32, now: i64) {
    let qualified = format!("table:{}", table);
    let id = stable_node_id(file_path, &qualified);
    out.nodes.push(Node {
        id,
        kind: NodeKind::Table,
        name: table.to_string(),
        qualified_name: qualified,
        file_path: file_path.to_string(),
        language: Language::Rust,
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

pub async fn try_resolve_target(
    queries: &QueryBuilder,
    project_root: &std::path::Path,
    ref_: &UnresolvedRef,
) -> Option<(Node, f64)> {
    let name = &ref_.reference_name;

    if name.ends_with("_handler") || name.starts_with("handle_") {
        if let Some(n) = resolve_by_kinds(queries, name, &[NodeKind::Function], HANDLER_DIRS).await
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
    let qualified = format!("{}::route:{}:{}", file_path, method, route_path);
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
    first
        .map(|c| c.is_uppercase() && c.is_alphabetic())
        .unwrap_or(false)
        && chars.all(|c| c.is_alphanumeric())
}

fn fn_after_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\n\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)").expect("fn after attr")
    })
}

fn route_path_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"^\s*"([^"]+)"\s*,"#).expect("route path"))
}

fn axum_method_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(get|post|put|patch|delete|head|options|trace)\s*\(\s*([A-Za-z_][\w:]*)")
            .expect("axum method handler")
    })
}

fn actix_method_to_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"web::(get|post|put|patch|delete|head)\s*\(\s*\)\s*\.to\s*\(\s*([A-Za-z_][\w:]*)"#,
        )
        .expect("actix method to")
    })
}

fn direct_to_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"^\s*\.to\s*\(\s*([A-Za-z_][\w:]*)"#).expect("direct to"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn get_and_post_on_one_path_are_two_routes() {
        let src = "fn app() -> Router {\n    Router::new()\n        .route(\"/users\", get(list))\n        .route(\"/users\", post(create))\n}\n";
        let out = extract_file("src/main.rs", src);
        let ids: std::collections::HashSet<_> = out.nodes.iter().map(|n| n.id.clone()).collect();
        assert_eq!(
            out.nodes.len(),
            2,
            "{:?}",
            out.nodes.iter().map(|n| &n.name).collect::<Vec<_>>()
        );
        assert_eq!(ids.len(), 2, "GET and POST /users share a node id");
    }

    use super::*;

    fn tables(src: &str) -> Vec<String> {
        extract_file("src/db.rs", src)
            .nodes
            .into_iter()
            .filter(|n| n.kind == NodeKind::Table)
            .map(|n| n.name)
            .collect()
    }

    #[test]
    fn plain_messages_do_not_become_tables() {
        let src = r#"fn f() {
    log("failed to read from disk");
    let b = convert("convert into bytes");
    panic!("could not join the thread");
    let s = "update the counter";
    let t = "select a file from the list";
}
"#;
        assert_eq!(tables(src), Vec::<String>::new());
    }

    #[test]
    fn sql_strings_give_one_table_node_per_name() {
        let src = r##"fn f<'a>(x: &'a str, e: char) {
    let _ = ('\u{e9}', '\'', 'é');
    sqlx::query("SELECT id\n FROM users JOIN orders ON orders.user_id = users.id");
    sqlx::query(r#"insert into users (id) values (1)"#);
    let c = '"';
    sqlx::query("UPDATE accounts SET balance = 0");
    sqlx::query("  WITH t AS (SELECT 1) DELETE FROM audit_log");
}
"##;
        let mut got = tables(src);
        got.sort();
        assert_eq!(got, vec!["accounts", "audit_log", "orders", "users"]);
    }

    #[test]
    fn actix_resource_window_cut_inside_a_character_does_not_panic() {
        let head = r#"web::resource("/a")"#;
        let mut src = format!("{head}.route(web::get().to(list)) // ");
        // Put a two-byte character across byte 500 after the resource call.
        while src.len() < head.len() + 499 {
            src.push('a');
        }
        src.push_str("\u{e9} more text\n");
        let r = extract_file("src/main.rs", &format!("fn f() {{ {src} }}"));
        assert!(r.nodes.iter().any(|n| n.name.contains("/a")));
    }

    #[test]
    fn actix_next_resource_far_away_cut_inside_a_character_does_not_panic() {
        let head = r#"web::resource("/a")"#;
        let mut src = format!("{head}.route(web::get().to(list)); let s = \"");
        while src.len() < head.len() + 499 {
            src.push('a');
        }
        src.push_str("\u{e9}\"; web::resource(\"/b\");\n");
        let r = extract_file("src/main.rs", &format!("fn f() {{ {src} }}"));
        assert!(r.nodes.iter().any(|n| n.name.contains("/a")));
    }
}
