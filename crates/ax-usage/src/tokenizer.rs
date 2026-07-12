//! Exact BPE token counting (o200k_base) with a per-file cache.
//!
//! Falls back to a chars/4 heuristic only when the tokenizer fails to
//! initialize, so savings numbers are measured rather than estimated.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use tiktoken_rs::CoreBPE;

static BPE: OnceLock<Option<CoreBPE>> = OnceLock::new();

/// Cap the file-token cache so a long-running daemon cannot grow unbounded.
const FILE_CACHE_MAX_ENTRIES: usize = 8192;

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
    fn missing_file_returns_none() {
        assert!(count_file_tokens(Path::new("Z:/definitely/not/here.rs")).is_none());
    }
}
