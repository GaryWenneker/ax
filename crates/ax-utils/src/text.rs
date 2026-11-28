//! Text helpers that never split a UTF-8 character.
//!
//! Slicing a `&str` at a byte offset panics when the offset falls inside a
//! multi-byte character. Code that cuts user, source or remote text to a byte
//! budget uses these helpers instead of `&s[..n]`.

/// The largest index `<= index` that is a character boundary of `s`
/// (clamped to `s.len()`).
pub fn floor_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// The smallest index `>= index` that is a character boundary of `s`
/// (clamped to `s.len()`).
pub fn ceil_char_boundary(s: &str, index: usize) -> usize {
    if index >= s.len() {
        return s.len();
    }
    let mut i = index;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// The longest prefix of `s` that is at most `max_bytes` long.
pub fn prefix_bytes(s: &str, max_bytes: usize) -> &str {
    &s[..floor_char_boundary(s, max_bytes)]
}

/// The longest suffix of `s` that is at most `max_bytes` long.
pub fn suffix_bytes(s: &str, max_bytes: usize) -> &str {
    let start = s.len().saturating_sub(max_bytes);
    &s[ceil_char_boundary(s, start)..]
}

/// `s[start..end]` with both ends clamped to the string and moved inward to
/// character boundaries. Never panics; an empty or inverted range gives "".
pub fn slice_bytes(s: &str, start: usize, end: usize) -> &str {
    let start = ceil_char_boundary(s, start);
    let end = floor_char_boundary(s, end);
    if start >= end {
        ""
    } else {
        &s[start..end]
    }
}

/// Cuts `s` to at most `max_chars` characters and appends `marker` when it
/// was cut. The marker is not counted.
pub fn truncate_chars(s: &str, max_chars: usize, marker: &str) -> String {
    match s.char_indices().nth(max_chars) {
        Some((cut, _)) => format!("{}{marker}", &s[..cut]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIXED: &str = "aé€😀z"; // 1 + 2 + 3 + 4 + 1 bytes

    #[test]
    fn floor_and_ceil_never_land_inside_a_character() {
        for i in 0..=MIXED.len() + 2 {
            assert!(MIXED.is_char_boundary(floor_char_boundary(MIXED, i)), "{i}");
            assert!(MIXED.is_char_boundary(ceil_char_boundary(MIXED, i)), "{i}");
            assert!(floor_char_boundary(MIXED, i) <= i.min(MIXED.len()));
            assert!(ceil_char_boundary(MIXED, i) >= i.min(MIXED.len()));
        }
    }

    #[test]
    fn prefix_and_suffix_respect_the_budget_and_boundaries() {
        assert_eq!(prefix_bytes(MIXED, 0), "");
        assert_eq!(prefix_bytes(MIXED, 2), "a");
        assert_eq!(prefix_bytes(MIXED, 3), "aé");
        assert_eq!(prefix_bytes(MIXED, 5), "aé");
        assert_eq!(prefix_bytes(MIXED, 100), MIXED);
        assert_eq!(suffix_bytes(MIXED, 1), "z");
        assert_eq!(suffix_bytes(MIXED, 4), "z");
        assert_eq!(suffix_bytes(MIXED, 5), "😀z");
        assert_eq!(suffix_bytes(MIXED, 100), MIXED);
    }

    #[test]
    fn slice_bytes_clamps_and_never_panics() {
        assert_eq!(slice_bytes(MIXED, 1, 3), "é");
        assert_eq!(slice_bytes(MIXED, 2, 3), ""); // starts inside é
        assert_eq!(slice_bytes(MIXED, 0, usize::MAX), MIXED);
        assert_eq!(slice_bytes(MIXED, 50, 60), "");
        assert_eq!(slice_bytes(MIXED, 5, 2), "");
        for a in 0..MIXED.len() + 2 {
            for b in 0..MIXED.len() + 2 {
                let _ = slice_bytes(MIXED, a, b);
            }
        }
    }

    #[test]
    fn truncate_chars_counts_characters_and_marks_cuts() {
        assert_eq!(truncate_chars(MIXED, 3, "…"), "aé€…");
        assert_eq!(truncate_chars(MIXED, 5, "…"), MIXED);
        assert_eq!(truncate_chars("", 0, "…"), "");
        assert_eq!(truncate_chars("ab", 0, "…"), "…");
    }
}
