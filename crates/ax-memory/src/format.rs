//! Inject-block formatting for preflight.

use crate::types::MemoryMatch;

/// Render matched memories as an XML block appended to the preflight inject.
/// Returns an empty string when there is nothing to inject.
pub fn format_memories_inject_block(matches: &[MemoryMatch], max_chars: usize) -> String {
    if matches.is_empty() {
        return String::new();
    }
    let mut body = String::from(
        "<ax_memories note=\"Durable project memories. Prompt matches come first; newest memories fill the rest so the vault is available on the next turn.\">\n",
    );
    for m in matches {
        let mut entry = format!(
            "### [{}] {}\n\n{}\n\n",
            m.memory.kind,
            m.memory.title,
            m.memory.body.trim()
        );
        if !m.memory.files.is_empty() {
            entry.push_str(&format!("Files: {}\n\n", m.memory.files.join(", ")));
        }
        if body.len() + entry.len() + 16 > max_chars {
            break;
        }
        body.push_str(&entry);
    }
    body.push_str("</ax_memories>\n");
    body
}

/// Hard cap for the preflight memory block. Titles only; a body is an `ax_recall`.
pub const PREFLIGHT_MEMORY_CHARS: usize = 6_000;

/// Matched memories as titles and ids only; `ax_recall` returns a body on demand.
pub fn format_memory_match_titles(matches: &[MemoryMatch]) -> String {
    if matches.is_empty() {
        return String::new();
    }
    let close = "</ax_memories>\n";
    let mut body = String::from(
        "<ax_memories note=\"Project memories matched for this prompt. Titles only; call ax_recall with the id or title for a body.\">\n",
    );
    for m in matches {
        let entry = format!(
            "- {} [{}] {}\n",
            m.memory.id,
            m.memory.kind,
            m.memory.title.replace('\n', " ")
        );
        if body.len() + entry.len() + close.len() > PREFLIGHT_MEMORY_CHARS {
            break;
        }
        body.push_str(&entry);
    }
    body.push_str(close);
    body
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MemoryRow;

    fn matched(id: &str, title: &str, body: &str) -> MemoryMatch {
        MemoryMatch {
            memory: MemoryRow {
                id: id.into(),
                kind: "decision".into(),
                title: title.into(),
                body: body.into(),
                tags: vec![],
                files: vec!["src/a.rs".into()],
                confidence: 1.0,
                source: "mcp".into(),
                enabled: true,
                created_at: 0,
                updated_at: 0,
                tier: "t2".into(),
            },
            score: 1.0,
        }
    }

    #[test]
    fn match_titles_list_id_and_title_without_body() {
        let out =
            format_memory_match_titles(&[matched("m1", "Use FNV for hashes", "LONG-BODY-TEXT")]);
        assert!(out.contains("m1"), "{out}");
        assert!(out.contains("[decision] Use FNV for hashes"), "{out}");
        assert!(out.contains("ax_recall"), "{out}");
        assert!(!out.contains("LONG-BODY-TEXT"), "{out}");
    }

    #[test]
    fn match_titles_empty_for_no_matches() {
        assert_eq!(format_memory_match_titles(&[]), "");
    }

    #[test]
    fn preflight_titles_stop_at_6000_bytes() {
        let title = "x".repeat(400);
        let matches: Vec<_> = (0..40)
            .map(|i| matched(&format!("m{i}"), &title, "BODY"))
            .collect();
        let out = format_memory_match_titles(&matches);
        assert!(out.len() <= 6_000, "len {}", out.len());
        assert!(out.contains("m0"), "{out}");
        assert!(!out.contains("BODY"), "{out}");
    }
}
