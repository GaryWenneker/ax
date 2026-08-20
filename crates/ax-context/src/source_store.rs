//! Graph-only source resolution for query-time snippets.
//!
//! Every snippet an agent sees comes from `file_contents` in `ax.db`, never from
//! the working tree. That is the whole point: a graph query must be answerable
//! from the graph, so its cost and its answer do not depend on a filesystem
//! sweep. See `docs/audits/2026-08-19-preflight-graph-only/`.
//!
//! Freshness is not assumed, it is checked. Stored text carries the hash it had
//! at index time; a read compares that against `files.content_hash` and refuses
//! to present a mismatch as current. There is deliberately **no disk fallback**:
//! a fallback would make the guarantee unobservable, since the failure mode it
//! hides is exactly the one we need to see.

use ax_db::queries::{QueryBuilder, SourceLookup};
use ax_utils::errors::AxError;

/// Shown when the graph has no stored text for a path.
pub const NOT_STORED_MARKER: &str = "source not stored";
/// Shown when stored text exists but no longer matches the indexed hash.
pub const STALE_MARKER: &str = "source stale";

/// Outcome of resolving one file's source from the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedSource {
    /// Stored text whose hash matches the indexed graph.
    Fresh(String),
    /// Stored text that predates the current file. Callers must label it.
    Stale(String),
    /// Nothing stored for an indexed path — needs a re-index, or the file is
    /// over the store cap.
    NotStored { over_cap: bool },
    /// The path is not in the graph at all.
    NotIndexed,
}

/// Resolve a project-relative path to source text using only the database.
pub async fn resolve_source(
    queries: &QueryBuilder,
    file_path: &str,
) -> Result<ResolvedSource, AxError> {
    let Some(lookup) = queries.lookup_source(file_path).await? else {
        return Ok(ResolvedSource::NotIndexed);
    };
    Ok(classify(lookup))
}

fn classify(lookup: SourceLookup) -> ResolvedSource {
    match lookup.stored_content {
        None => ResolvedSource::NotStored {
            over_cap: lookup.indexed_size > ax_db::source_store_cap_bytes() as i64,
        },
        Some(content) => {
            let fresh = lookup
                .stored_hash
                .as_deref()
                .is_some_and(|h| !lookup.indexed_hash.is_empty() && h == lookup.indexed_hash);
            if fresh {
                ResolvedSource::Fresh(content)
            } else {
                ResolvedSource::Stale(content)
            }
        }
    }
}

/// Human-readable reason for a resolution that produced no usable text.
pub fn unavailable_reason(resolved: &ResolvedSource, file_path: &str) -> Option<String> {
    match resolved {
        ResolvedSource::Fresh(_) | ResolvedSource::Stale(_) => None,
        ResolvedSource::NotStored { over_cap: true } => Some(format!(
            "({NOT_STORED_MARKER}: {file_path} exceeds the {} byte source-store cap)",
            ax_db::source_store_cap_bytes()
        )),
        ResolvedSource::NotStored { over_cap: false } => Some(format!(
            "({NOT_STORED_MARKER}: {file_path} — run ax index to backfill the source store)"
        )),
        ResolvedSource::NotIndexed => {
            Some(format!("({NOT_STORED_MARKER}: {file_path} is not indexed)"))
        }
    }
}

/// Slice `content` to a node's line range and number the lines.
///
/// `start_line` is 1-based and inclusive, `end_line` inclusive — the same
/// convention the graph stores.
pub fn numbered_slice(
    content: &str,
    start_line: i32,
    end_line: i32,
    max_lines: usize,
    max_chars: usize,
    sep: char,
) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let (start, end) = line_bounds(start_line, end_line, lines.len());
    let slice = lines.get(start..end).unwrap_or(&[]);
    let truncated_lines = slice.len() > max_lines;
    let out = slice
        .iter()
        .take(max_lines)
        .enumerate()
        .map(|(i, line)| format!("{}{}{}", start + i + 1, sep, line))
        .collect::<Vec<_>>()
        .join("\n");
    if out.len() > max_chars {
        format!(
            "{}\n...(truncated to {} chars; increase maxSourceChars)",
            truncate_on_char_boundary(&out, max_chars),
            max_chars
        )
    } else if truncated_lines {
        format!(
            "{}\n...(truncated to {} lines; off-spine signatures omitted (adaptive skeleton); increase maxLinesPerSnippet)",
            out, max_lines
        )
    } else {
        out
    }
}

