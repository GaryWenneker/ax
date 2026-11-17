//! Prospective memories: fire when a glob/symbol/error matches (Phase 9).

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhenClause {
    pub kind: String,
    pub pattern: String,
}

pub fn parse_when(raw: &str) -> Option<WhenClause> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some((kind, pattern)) = raw.split_once(':') {
        let kind = kind.trim().to_ascii_lowercase();
        let pattern = pattern.trim().to_string();
        if kind.is_empty() || pattern.is_empty() {
            return None;
        }
        if matches!(kind.as_str(), "file" | "symbol" | "error" | "command") {
            return Some(WhenClause { kind, pattern });
        }
        return None;
    }
    // Bare glob defaults to file.
    Some(WhenClause {
        kind: "file".into(),
        pattern: raw.to_string(),
    })
}

pub fn when_matches(clause: &WhenClause, kind: &str, value: &str) -> bool {
    if clause.kind != kind {
        return false;
    }
    if clause.pattern.contains('*') {
        let prefix = clause.pattern.split('*').next().unwrap_or("");
        value.starts_with(prefix) || value.contains(prefix)
    } else {
        value == clause.pattern || value.contains(&clause.pattern)
    }
}

/// Tags like `when:file:src/*.rs` that fire for a path.
pub fn prospective_tags_for_path(tags: &[String], path: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for tag in tags {
        let Some(raw) = tag.strip_prefix("when:") else {
            continue;
        };
        let Some(clause) = parse_when(raw) else {
            continue;
        };
        if when_matches(&clause, "file", path) {
            hits.push(tag.clone());
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::{parse_when, prospective_tags_for_path, when_matches};

    #[test]
    fn parses_and_matches_file_globs() {
        let clause = parse_when("file:src/*.rs").unwrap();
        assert_eq!(clause.kind, "file");
        assert!(when_matches(&clause, "file", "src/a.rs"));
        assert!(!when_matches(&clause, "symbol", "src/a.rs"));
        assert!(parse_when("").is_none());
        assert!(parse_when("other:x").is_none());
        let hits =
            prospective_tags_for_path(&["when:file:src/*.rs".into(), "other".into()], "src/a.rs");
        assert_eq!(hits, vec!["when:file:src/*.rs".to_string()]);
    }
}
