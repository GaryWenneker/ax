//! Search query parser — kind:/lang:/path:/name: filters.

use ax_types::{Language, NodeKind};

#[derive(Debug, Clone, Default)]
pub struct ParsedQuery {
    pub text: String,
    pub kinds: Vec<NodeKind>,
    pub languages: Vec<Language>,
    pub path_filters: Vec<String>,
    pub name_filters: Vec<String>,
}

pub fn parse_query(raw: &str) -> ParsedQuery {
    let mut out = ParsedQuery::default();
    let tokens: Vec<&str> = raw.split_whitespace().collect();
    let mut text_parts = Vec::new();
    for tok in tokens {
        if let Some((key, value)) = tok.split_once(':') {
            let key = key.to_lowercase();
            let value = unquote(value);
            match key.as_str() {
                "kind" => {
                    if let Some(k) = NodeKind::from_str(&value) { out.kinds.push(k); } else { text_parts.push(tok); }
                }
                "lang" | "language" => {
                    if let Some(l) = Language::from_str(&value.to_lowercase()) { out.languages.push(l); } else { text_parts.push(tok); }
                }
                "path" => out.path_filters.push(value),
                "name" => out.name_filters.push(value),
                _ => text_parts.push(tok),
            }
        } else { text_parts.push(tok); }
    }
    out.text = text_parts.join(" ");
    out
}

fn unquote(s: &str) -> String {
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') { s[1..s.len()-1].to_string() } else { s.to_string() }
}

pub fn bounded_edit_distance(a: &str, b: &str, max_dist: usize) -> usize {
    if a == b { return 0; }
    let al = a.len(); let bl = b.len();
    if al.abs_diff(bl) > max_dist { return max_dist + 1; }
    if al == 0 { return bl; }
    if bl == 0 { return al; }
    let mut prev: Vec<usize> = (0..=bl).collect();
    let mut cur = vec![0; bl + 1];
    for (i, ac) in a.bytes().enumerate() {
        cur[0] = i + 1;
        let mut row_min = cur[0];
        for (j, bc) in b.bytes().enumerate() {
            let cost = if ac == bc { 0 } else { 1 };
            cur[j + 1] = (cur[j] + 1).min(prev[j + 1] + 1).min(prev[j] + cost);
            row_min = row_min.min(cur[j + 1]);
        }
        if row_min > max_dist { return max_dist + 1; }
        prev.clone_from_slice(&cur);
    }
    prev[bl]
}
#[cfg(test)]
mod tests {
    use super::*;
    use ax_types::{Language, NodeKind};

    #[test]
    fn parses_kind_and_lang_filters() {
        let q = parse_query("kind:function lang:typescript greet");
        assert_eq!(q.kinds, vec![NodeKind::Function]);
        assert_eq!(q.languages, vec![Language::Typescript]);
        assert_eq!(q.text, "greet");
    }

    #[test]
    fn parses_path_filter() {
        let q = parse_query("path:src/** handler");
        assert_eq!(q.path_filters, vec!["src/**"]);
        assert_eq!(q.text, "handler");
    }
}