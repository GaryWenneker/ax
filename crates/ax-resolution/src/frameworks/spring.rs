//! Spring Boot route + config extraction and resolution - CG: frameworks/java.ts (springResolver).

use std::collections::HashSet;
use std::path::Path;

use ax_db::queries::QueryBuilder;
use ax_types::{Language, Node, NodeKind, ReferenceKind, SearchOptions, UnresolvedReference};

use crate::strip_comments::strip_comments_for_regex;
use crate::types::UnresolvedRef;

use super::extract::{now_ms, stable_node_id, FrameworkExtractResult};

const SERVICE_DIRS: &[&str] = &["/service/", "/services/"];
const REPO_DIRS: &[&str] = &["/repository/", "/repositories/"];
const CONTROLLER_DIRS: &[&str] = &["/controller/", "/controllers/"];
const ENTITY_DIRS: &[&str] = &["/entity/", "/entities/", "/model/", "/models/", "/domain/"];
const COMPONENT_DIRS: &[&str] = &["/component/", "/components/", "/config/"];

pub fn claims_reference(name: &str) -> bool {
    name.ends_with(":prefix")
}

pub fn detect(project_root: &Path, file_paths: &[String]) -> bool {
    if let Ok(pom) = std::fs::read_to_string(project_root.join("pom.xml")) {
        if pom.contains("spring-boot") || pom.contains("springframework") {
            return true;
        }
    }
    for gf in ["build.gradle", "build.gradle.kts"] {
        if let Ok(g) = std::fs::read_to_string(project_root.join(gf)) {
            if g.contains("spring-boot") || g.contains("springframework") {
                return true;
            }
        }
    }
    for f in file_paths {
        if f.ends_with(".java") {
            if let Ok(c) = std::fs::read_to_string(project_root.join(f)) {
                if c.contains("@SpringBootApplication")
                    || c.contains("@RestController")
                    || c.contains("@Service")
                    || c.contains("@Repository")
                {
                    return true;
                }
            }
        }
    }
    false
}

pub fn is_spring_config_path(file_path: &str) -> bool {
    is_spring_config_file(file_path)
}

pub fn extract_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    if is_spring_config_file(file_path) {
        return extract_spring_config(file_path, content);
    }
    if file_path.ends_with(".java") || file_path.ends_with(".kt") {
        return extract_java_file(file_path, content);
    }
    FrameworkExtractResult::default()
}

pub async fn try_resolve_target(
    queries: &QueryBuilder,
    ref_: &UnresolvedRef,
) -> Option<(ax_types::Node, f64)> {
    let name = &ref_.reference_name;

    if name.ends_with(":prefix") {
        let prefix = name.trim_end_matches(":prefix");
        let canon_prefix = canonical_config_key(prefix);
        if let Some(n) = find_config_node(queries, &canon_prefix, true).await {
            return Some((n, 0.85));
        }
        return None;
    }

    let lang = ref_.language;
    if (lang == Language::Java || lang == Language::Kotlin)
        && name.contains('.')
        && !name.contains("::")
        && name.split('.').count() >= 2
    {
        let canon_ref = canonical_config_key(name);
        if let Some(n) = find_config_node(queries, &canon_ref, false).await {
            return Some((n, 0.9));
        }
    }

    if name.ends_with("Service") {
        if let Some(n) = resolve_by_name_and_kind(queries, name, &[NodeKind::Class, NodeKind::Interface], SERVICE_DIRS).await {
            return Some((n, 0.85));
        }
    }
    if name.ends_with("Repository") {
        if let Some(n) = resolve_by_name_and_kind(queries, name, &[NodeKind::Class, NodeKind::Interface], REPO_DIRS).await {
            return Some((n, 0.85));
        }
    }
    if name.ends_with("Controller") {
        if let Some(n) = resolve_by_name_and_kind(queries, name, &[NodeKind::Class], CONTROLLER_DIRS).await {
            return Some((n, 0.85));
        }
    }
    if name.ends_with("Component") || name.ends_with("Config") {
        if let Some(n) = resolve_by_name_and_kind(queries, name, &[NodeKind::Class], COMPONENT_DIRS).await {
            return Some((n, 0.8));
        }
    }
    if is_pascal_entity(name) {
        if let Some(n) = resolve_by_name_and_kind(queries, name, &[NodeKind::Class], ENTITY_DIRS).await {
            return Some((n, 0.7));
        }
    }
    None
}

