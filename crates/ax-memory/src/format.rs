//! Inject-block formatting for preflight.

use crate::types::MemoryMatch;

/// Render matched memories as an XML block appended to the preflight inject.
/// Returns an empty string when there is nothing to inject.
pub fn format_memories_inject_block(matches: &[MemoryMatch], max_chars: usize) -> String {
    if matches.is_empty() {
        return String::new();
    }
    let mut body = String::from(
        "<ax_memories note=\"Durable project memories matched for this prompt — treat as established context.\">\n",
    );
    for m in matches {
        let mut entry = format!("### [{}] {}\n\n{}\n\n", m.memory.kind, m.memory.title, m.memory.body.trim());
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
