//! NestJS HTTP route extraction (@Controller + @Get/@Post/...).

use regex::Regex;

use ax_types::{Node, NodeKind, ReferenceKind, UnresolvedReference};

use crate::strip_comments::strip_comments_for_regex;

use super::extract::{language_for_path, now_ms, stable_node_id, FrameworkExtractResult};

const HTTP_METHODS: &[&str] = &["Get", "Post", "Put", "Patch", "Delete", "Head", "Options", "All"];

pub fn detect(project_root: &std::path::Path) -> bool {
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
            if keys.iter().any(|k| k.starts_with("@nestjs/")) {
                return true;
            }
        }
    }
    false
}

pub fn extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    if !file_path.ends_with(".ts")
        && !file_path.ends_with(".js")
        && !file_path.ends_with(".mts")
        && !file_path.ends_with(".cjs")
    {
        return FrameworkExtractResult::default();
    }

    let lang = language_for_path(file_path);
    let safe = strip_comments_for_regex(content, lang);
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();

    for method in HTTP_METHODS {
        let pattern = format!(r"@({})\s*\(", method);
        let re = Regex::new(&pattern).expect("method regex");
        for m in re.find_iter(&safe) {
            let open = m.end() - 1;
            let args_end = read_balanced_paren_end(&safe, open);
            if args_end == usize::MAX {
                continue;
            }
            let args = safe[open + 1..args_end].trim();
            let method_path = parse_string_arg(args);
            let prefix = controller_prefix_before(&safe, m.start());
            let path = join_http_path(&prefix, &method_path);
            let line = safe[..m.start()].matches('\n').count() as i32 + 1;
            let handler = method_name_after(&safe, args_end + 1);
            let qualified = format!("{}::{}:{}", file_path, method.to_uppercase(), path);
            let route_id = stable_node_id(file_path, &qualified);

            out.nodes.push(Node {
                id: route_id.clone(),
                kind: NodeKind::Route,
                name: format!("{} {}", method.to_uppercase(), path),
                qualified_name: qualified,
                file_path: file_path.to_string(),
                language: lang,
                start_line: line,
                end_line: line,
                start_column: 0,
                end_column: (args_end - m.start()) as i32,
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

            if let Some(handler_name) = handler {
                out.references.push(UnresolvedReference {
                    from_node_id: route_id,
                    reference_name: handler_name,
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

    out
}

fn controller_prefix_before(safe: &str, index: usize) -> String {
    let before = &safe[..index];
    let re = Regex::new(r#"@Controller\s*\(\s*["']([^"']*)["']\s*\)"#).expect("controller re");
    re.captures_iter(before)
        .last()
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

fn parse_string_arg(args: &str) -> String {
    let re = Regex::new(r#"['"]([^'"]*)['"]"#).expect("str arg");
    re.captures(args)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

fn join_http_path(prefix: &str, path: &str) -> String {
    let p = prefix.trim().trim_matches('/');
    let m = path.trim();
    if p.is_empty() {
        if m.is_empty() {
            "/".to_string()
        } else if m.starts_with('/') {
            m.to_string()
        } else {
            format!("/{}", m)
        }
    } else if m.is_empty() {
        format!("/{}", p)
    } else {
        format!("/{}/{}", p, m.trim_start_matches('/'))
    }
}

fn method_name_after(safe: &str, from: usize) -> Option<String> {
    let tail = safe.get(from..).unwrap_or("");
    let re = Regex::new(r"\b(?:async\s+)?(\w+)\s*\(").expect("method name");
    re.captures(tail).and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

fn read_balanced_paren_end(s: &str, open: usize) -> usize {
    let bytes = s.as_bytes();
    if open >= bytes.len() || bytes[open] != b'(' {
        return usize::MAX;
    }
    let mut depth = 0;
    let mut i = open;
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
                return i;
            }
        }
        i += 1;
    }
    usize::MAX
}