/// Slice `content` to a node's line range without line numbers, capped at
/// `max_bytes`. Used for context code blocks.
pub fn slice_lines(content: &str, start_line: i32, end_line: i32, max_bytes: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let (start, end) = line_bounds(start_line, end_line, lines.len());
    let block = lines.get(start..end).unwrap_or(&[]).join("\n");
    if block.len() > max_bytes {
        truncate_on_char_boundary(&block, max_bytes).to_string()
    } else {
        block
    }
}

/// Convert a graph line range (1-based, inclusive, `i32`) into a half-open
/// slice range clamped to `line_count`.
///
/// Line numbers are stored as `i32`, so a corrupt or sentinel negative value is
/// representable. Casting one straight to `usize` would wrap to an enormous
/// index, so clamp at zero first and keep `start <= end`.
fn line_bounds(start_line: i32, end_line: i32, line_count: usize) -> (usize, usize) {
    let start = start_line.max(1) as usize - 1;
    let end = end_line.max(0) as usize;
    let end = end.min(line_count);
    (start.min(end), end)
}

/// Byte slicing would panic mid-codepoint on non-ASCII source, so step back to
/// the nearest boundary.
fn truncate_on_char_boundary(s: &str, max: usize) -> &str {
    if max >= s.len() {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Prefix a stale snippet so the agent can never mistake it for current source.
pub fn stale_note(file_path: &str) -> String {
    format!("({STALE_MARKER}: {file_path} changed since indexing; run ax_sync to refresh)")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lookup(indexed: &str, stored: Option<(&str, &str)>, size: i64) -> SourceLookup {
        SourceLookup {
            indexed_hash: indexed.to_string(),
            indexed_size: size,
            stored_hash: stored.map(|(h, _)| h.to_string()),
            stored_content: stored.map(|(_, c)| c.to_string()),
        }
    }

    #[test]
    fn matching_hash_is_fresh() {
        let r = classify(lookup("h1", Some(("h1", "fn main() {}")), 12));
        assert_eq!(r, ResolvedSource::Fresh("fn main() {}".into()));
    }

    #[test]
    fn differing_hash_is_stale_not_fresh() {
        let r = classify(lookup("h2", Some(("h1", "old text")), 8));
        assert_eq!(r, ResolvedSource::Stale("old text".into()));
    }

    /// A failed parse writes an empty `files.content_hash`. Empty must never be
    /// treated as "matches", or unparseable files would look verified.
    #[test]
    fn empty_indexed_hash_is_never_fresh() {
        let r = classify(lookup("", Some(("", "text")), 4));
        assert_eq!(r, ResolvedSource::Stale("text".into()));
    }

    #[test]
    fn missing_row_under_cap_asks_for_backfill() {
        let r = classify(lookup("h1", None, 100));
        assert_eq!(r, ResolvedSource::NotStored { over_cap: false });
        let msg = unavailable_reason(&r, "a.rs").unwrap();
        assert!(msg.contains("run ax index"), "{msg}");
    }

    #[test]
    fn missing_row_over_cap_says_so() {
        let huge = ax_db::source_store_cap_bytes() as i64 + 1;
        let r = classify(lookup("h1", None, huge));
        assert_eq!(r, ResolvedSource::NotStored { over_cap: true });
        let msg = unavailable_reason(&r, "big.rs").unwrap();
        assert!(msg.contains("cap"), "{msg}");
    }

    #[test]
    fn numbered_slice_numbers_from_start_line() {
        let content = (1..=10).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        let out = numbered_slice(&content, 3, 5, 40, 4000, '\t');
        assert_eq!(out, "3\tline3\n4\tline4\n5\tline5");
    }

    #[test]
    fn numbered_slice_reports_line_truncation() {
        let content = (1..=20).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        let out = numbered_slice(&content, 1, 20, 3, 4000, '\t');
        assert!(out.contains("1\tline1"));
        assert!(out.contains("truncated to 3 lines"));
        assert!(!out.contains("line4"), "must stop at the line cap");
    }

    #[test]
    fn numbered_slice_reports_char_truncation() {
        let content = "x".repeat(500);
        let out = numbered_slice(&content, 1, 1, 40, 50, '\t');
        assert!(out.contains("truncated to 50 chars"), "{out}");
    }

    /// Byte-slicing a multibyte character used to panic; the boundary walk must
    /// keep the output valid UTF-8.
    #[test]
    fn char_truncation_does_not_split_multibyte() {
        let content = "é".repeat(200);
        let out = numbered_slice(&content, 1, 1, 40, 51, '\t');
        assert!(out.contains("truncated to 51 chars"), "{out}");
    }

    #[test]
    fn end_line_past_eof_is_clamped() {
        let out = numbered_slice("only\nlines\n", 1, 999, 40, 4000, '\t');
        assert_eq!(out, "1\tonly\n2\tlines");
    }

    #[test]
    fn start_line_past_eof_yields_empty_not_panic() {
        let out = numbered_slice("a\nb", 50, 60, 40, 4000, '\t');
        assert_eq!(out, "");
    }

    /// Line numbers are i32; a negative must clamp, not wrap to a huge index.
    #[test]
    fn negative_line_numbers_do_not_panic() {
        assert_eq!(numbered_slice("a\nb\nc", -5, 2, 40, 4000, '\t'), "1\ta\n2\tb");
        assert_eq!(numbered_slice("a\nb\nc", -5, -1, 40, 4000, '\t'), "");
        assert_eq!(slice_lines("a\nb\nc", -3, -2, 4000), "");
    }

    #[test]
    fn inverted_range_yields_empty() {
        assert_eq!(numbered_slice("a\nb\nc", 3, 1, 40, 4000, '\t'), "");
    }

    #[test]
    fn slice_lines_has_no_line_numbers_and_caps_bytes() {
        assert_eq!(slice_lines("a\nb\nc", 1, 2, 4000), "a\nb");
        assert_eq!(slice_lines("abcdef", 1, 1, 3), "abc");
    }

    /// Properties over the whole input space rather than hand-picked cases.
    ///
    /// The space here is small enough to enumerate exhaustively, which is a
    /// stronger statement than random sampling would make and needs no
    /// generator dependency: every combination below really is checked.
    mod properties {
        use super::*;

        const SEP: char = '\t';

        /// Every state a `SourceLookup` can be in: the graph's hash (present or
        /// blank from a failed parse), the stored row (absent, matching,
        /// diverged, or hash-present-without-text), and size either side of the
        /// cap.
        fn all_lookups() -> Vec<(SourceLookup, &'static str)> {
            let cap = ax_db::source_store_cap_bytes() as i64;
            let mut out = Vec::new();
            for indexed in ["h1", ""] {
                for size in [0, 10, cap, cap + 1] {
                    for (stored, label) in [
                        (None, "no row"),
                        (Some(("h1", "text-a")), "hash matches h1"),
                        (Some(("h2", "text-b")), "hash diverged"),
                        (Some(("", "text-c")), "blank stored hash"),
                    ] {
                        out.push((
                            SourceLookup {
                                indexed_hash: indexed.to_string(),
                                indexed_size: size,
                                stored_hash: stored.map(|(h, _)| h.to_string()),
                                stored_content: stored.map(|(_, c)| c.to_string()),
                            },
                            label,
                        ));
                    }
                }
            }
            out
        }

        /// Fresh means exactly one thing: a non-blank graph hash the stored row
        /// agrees with. Anything else that still has text is Stale, so it gets
        /// labelled. This is the invariant the whole store rests on — if it can
        /// be violated, an agent can be shown old source as current.
        #[test]
        fn fresh_iff_hashes_agree_and_the_graph_hash_is_real() {
            for (lookup, label) in all_lookups() {
                let expected_fresh = lookup.stored_content.is_some()
                    && !lookup.indexed_hash.is_empty()
                    && lookup.stored_hash.as_deref() == Some(lookup.indexed_hash.as_str());
                let stored_text = lookup.stored_content.clone();
                let indexed_size = lookup.indexed_size;
                let cap = ax_db::source_store_cap_bytes() as i64;

                match classify(lookup) {
                    ResolvedSource::Fresh(text) => {
                        assert!(expected_fresh, "classified Fresh for '{label}'");
                        assert_eq!(Some(text), stored_text, "Fresh altered the stored text");
                    }
                    ResolvedSource::Stale(text) => {
                        assert!(!expected_fresh, "classified Stale for a match ('{label}')");
                        assert_eq!(Some(text), stored_text, "Stale altered the stored text");
                        // Text exists, so the caller must be able to label it.
                        assert!(stale_note("a.rs").contains(STALE_MARKER));
                    }
                    ResolvedSource::NotStored { over_cap } => {
                        assert!(stored_text.is_none(), "NotStored dropped stored text");
                        assert_eq!(over_cap, indexed_size > cap, "cap verdict for '{label}'");
                    }
                    ResolvedSource::NotIndexed => {
                        unreachable!("classify never invents NotIndexed")
                    }
                }
            }
        }

        /// Anything without usable text must carry a reason, and anything with
        /// text must not — otherwise a snippet is either silently empty or
        /// needlessly decorated.
        #[test]
        fn every_textless_outcome_explains_itself() {
            for (lookup, label) in all_lookups() {
                let resolved = classify(lookup);
                let reason = unavailable_reason(&resolved, "a.rs");
                match resolved {
                    ResolvedSource::Fresh(_) | ResolvedSource::Stale(_) => {
                        assert!(reason.is_none(), "text outcome got a reason for '{label}'")
                    }
                    _ => {
                        let msg = reason.expect("textless outcome needs a reason");
                        assert!(msg.contains(NOT_STORED_MARKER), "{msg}");
                        assert!(msg.contains("a.rs"), "reason must name the file: {msg}");
                    }
                }
            }
        }

        fn contents() -> Vec<(&'static str, &'static str)> {
            vec![
                ("empty", ""),
                ("one line no newline", "alpha"),
                ("trailing newline", "alpha\nbeta\n"),
                ("blank lines", "alpha\n\ngamma\n"),
                ("multibyte", "héllo\nwörld\nvögel"),
                ("crlf", "alpha\r\nbeta\r\ngamma"),
            ]
        }

        /// Line ranges arrive from the graph as `i32`, so the space includes
        /// negatives, zero, inverted ranges and values past EOF.
        fn ranges() -> Vec<(i32, i32)> {
            let vals = [i32::MIN, -3, -1, 0, 1, 2, 3, 6, 999, i32::MAX];
            let mut out = Vec::new();
            for s in vals {
                for e in vals {
                    out.push((s, e));
                }
            }
            out
        }

        /// A numbered snippet may only show lines that are really in the file,
        /// labelled with the number they really have. A wrong number sends an
        /// agent to the wrong place, which is worse than showing nothing.
        #[test]
        fn numbered_lines_are_real_lines_with_their_real_numbers() {
            for (name, content) in contents() {
                let file_lines: Vec<&str> = content.lines().collect();
                for (start, end) in ranges() {
                    let out = numbered_slice(content, start, end, 40, 100_000, SEP);
                    let mut expected_no = start.max(1) as usize;
                    for line in out.lines() {
                        let Some((num, text)) = line.split_once(SEP) else {
                            continue; // truncation note
                        };
                        let n: usize = num.parse().unwrap_or_else(|_| {
                            panic!("'{name}' {start}..{end}: bad line number {num:?}")
                        });
                        assert_eq!(
                            n, expected_no,
                            "'{name}' {start}..{end}: numbers must run consecutively"
                        );
                        assert!(n >= 1 && n <= file_lines.len(), "'{name}': number {n} out of range");
                        assert_eq!(
                            text, file_lines[n - 1],
                            "'{name}': line {n} does not match the file"
                        );
                        expected_no += 1;
                    }
                }
            }
        }

        /// The line cap is a cap: at most `max_lines` numbered lines, plus at
        /// most one note saying it truncated.
        #[test]
        fn the_line_cap_is_never_exceeded() {
            for (name, content) in contents() {
                for max_lines in [1usize, 2, 3] {
                    for (start, end) in ranges() {
                        let out = numbered_slice(content, start, end, max_lines, 100_000, SEP);
                        let numbered = out.lines().filter(|l| l.contains(SEP)).count();
                        assert!(
                            numbered <= max_lines,
                            "'{name}' {start}..{end}: {numbered} lines over a cap of {max_lines}"
                        );
                        let notes = out.lines().filter(|l| l.starts_with("...(truncated")).count();
                        assert!(notes <= 1, "'{name}': {notes} truncation notes");
                    }
                }
            }
        }

        /// The line numbers a range must produce, by definition: the requested
        /// span intersected with the lines the file actually has. Computed in
        /// `i64` because the range space includes `i32::MIN`/`MAX`.
        fn expected_numbers(start_line: i32, end_line: i32, line_count: usize) -> Vec<usize> {
            let lo = (start_line as i64).max(1);
            let hi = (end_line as i64).min(line_count as i64);
            if hi < lo {
                return Vec::new();
            }
            (lo..=hi).map(|n| n as usize).collect()
        }

        /// The other half of "never shows a line that isn't there": it must show
        /// the lines that *are* there.
        ///
        /// Without this, every property above is one-sided and an empty snippet
        /// passes them all — which is how a missing EOF clamp survived: the
        /// out-of-range slice returned nothing, and nothing violates a
        /// never-exceed bound. Caps are set high here so truncation cannot be
        /// mistaken for completeness.
        #[test]
        fn a_range_overlapping_the_file_shows_exactly_those_lines() {
            for (name, content) in contents() {
                let line_count = content.lines().count();
                for (start, end) in ranges() {
                    let out = numbered_slice(content, start, end, 100_000, 100_000_000, SEP);
                    let shown: Vec<usize> = out
                        .lines()
                        .filter_map(|l| l.split_once(SEP))
                        .filter_map(|(n, _)| n.parse().ok())
                        .collect();
                    assert_eq!(
                        shown,
                        expected_numbers(start, end, line_count),
                        "'{name}' {start}..{end}: snippet is not the requested range \
                         clipped to the file"
                    );
                }
            }
        }

        /// Same completeness bound for the context-block slicer.
        #[test]
        fn slice_lines_returns_every_line_in_an_overlapping_range() {
            for (name, content) in contents() {
                let file_lines: Vec<&str> = content.lines().collect();
                for (start, end) in ranges() {
                    let out = slice_lines(content, start, end, 100_000_000);
                    let expected: Vec<&str> = expected_numbers(start, end, file_lines.len())
                        .iter()
                        .map(|n| file_lines[n - 1])
                        .collect();
                    assert_eq!(
                        out,
                        expected.join("\n"),
                        "'{name}' {start}..{end}: block is not the requested range"
                    );
                }
            }
        }

        /// `slice_lines` feeds context code blocks: it must stay within the byte
        /// budget and emit only real lines, never a partial codepoint.
        #[test]
        fn slice_lines_respects_its_budget_and_the_file() {
            for (name, content) in contents() {
                let file_lines: Vec<&str> = content.lines().collect();
                for max_bytes in [0usize, 1, 4, 12, 100_000] {
                    for (start, end) in ranges() {
                        let out = slice_lines(content, start, end, max_bytes);
                        assert!(
                            out.len() <= max_bytes,
                            "'{name}' {start}..{end}: {} bytes over a budget of {max_bytes}",
                            out.len()
                        );
                        // Untruncated output must be exactly consecutive file lines.
                        if out.len() < max_bytes && !out.is_empty() {
                            for line in out.lines() {
                                assert!(
                                    file_lines.contains(&line),
                                    "'{name}': emitted a line not in the file: {line:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
