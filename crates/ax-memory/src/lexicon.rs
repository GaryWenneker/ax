//! Reviewed Dutch–English software synonym sets (Phase 6).

/// Expand a query token with bilingual / synonym peers (lowercase).
pub fn expand_token(token: &str) -> Vec<String> {
    let t = token.trim().to_ascii_lowercase();
    if t.is_empty() {
        return Vec::new();
    }
    for group in SYNONYM_GROUPS {
        if group.iter().any(|w| *w == t) {
            return group.iter().map(|w| (*w).to_string()).collect();
        }
    }
    vec![t]
}

const SYNONYM_GROUPS: &[&[&str]] = &[
    &["delete", "remove", "drop", "verwijder", "verwijderen"],
    &["bug", "defect", "error", "fout", "issue"],
    &["fix", "repair", "herstel", "oplossen"],
    &["file", "bestand", "path", "pad"],
    &["test", "toets", "spec"],
];

#[cfg(test)]
mod tests {
    use super::expand_token;

    #[test]
    fn dutch_bug_expands_to_english_peers() {
        let expanded = expand_token("fout");
        assert!(expanded.iter().any(|w| w == "bug"));
        assert!(expanded.iter().any(|w| w == "error"));
        assert_eq!(expand_token(""), Vec::<String>::new());
        assert_eq!(
            expand_token("unique-token"),
            vec!["unique-token".to_string()]
        );
    }
}
