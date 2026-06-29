//! Express / Node.js route extraction.

use regex::Regex;

use ax_types::{
    Language, Node, NodeKind, ReferenceKind, UnresolvedReference,
};

use crate::strip_comments::strip_comments_for_regex;

use super::extract::{language_for_path, now_ms, stable_node_id, FrameworkExtractResult};

const RESERVED_CALLS: &[&str] = &[
    "json", "jsonp", "send", "sendStatus", "sendFile", "status", "end", "redirect",
    "render", "set", "get", "header", "type", "format", "attachment", "download",
    "cookie", "clearCookie", "append", "location", "vary", "links", "accepts", "is",
    "next", "then", "catch", "finally", "resolve", "reject", "all", "race",
    "map", "filter", "forEach", "reduce", "find", "push", "pop", "slice", "splice",
    "includes", "keys", "values", "entries", "assign", "parse", "stringify",
    "log", "error", "warn", "info", "String", "Number", "Boolean", "Array", "Object",
    "Date", "Math", "JSON", "Promise", "require", "fail",
];

pub fn detect(project_root: &std::path::Path) -> bool {
    let pkg_path = project_root.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
            let deps = merge_deps(&json);
            if deps.get("express").is_some()
                || deps.get("fastify").is_some()
                || deps.get("koa").is_some()
                || deps.get("hapi").is_some()
            {
                return true;
            }
        }
    }
    false
}

pub fn extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    if !is_express_file(file_path) {
        return FrameworkExtractResult::default();
    }

    let lang = language_for_path(file_path);
    let safe = strip_comments_for_regex(content, lang);
    let head_re = Regex::new(
        r#"\b(app|router)\.(get|post|put|patch|delete|all|use)\s*\(\s*['"]([^'"]+)['"]\s*,"#,
    )
    .expect("express head regex");

    let mut out = FrameworkExtractResult::default();
    let now = now_ms();

    for cap in head_re.captures_iter(&safe) {
        let method = cap.get(2).map(|m| m.as_str()).unwrap_or("get");
        let route_path = cap.get(3).map(|m| m.as_str()).unwrap_or("");
        if method == "use" && !route_path.starts_with('/') {
            continue;
        }

        let match_start = cap.get(0).map(|m| m.start()).unwrap_or(0);
        let line = safe[..match_start].matches('\n').count() as i32 + 1;
        let qualified = format!("{}::{}:{}", file_path, method.to_uppercase(), route_path);
        let route_id = stable_node_id(file_path, &qualified);

        let route_node = Node {
            id: route_id.clone(),
            kind: NodeKind::Route,
            name: format!("{} {}", method.to_uppercase(), route_path),
            qualified_name: qualified,
            file_path: file_path.to_string(),
            language: lang,
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
        };
        out.nodes.push(route_node);

        let open_paren = safe[match_start..].find('(');
        if open_paren.is_none() {
            continue;
        }
        let open_paren = match_start + open_paren.unwrap();
        let close_paren = match_delim(&safe, open_paren, '(', ')');
        if close_paren <= open_paren {
            continue;
        }
        let args = safe[open_paren + 1..close_paren].to_string();

        if let Some(arrow_at) = args.find("=>") {
            let after_arrow = &args[arrow_at + 2..];
            let body = extract_arrow_body(after_arrow);
            let call_re = Regex::new(r"\b([A-Za-z_$][\w$]*)\s*\(").expect("call regex");
            let mut seen = std::collections::HashSet::new();
            for cm in call_re.captures_iter(body) {
                let name = cm.get(1).map(|m| m.as_str()).unwrap_or("");
                if name.is_empty() || seen.contains(name) || RESERVED_CALLS.contains(&name) {
                    continue;
                }
                seen.insert(name);
                out.references.push(make_ref(
                    &route_id,
                    name,
                    ReferenceKind::Calls,
                    line,
                    file_path,
                    lang,
                ));
            }
        } else {
            let parts: Vec<&str> = args
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if let Some(last) = parts.last() {
                if let Some(handler_name) = extract_tail_ident(last) {
                    out.references.push(make_ref(
                        &route_id,
                        &handler_name,
                        ReferenceKind::References,
                        line,
                        file_path,
                        lang,
                    ));
                }
            }
        }
    }

    out
}

fn is_express_file(file_path: &str) -> bool {
    file_path.ends_with(".ts")
        || file_path.ends_with(".tsx")
        || file_path.ends_with(".js")
        || file_path.ends_with(".mjs")
        || file_path.ends_with(".cjs")
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

fn make_ref(
    from_id: &str,
    name: &str,
    kind: ReferenceKind,
    line: i32,
    file_path: &str,
    lang: Language,
) -> UnresolvedReference {
    UnresolvedReference {
        from_node_id: from_id.to_string(),
        reference_name: name.to_string(),
        reference_kind: kind,
        line,
        column: 0,
        file_path: Some(file_path.to_string()),
        language: Some(lang),
        candidates: None,
    }
}

fn extract_tail_ident(expr: &str) -> Option<String> {
    let cleaned = expr.replace(' ', "").replace("()", "");
    let re = Regex::new(r"(?:\.|^)([A-Za-z_][A-Za-z0-9_]*)$").ok();
    re.and_then(|r| r.captures(&cleaned).and_then(|c| c.get(1).map(|m| m.as_str().to_string())))
}

fn extract_arrow_body(after_arrow: &str) -> &str {
    let brace_at = after_arrow.find('{');
    if let Some(b) = brace_at {
        if after_arrow[..b].trim().is_empty() {
            let end = match_delim(after_arrow, b, '{', '}');
            if end > b {
                return &after_arrow[b + 1..end];
            }
        }
    }
    after_arrow
}

fn match_delim(s: &str, open: usize, open_ch: char, close_ch: char) -> usize {
    let bytes = s.as_bytes();
    let open_b = open_ch as u8;
    let close_b = close_ch as u8;
    let mut depth = 0;
    let mut i = open;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' || b == b'\'' || b == b'`' {
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
        if b == open_b {
            depth += 1;
        } else if b == close_b {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
        i += 1;
    }
    usize::MAX
}