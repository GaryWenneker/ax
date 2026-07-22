//! Exact BPE token counting (o200k_base) with a per-file cache.
//!
//! Falls back to a chars/4 heuristic only when the tokenizer fails to
//! initialize, so savings numbers are measured rather than estimated.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use serde::Serialize;
use tiktoken_rs::CoreBPE;

static BPE: OnceLock<Option<CoreBPE>> = OnceLock::new();

/// Cap the file-token cache so a long-running daemon cannot grow unbounded.
const FILE_CACHE_MAX_ENTRIES: usize = 8192;

/// Max input bytes accepted by [`tokenize_text`] (UI / API protection).
pub const TOKENIZE_MAX_INPUT_BYTES: usize = 32 * 1024;
/// Max token chips returned by [`tokenize_text`].
pub const TOKENIZE_MAX_TOKENS: usize = 4096;

/// Result of splitting text into o200k BPE token strings for visualization.
#[derive(Debug, Clone, Serialize)]
pub struct TokenizeResult {
    pub tokens: Vec<String>,
    /// Full token count of the (possibly input-capped) text before chip truncation.
    pub count: usize,
    pub chars: usize,
    /// True when input or chip list was truncated to stay within caps.
    pub truncated: bool,
}

#[derive(Clone, Copy)]
struct CachedFileTokens {
    mtime_ms: u64,
    size: u64,
    tokens: i64,
}

static FILE_CACHE: OnceLock<Mutex<HashMap<PathBuf, CachedFileTokens>>> = OnceLock::new();

fn bpe() -> Option<&'static CoreBPE> {
    BPE.get_or_init(|| tiktoken_rs::o200k_base().ok()).as_ref()
}

/// Whether exact BPE counting is available (o200k_base loaded).
pub fn tokenizer_available() -> bool {
    bpe().is_some()
}

/// Count tokens in `text`. Exact (o200k_base BPE) when available,
/// otherwise a conservative chars/4 heuristic.
pub fn count_tokens(text: &str) -> usize {
    match bpe() {
        Some(enc) => enc.encode_ordinary(text).len(),
        None => text.len() / 4,
    }
}

/// Truncate `text` to at most `max_bytes` on a UTF-8 char boundary.
pub fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// Split `text` into o200k token strings for chip visualization.
///
/// Caps input at [`TOKENIZE_MAX_INPUT_BYTES`] and returned chips at
/// [`TOKENIZE_MAX_TOKENS`]. When the BPE encoder is unavailable, falls back to
/// ~4-char chunks so the UI still has something to render.
pub fn tokenize_text(text: &str) -> TokenizeResult {
    let input_truncated = text.len() > TOKENIZE_MAX_INPUT_BYTES;
    let slice = if input_truncated {
        truncate_utf8(text, TOKENIZE_MAX_INPUT_BYTES)
    } else {
        text.to_string()
    };
    let chars = slice.chars().count();

    match bpe() {
        Some(enc) => {
            let ids = enc.encode_ordinary(&slice);
            let count = ids.len();
            let chip_truncated = count > TOKENIZE_MAX_TOKENS;
            let take = count.min(TOKENIZE_MAX_TOKENS);
            let tokens: Vec<String> = ids[..take]
                .iter()
                .map(|id| match enc.decode_bytes(&[*id]) {
                    Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                    Err(_) => format!("<{id}>"),
                })
                .collect();
            TokenizeResult {
                tokens,
                count,
                chars,
                truncated: input_truncated || chip_truncated,
            }
        }
        None => {
            let chunks: Vec<String> = slice
                .chars()
                .collect::<Vec<_>>()
                .chunks(4)
                .map(|c| c.iter().collect::<String>())
                .collect();
            let count = chunks.len();
            let chip_truncated = count > TOKENIZE_MAX_TOKENS;
            let tokens = chunks.into_iter().take(TOKENIZE_MAX_TOKENS).collect();
            TokenizeResult {
                tokens,
                count,
                chars,
                truncated: input_truncated || chip_truncated,
            }
        }
    }
}

