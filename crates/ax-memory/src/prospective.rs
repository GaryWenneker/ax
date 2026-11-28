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

/// Whether `value` of the given `kind` fires `clause`.
///
/// File patterns are globs matched against the whole path (`/` separators, a
/// leading `./` ignored): `*` stays within one folder and `**` spans folders.
/// A file pattern without `/` also matches the file name alone, so `*.sql` and
/// `Cargo.toml` match in any folder. For other kinds, a pattern with glob
/// characters must match the whole value; a plain pattern matches anywhere in it.
pub fn when_matches(clause: &WhenClause, kind: &str, value: &str) -> bool {
    if clause.kind != kind {
        return false;
    }
    if kind == "file" {
        return file_matches(&clause.pattern, value);
    }
    if has_glob_chars(&clause.pattern) {
        globset::GlobBuilder::new(&clause.pattern)
            .build()
            .is_ok_and(|g| g.compile_matcher().is_match(value))
    } else {
        value.contains(&clause.pattern)
    }
}

fn has_glob_chars(pattern: &str) -> bool {
    pattern.contains(['*', '?', '[', '{'])
}

fn normalize_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    let mut rest = path.as_str();
    while let Some(stripped) = rest.strip_prefix("./") {
        rest = stripped;
    }
    rest.to_string()
}

fn file_matches(pattern: &str, path: &str) -> bool {
    let pattern = normalize_path(pattern);
    let path = normalize_path(path);
    let Ok(glob) = globset::GlobBuilder::new(&pattern)
        .literal_separator(true)
        .build()
    else {
        return false;
    };
    let matcher = glob.compile_matcher();
    if matcher.is_match(&path) {
        return true;
    }
    !pattern.contains('/') && matcher.is_match(path.rsplit('/').next().unwrap_or(&path))
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

    #[test]
    fn file_globs_match_the_whole_path() {
        let rs = parse_when("file:src/*.rs").unwrap();
        assert!(when_matches(&rs, "file", "src/app.rs"));
        assert!(!when_matches(&rs, "file", "src/app.ts"));
        assert!(!when_matches(&rs, "file", "vendor/src/x.rs"));
        assert!(!when_matches(&rs, "file", "vendor/src/x.py"));

        let sql = parse_when("file:*.sql").unwrap();
        assert!(when_matches(&sql, "file", "schema.sql"));
        assert!(when_matches(&sql, "file", "db/migrations/001.sql"));
        assert!(!when_matches(&sql, "file", "src/main.rs"));

        let deep = parse_when("file:crates/**/*.rs").unwrap();
        assert!(when_matches(&deep, "file", "crates/a/src/lib.rs"));
        assert!(!when_matches(&deep, "file", "crates/a/README.md"));
        assert!(when_matches(&deep, "file", "./crates/a/src/lib.rs"));
    }

    #[test]
    fn plain_file_patterns_match_the_path_or_file_name_exactly() {
        let exact = parse_when("file:src/lib.rs").unwrap();
        assert!(when_matches(&exact, "file", "src/lib.rs"));
        assert!(!when_matches(&exact, "file", "vendor/src/lib.rs"));
        assert!(!when_matches(&exact, "file", "src/lib.rs.bak"));

        let name = parse_when("file:Cargo.toml").unwrap();
        assert!(when_matches(&name, "file", "crates/a/Cargo.toml"));
        assert!(!when_matches(&name, "file", "crates/a/Cargo.toml.orig"));
    }

    #[test]
    fn non_file_globs_check_the_suffix() {
        let sym = parse_when("symbol:parse_*_config").unwrap();
        assert!(when_matches(&sym, "symbol", "parse_web_config"));
        assert!(!when_matches(&sym, "symbol", "parse_web"));
        let err = parse_when("error:database is locked").unwrap();
        assert!(when_matches(&err, "error", "error: database is locked (5)"));
    }
}
