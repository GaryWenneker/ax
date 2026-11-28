//! Normalize memory text for FTS / BM25F (Phase 6).

use rust_stemmers::{Algorithm, Stemmer};

/// Split camelCase, snake_case, and path separators; fold to lowercase; stem EN+NL.
pub fn normalize_search_text(input: &str) -> String {
    let en = Stemmer::create(Algorithm::English);
    let nl = Stemmer::create(Algorithm::Dutch);
    let mut out: Vec<String> = Vec::new();
    for raw in input.split(|c: char| {
        c.is_whitespace()
            || matches!(
                c,
                '/' | '\\' | '.' | '-' | ':' | ',' | ';' | '|' | '(' | ')'
            )
    }) {
        if raw.is_empty() {
            continue;
        }
        for piece in raw.split('_') {
            push_camel_pieces(&mut out, piece, &en, &nl);
        }
    }
    out.join(" ")
}

fn push_camel_pieces(out: &mut Vec<String>, piece: &str, en: &Stemmer, nl: &Stemmer) {
    if piece.is_empty() {
        return;
    }
    let mut current = String::new();
    let mut chars = piece.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_uppercase()
            && !current.is_empty()
            && chars
                .peek()
                .map(|next| next.is_ascii_lowercase() || next.is_ascii_digit())
                .unwrap_or(true)
        {
            push_stemmed(out, &current, en, nl);
            current.clear();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        push_stemmed(out, &current, en, nl);
    }
}

fn push_stemmed(out: &mut Vec<String>, token: &str, en: &Stemmer, nl: &Stemmer) {
    let lower = token.to_ascii_lowercase();
    if lower.is_empty() {
        return;
    }
    // Each word adds its own lowercase form and its distinct stems, so the
    // result for a word never depends on the rest of the text.
    let en_stem = en.stem(&lower).into_owned();
    let nl_stem = nl.stem(&lower).into_owned();
    let add_nl = nl_stem != lower && nl_stem != en_stem;
    let add_en = en_stem != lower;
    out.push(lower);
    if add_en {
        out.push(en_stem);
    }
    if add_nl {
        out.push(nl_stem);
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_search_text;

    #[test]
    fn splits_camel_snake_and_paths() {
        let text = normalize_search_text("parseHttpResponse_from_path");
        for token in ["parse", "http", "response", "from", "path"] {
            assert!(
                text.split_whitespace()
                    .any(|t| t == token || t.starts_with(token)),
                "missing {token} in {text:?}"
            );
        }
        let path = normalize_search_text("src/FooBar.rs");
        for token in ["src", "foo", "bar", "rs"] {
            assert!(
                path.split_whitespace()
                    .any(|t| t == token || t.starts_with(token)),
                "missing {token} in {path:?}"
            );
        }
        assert_eq!(normalize_search_text(""), "");
        let stemmed = normalize_search_text("running runners");
        assert!(stemmed.contains("run"), "{stemmed}");
    }

    /// Each word's tokens depend only on that word, never on what came before.
    #[test]
    fn a_word_normalizes_the_same_wherever_it_appears() {
        let alone = normalize_search_text("lopen");
        assert!(alone.split_whitespace().count() > 1, "{alone}");
        assert_eq!(
            normalize_search_text("lopen lopen"),
            format!("{alone} {alone}")
        );
    }

    /// Large documents normalize in linear time.
    #[test]
    fn a_large_document_normalizes_quickly() {
        let words: Vec<String> = (0..40_000u32)
            .map(|mut n| {
                let mut w = String::from("w");
                for _ in 0..4 {
                    w.push((b'a' + (n % 26) as u8) as char);
                    n /= 26;
                }
                w.push_str("ingen");
                w
            })
            .collect();
        let text = words.join(" ");
        let started = std::time::Instant::now();
        let out = normalize_search_text(&text);
        assert!(out.split_whitespace().count() >= words.len());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(3),
            "took {:?}",
            started.elapsed()
        );
    }
}