fn file_cache() -> &'static Mutex<HashMap<PathBuf, CachedFileTokens>> {
    FILE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn mtime_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Exact BPE token count for an inclusive 1-indexed line range in a file.
/// Returns `None` when the file cannot be read or the tokenizer is unavailable.
pub fn count_file_line_range_tokens(path: &Path, start_line: u32, end_line: u32) -> Option<i64> {
    if bpe().is_none() || start_line == 0 || end_line < start_line {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let lines: Vec<&str> = text.lines().collect();
    let start_idx = (start_line - 1) as usize;
    let end_idx = (end_line as usize).min(lines.len());
    if start_idx >= end_idx {
        return Some(0);
    }
    let slice = lines[start_idx..end_idx].join("\n");
    if slice.is_empty() {
        return Some(0);
    }
    Some(count_tokens(&slice) as i64)
}

/// Exact token count of a file's current contents, cached by (mtime, size).
/// Returns `None` when the file cannot be read or the tokenizer is unavailable.
pub fn count_file_tokens(path: &Path) -> Option<i64> {
    if bpe().is_none() {
        return None;
    }
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let key_mtime = mtime_ms(&meta);
    let key_size = meta.len();

    if let Ok(cache) = file_cache().lock() {
        if let Some(hit) = cache.get(path) {
            if hit.mtime_ms == key_mtime && hit.size == key_size {
                return Some(hit.tokens);
            }
        }
    }

    let bytes = std::fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let tokens = count_tokens(&text) as i64;

    if let Ok(mut cache) = file_cache().lock() {
        if cache.len() >= FILE_CACHE_MAX_ENTRIES {
            cache.clear();
        }
        cache.insert(
            path.to_path_buf(),
            CachedFileTokens {
                mtime_ms: key_mtime,
                size: key_size,
                tokens,
            },
        );
    }
    Some(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_tokens_exactly() {
        assert!(tokenizer_available());
        let n = count_tokens("fn main() { println!(\"hello world\"); }");
        assert!(n > 5 && n < 30, "unexpected token count: {n}");
    }

    #[test]
    fn empty_text_is_zero_tokens() {
        assert_eq!(count_tokens(""), 0);
    }

    #[test]
    fn file_tokens_cached_by_mtime_and_size() {
        let dir = std::env::temp_dir().join("ax-usage-tokenizer-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.rs");
        std::fs::write(&path, "fn a() {}\nfn b() {}\n").unwrap();

        let first = count_file_tokens(&path).unwrap();
        let second = count_file_tokens(&path).unwrap();
        assert_eq!(first, second);
        assert!(first > 0);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn line_range_tokens_subset_of_full_file() {
        let dir = std::env::temp_dir().join("ax-usage-tokenizer-range-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("range.rs");
        std::fs::write(
            &path,
            "line1\nline2\nline3\nline4\nline5\nline6\nline7\n",
        )
        .unwrap();

        let full = count_file_tokens(&path).unwrap();
        let slice = count_file_line_range_tokens(&path, 2, 4).unwrap();
        assert!(slice > 0);
        assert!(slice < full);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_file_returns_none() {
        assert!(count_file_tokens(Path::new("Z:/definitely/not/here.rs")).is_none());
    }

    #[test]
    fn tokenize_text_roundtrips_visible_tokens() {
        assert!(tokenizer_available());
        let r = tokenize_text("hello world");
        assert!(r.count >= 2);
        assert!(!r.tokens.is_empty());
        assert!(!r.truncated);
        let joined: String = r.tokens.concat();
        assert!(joined.contains("hello") || joined.contains("hell"));
    }

    #[test]
    fn tokenize_text_respects_token_cap() {
        let big = "word ".repeat(TOKENIZE_MAX_TOKENS + 200);
        let r = tokenize_text(&big);
        assert!(r.truncated);
        assert!(r.tokens.len() <= TOKENIZE_MAX_TOKENS);
        assert!(r.count >= r.tokens.len());
    }

    #[test]
    fn truncate_utf8_respects_char_boundary() {
        let s = "café🚀";
        let t = truncate_utf8(s, 5);
        assert!(t.len() <= 5);
        assert!(s.starts_with(&t));
    }
}
