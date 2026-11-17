//! Origin, author, trust, and commit stamps on memories.

/// Trusted vault text vs imported/doc text that must be treated as data.
pub fn trust_level_for(source: &str, kind: &str) -> &'static str {
    let source = source.trim().to_ascii_lowercase();
    let kind = kind.trim().to_ascii_lowercase();
    if source == "foreign"
        || source == "import"
        || source == "blossom"
        || kind == "doc"
        || kind == "git"
    {
        "untrusted"
    } else {
        "trusted"
    }
}

pub fn origin_for(source: &str) -> String {
    let source = source.trim();
    if source.is_empty() {
        "manual".into()
    } else {
        source.to_string()
    }
}
