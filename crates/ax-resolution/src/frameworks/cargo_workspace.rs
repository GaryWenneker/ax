//! Cargo workspace crate map — CG: frameworks/cargo-workspace.ts

use std::collections::{HashMap, HashSet};
use std::path::Path;

const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", "dist", "build"];
const MAX_GLOB_WALK_DEPTH: usize = 5;
const GLOB_CHARS: &[char] = &['*', '?', '[', '{', '}', '!'];

pub fn load_crate_map(project_root: &Path) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let root_cargo = project_root.join("Cargo.toml");
    let content = std::fs::read_to_string(&root_cargo).unwrap_or_default();
    if content.is_empty() {
        return result;
    }

    let raw_members = parse_workspace_members(&content);
    let members = expand_members(&raw_members, project_root);
    for member_path in members {
        let member_cargo = project_root.join(&member_path).join("Cargo.toml");
        let member_toml = std::fs::read_to_string(&member_cargo).unwrap_or_default();
        if member_toml.is_empty() {
            continue;
        }
        let package_name = parse_package_name(&member_toml);
        if let Some(name) = package_name {
            add_crate_alias(&mut result, name, member_path);
        }
    }
    result
}

fn add_crate_alias(map: &mut HashMap<String, String>, crate_name: String, member_path: String) {
    let normalized = crate_name.replace('-', "_");
    map.insert(crate_name.clone(), member_path.clone());
    if normalized != crate_name {
        map.insert(normalized, member_path);
    }
}

fn parse_package_name(cargo_toml: &str) -> Option<String> {
    let section = get_section(cargo_toml, "package")?;
    let re = regex::Regex::new(r#"name\s*=\s*["']([^"'\n]+)["']"#).expect("package name");
    re.captures(&section)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
}

fn parse_workspace_members(cargo_toml: &str) -> Vec<String> {
    let section = match get_section(cargo_toml, "workspace") {
        Some(s) => s,
        None => return Vec::new(),
    };
    let members_value = match get_array_value(&section, "members") {
        Some(v) => v,
        None => return Vec::new(),
    };
    extract_quoted_values(&members_value)
}

fn has_glob_chars(member: &str) -> bool {
    member.chars().any(|c| GLOB_CHARS.contains(&c))
}

fn clean_path(member_path: &str) -> String {
    member_path.replace('\\', "/").trim_end_matches('/').to_string()
}

/// CG: `expandMembers` + `expandGlobMember` (picomatch → `glob::Pattern`).
fn expand_members(raw_members: &[String], project_root: &Path) -> Vec<String> {
    let mut expanded = Vec::new();
    let mut seen = HashSet::new();
    for member in raw_members {
        let candidates = if has_glob_chars(member) {
            expand_glob_member(member, project_root)
        } else {
            vec![clean_path(member)]
        };
        for candidate in candidates {
            if seen.insert(candidate.clone()) {
                expanded.push(candidate);
            }
        }
    }
    expanded
}

fn expand_glob_member(member: &str, project_root: &Path) -> Vec<String> {
    let normalized = member.replace('\\', "/");
    let first_glob = normalized
        .chars()
        .position(|c| GLOB_CHARS.contains(&c))
        .unwrap_or(normalized.len());
    let static_prefix = normalized[..first_glob]
        .rfind('/')
        .map(|i| normalized[..i].trim_end_matches('/'))
        .unwrap_or("");

    let start_dir = if static_prefix.is_empty() {
        project_root.to_path_buf()
    } else {
        project_root.join(static_prefix)
    };

    let mut matches = Vec::new();
    let mut seen = HashSet::new();
    walk_glob_dirs(
        &start_dir,
        static_prefix,
        &normalized,
        0,
        &mut matches,
        &mut seen,
    );
    matches
}

fn walk_glob_dirs(
    dir: &Path,
    rel_base: &str,
    pattern: &str,
    depth: usize,
    matches: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    if depth > MAX_GLOB_WALK_DEPTH {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if SKIP_DIRS.contains(&name.as_str()) || name.starts_with('.') {
            continue;
        }
        let rel = if rel_base.is_empty() {
            name.clone()
        } else {
            format!("{}/{}", rel_base, name)
        };
        if glob_pattern_matches(pattern, &rel) && seen.insert(rel.clone()) {
            matches.push(rel.clone());
        }
        let path = entry.path();
        if path.is_dir() {
            walk_glob_dirs(&path, &rel, pattern, depth + 1, matches, seen);
        }
    }
}

fn glob_pattern_matches(pattern: &str, path: &str) -> bool {
    // Picomatch-style: `*` and `?` do not cross `/` (CG cargo-workspace.ts + picomatch).
    let re = glob_pattern_to_regex(pattern);
    regex::Regex::new(&re)
        .map(|r| r.is_match(path))
        .unwrap_or(false)
}

fn glob_pattern_to_regex(pattern: &str) -> String {
    let mut re = String::from("^");
    for ch in pattern.chars() {
        match ch {
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '@' | '%' => {
                re.push('\\');
                re.push(ch);
            }
            '[' => {
                re.push(ch);
            }
            ']' => {
                re.push(ch);
            }
            _ => re.push(ch),
        }
    }
    re.push('$');
    re
}

fn get_section(content: &str, section_name: &str) -> Option<String> {
    let header = format!("[{section_name}]");
    let mut in_section = false;
    let mut lines: Vec<String> = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if !in_section {
            if trimmed == header {
                in_section = true;
            }
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            break;
        }
        lines.push(line.to_string());
    }
    if in_section {
        Some(lines.join("\n"))
    } else {
        None
    }
}

fn get_array_value(section: &str, key: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(r"\b{}\s*=", regex::escape(key))).expect("key re");
    let m = re.find(section)?;
    let mut i = m.end();
    while i < section.len() && section.as_bytes()[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= section.len() || section.as_bytes()[i] != b'[' {
        return None;
    }
    i += 1;
    let mut depth = 1;
    let start = i;
    while i < section.len() {
        let b = section.as_bytes()[i];
        if b == b'[' {
            depth += 1;
        } else if b == b']' {
            depth -= 1;
            if depth == 0 {
                return Some(section[start..i].to_string());
            }
        }
        i += 1;
    }
    None
}

fn extract_quoted_values(value_list: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut quote: Option<char> = None;
    let mut current = String::new();
    let mut escaped = false;
    for ch in value_list.chars() {
        if quote.is_none() {
            if ch == '"' || ch == '\'' {
                quote = Some(ch);
                current.clear();
            }
            continue;
        }
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote.unwrap() {
            let v = current.trim();
            if !v.is_empty() {
                values.push(v.to_string());
            }
            quote = None;
            continue;
        }
        current.push(ch);
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn glob_pattern_matches_crate_star() {
        assert!(glob_pattern_matches("crates/*", "crates/ax-db"));
        assert!(!glob_pattern_matches("crates/*", "crates/ax-db/src"));
    }

    #[test]
    fn workspace_glob_smoke_fixture() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../test-smoke-workspace");
        if !root.join("Cargo.toml").exists() {
            return;
        }
        let map = load_crate_map(&root);
        assert!(map.contains_key("alpha-crate"));
        assert!(map.contains_key("beta-crate"));
        assert_eq!(map.get("alpha-crate").map(|s| s.as_str()), Some("crates/alpha"));
    }
}