async fn find_config_node(
    queries: &QueryBuilder,
    canon_key: &str,
    prefix_match: bool,
) -> Option<ax_types::Node> {
    let opts = SearchOptions {
        kinds: Some(vec![NodeKind::Constant]),
        languages: Some(vec![Language::Yaml, Language::Properties]),
        limit: Some(200),
        ..Default::default()
    };
    let results = queries.search_nodes(canon_key, &opts).await.unwrap_or_default();
    let mut matches: Vec<_> = results
        .into_iter()
        .map(|r| r.node)
        .filter(|n| {
            let canon = canonical_config_key(&n.qualified_name);
            if prefix_match {
                canon.starts_with(canon_key)
            } else {
                canon == canon_key
            }
        })
        .collect();
    if matches.is_empty() {
        return None;
    }
    if prefix_match {
        matches.sort_by_key(|n| canonical_config_key(&n.qualified_name).len());
        return Some(matches[0].clone());
    }
    if matches.len() == 1 {
        return Some(matches[0].clone());
    }
    matches.sort_by_key(|n| config_file_score(&n.file_path));
    Some(matches[0].clone())
}

fn is_spring_config_file(file_path: &str) -> bool {
    let base = file_path.rsplit('/').next().unwrap_or(file_path);
    regex::Regex::new(r"^(application|bootstrap)(-[\w.-]+)?\.(yml|yaml|properties)$")
        .map(|re| re.is_match(base))
        .unwrap_or(false)
}

fn extract_spring_config(file_path: &str, content: &str) -> FrameworkExtractResult {
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();
    let is_properties = file_path.ends_with(".properties");
    let lang = if is_properties { Language::Properties } else { Language::Yaml };

    if is_properties {
        for (i, raw) in content.lines().enumerate() {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                continue;
            }
            let sep = raw.find('=').or_else(|| raw.find(':'));
            if sep.is_none() {
                continue;
            }
            let key = raw[..sep.unwrap()].trim();
            emit_config_leaf(&mut out, file_path, key, i + 1, lang, now);
        }
        return out;
    }

    let mut stack: Vec<(usize, String)> = Vec::new();
    for (i, raw) in content.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" || trimmed.starts_with("- ") {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let colon = raw.find(':');
        if colon.is_none() {
            continue;
        }
        let key = raw[indent..colon.unwrap()].trim();
        if key.is_empty() {
            continue;
        }
        let after = raw[colon.unwrap() + 1..].trim();
        while stack.last().map(|(ind, _)| indent <= *ind).unwrap_or(false) {
            stack.pop();
        }
        let dotted = if stack.is_empty() {
            key.to_string()
        } else {
            let prefix = stack.iter().map(|(_, k)| k.as_str()).collect::<Vec<_>>().join(".");
            format!("{prefix}.{key}")
        };
        if after.is_empty() || after.starts_with('#') {
            stack.push((indent, key.to_string()));
        } else {
            emit_config_leaf(&mut out, file_path, &dotted, i + 1, lang, now);
        }
    }
    out
}

fn emit_config_leaf(
    out: &mut FrameworkExtractResult,
    file_path: &str,
    dotted_key: &str,
    line: usize,
    lang: Language,
    now: i64,
) {
    if dotted_key.is_empty() {
        return;
    }
    let leaf = dotted_key.rsplit('.').next().unwrap_or(dotted_key);
    let qualified = format!("{}::config:{}", file_path, dotted_key);
    out.nodes.push(Node {
        id: stable_node_id(file_path, &qualified),
        kind: NodeKind::Constant,
        name: leaf.to_string(),
        qualified_name: dotted_key.to_string(),
        file_path: file_path.to_string(),
        language: lang,
        start_line: line as i32,
        end_line: line as i32,
        start_column: 0,
        end_column: 0,
        docstring: None,
        signature: Some(dotted_key.to_string()),
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

fn extract_java_file(file_path: &str, content: &str) -> FrameworkExtractResult {
    let lang = if file_path.ends_with(".kt") {
        Language::Kotlin
    } else {
        Language::Java
    };
    let safe = strip_comments_for_regex(content, lang);
    let mut out = FrameworkExtractResult::default();
    let now = now_ms();

    let class_prefix = extract_class_request_mapping_prefix(&safe);
    for (ann, verb) in [
        ("GetMapping", "GET"),
        ("PostMapping", "POST"),
        ("PutMapping", "PUT"),
        ("PatchMapping", "PATCH"),
        ("DeleteMapping", "DELETE"),
    ] {
        let pattern = format!(r"@{ann}\s*\(");
        let re = regex::Regex::new(&pattern).expect("mapping regex");
        for m in re.find_iter(&safe) {
            let args_end = read_balanced_paren_end(&safe, m.end() - 1);
            let args = if args_end != usize::MAX {
                safe[m.end()..args_end].trim()
            } else {
                ""
            };
            let sub = parse_mapping_path(args);
            let route_path = join_http_path(&class_prefix, &sub);
            let line = safe[..m.start()].matches('\n').count() as i32 + 1;
            let qualified = format!("{}::route:{}", file_path, route_path);
            let route_id = stable_node_id(file_path, &qualified);
            out.nodes.push(Node {
                id: route_id.clone(),
                kind: NodeKind::Route,
                name: format!("{verb} {route_path}"),
                qualified_name: qualified,
                file_path: file_path.to_string(),
                language: lang,
                start_line: line,
                end_line: line,
                start_column: 0,
                end_column: m.as_str().len() as i32,
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
            if let Some(handler) = method_name_after(&safe, args_end.saturating_add(1)) {
                out.references.push(UnresolvedReference {
                    from_node_id: route_id,
                    reference_name: handler,
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

    extract_spring_value_bindings(file_path, &safe, lang, now, &mut out);
    out
}

fn read_balanced_paren_end(s: &str, open: usize) -> usize {
    if s.get(open..open + 1) != Some("(") {
        return usize::MAX;
    }
    let mut depth = 0;
    for (i, ch) in s[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return open + i + 1;
                }
            }
            _ => {}
        }
    }
    usize::MAX
}

fn extract_class_request_mapping_prefix(safe: &str) -> String {
    let re = regex::Regex::new(
        r"@RequestMapping\s*\(([^)]*)\)\s*(?:@[\w.]+(?:\([^)]*\))?\s*)*(?:public\s+|final\s+|abstract\s+|open\s+|data\s+|sealed\s+)*class\b",
    )
    .expect("class mapping");
    if let Some(cap) = re.captures(safe) {
        parse_mapping_path(cap.get(1).map(|m| m.as_str()).unwrap_or(""))
    } else {
        String::new()
    }
}

fn extract_spring_value_bindings(
    file_path: &str,
    safe: &str,
    lang: Language,
    now: i64,
    out: &mut FrameworkExtractResult,
) {
    let value_re =
        regex::Regex::new(r#"@Value\s*\(\s*["']\$\{([^}:]+)(?::[^}]*)?\}["']\s*\)"#).expect("value");
    for cap in value_re.captures_iter(safe) {
        let key = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if key.is_empty() {
            continue;
        }
        let line = safe[..cap.get(0).unwrap().start()].matches('\n').count() as i32 + 1;
        let qualified = format!("{}::@Value:{}", file_path, key);
        let bind_id = stable_node_id(file_path, &qualified);
        out.nodes.push(Node {
            id: bind_id.clone(),
            kind: NodeKind::Constant,
            name: key.to_string(),
            qualified_name: qualified,
            file_path: file_path.to_string(),
            language: lang,
            start_line: line,
            end_line: line,
            start_column: 0,
            end_column: cap.get(0).unwrap().as_str().len() as i32,
            docstring: None,
            signature: Some(format!("@Value(\"{key}\")")),
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
            from_node_id: bind_id,
            reference_name: key.to_string(),
            reference_kind: ReferenceKind::References,
            line,
            column: 0,
            file_path: Some(file_path.to_string()),
            language: Some(lang),
            candidates: None,
        });
    }

    let cp_re = regex::Regex::new(
        r#"@ConfigurationProperties\s*\(\s*(?:prefix\s*=\s*)?["']([^"']+)["']"#,
    )
    .expect("cp");
    for cap in cp_re.captures_iter(safe) {
        let prefix = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");
        if prefix.is_empty() {
            continue;
        }
        let line = safe[..cap.get(0).unwrap().start()].matches('\n').count() as i32 + 1;
        let qualified = format!("{}::@ConfigurationProperties:{}", file_path, prefix);
        let bind_id = stable_node_id(file_path, &qualified);
        out.nodes.push(Node {
            id: bind_id.clone(),
            kind: NodeKind::Constant,
            name: prefix.to_string(),
            qualified_name: qualified,
            file_path: file_path.to_string(),
            language: lang,
            start_line: line,
            end_line: line,
            start_column: 0,
            end_column: cap.get(0).unwrap().as_str().len() as i32,
            docstring: None,
            signature: Some(format!("@ConfigurationProperties(\"{prefix}\")")),
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
            from_node_id: bind_id,
            reference_name: format!("{prefix}:prefix"),
            reference_kind: ReferenceKind::References,
            line,
            column: 0,
            file_path: Some(file_path.to_string()),
            language: Some(lang),
            candidates: None,
        });
    }
}

fn parse_mapping_path(args: &str) -> String {
    let re = regex::Regex::new(r#"["']([^"']*)["']"#).expect("path");
    re.captures(args)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

fn join_http_path(prefix: &str, sub: &str) -> String {
    let parts: Vec<_> = [prefix, sub]
        .iter()
        .map(|p| p.trim().trim_matches('/'))
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", parts.join("/"))
    }
}

fn method_name_after(safe: &str, start: usize) -> Option<String> {
    let tail = safe.get(start..start + 600.min(safe.len().saturating_sub(start)))?;
    let re =
        regex::Regex::new(r"\bfun\s+(\w+)\s*\(|\b(?:public|private|protected)\s+[^;{=]*?\s+(\w+)\s*\(")
            .expect("method");
    re.captures(tail)
        .and_then(|c| c.get(1).or_else(|| c.get(2)).map(|m| m.as_str().to_string()))
}

fn canonical_config_key(key: &str) -> String {
    key.to_lowercase().replace('-', "").replace('_', "")
}

fn config_file_score(file_path: &str) -> usize {
    let base = file_path.rsplit('/').next().unwrap_or(file_path);
    let is_base = regex::Regex::new(r"^(application|bootstrap)\.(yml|yaml|properties)$")
        .map(|re| re.is_match(base))
        .unwrap_or(false);
    (if is_base { 0 } else { 1 }) * 1000 + base.len()
}

async fn resolve_by_name_and_kind(
    queries: &QueryBuilder,
    name: &str,
    kinds: &[NodeKind],
    dirs: &[&str],
) -> Option<ax_types::Node> {
    let candidates = queries.get_nodes_by_name(name).await.unwrap_or_default();
    let kind_set: HashSet<_> = kinds.iter().copied().collect();
    let filtered: Vec<_> = candidates.into_iter().filter(|n| kind_set.contains(&n.kind)).collect();
    if filtered.is_empty() {
        return None;
    }
    let preferred: Vec<_> = filtered
        .iter()
        .filter(|n| dirs.iter().any(|d| n.file_path.contains(d)))
        .collect();
    if let Some(n) = preferred.first() {
        return Some((*n).clone());
    }
    filtered.first().cloned()
}

fn is_pascal_entity(s: &str) -> bool {
    let mut chars = s.chars();
    let first = chars.next();
    first.map(|c| c.is_uppercase() && c.is_alphabetic()).unwrap_or(false)
        && chars.all(|c| c.is_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_mapping_route() {
        let src = "@RestController @RequestMapping(\"/api\") class C { @GetMapping(\"/users\") public void list() {} }";
        let r = extract_java_file("src/UserController.java", src);
        assert!(r.nodes.iter().any(|n| n.name.contains("GET") && n.name.contains("/api/users")));
    }

    #[test]
    fn yaml_config_leaf() {
        let yaml = "app:\n  cache:\n    ttl: 30\n";
        let r = extract_spring_config("application.yml", yaml);
        assert!(r.nodes.iter().any(|n| n.qualified_name == "app.cache.ttl"));
    }

    #[test]
    fn value_binding_reference() {
        let src = "@Value(\"${app.cache.ttl}\") private int ttl;";
        let r = extract_java_file("Config.java", src);
        assert!(r.references.iter().any(|ref_| ref_.reference_name == "app.cache.ttl"));
    }
}
