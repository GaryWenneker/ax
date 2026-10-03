//! On-demand context-cache and file-token-cache status. Lines never carry a body.

use crate::context_cache::{cache_enabled, cache_threshold, context_cache_counts};
use crate::store::open_pool;
use crate::tokenizer::{token_cache_status, TokenCacheStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheStatusSnapshot {
    pub enabled: bool,
    pub threshold: i64,
    pub live_rows: i64,
    pub stored_tokens: i64,
    pub expired_rows: i64,
    pub session_rows: i64,
    pub session_stored_tokens: i64,
    pub session_inline_tokens: i64,
    /// `Some("unavailable")` when the usage database cannot be read.
    pub context_error: Option<String>,
    pub tokens: TokenCacheStatus,
    pub session_known: bool,
}

/// Keep letters, digits, `_`, and `-`. Empty becomes `global`.
pub fn cache_group_key(session: Option<&str>) -> String {
    let mut out = String::new();
    for c in session.unwrap_or("").chars() {
        if out.len() >= 64 {
            break;
        }
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c);
        }
    }
    if out.is_empty() {
        "global".to_string()
    } else {
        out
    }
}

pub async fn load_cache_status(session: Option<&str>) -> CacheStatusSnapshot {
    let known = cache_group_key(session) != "global";
    let mut snap = CacheStatusSnapshot {
        enabled: cache_enabled(),
        threshold: cache_threshold(),
        live_rows: 0,
        stored_tokens: 0,
        expired_rows: 0,
        session_rows: 0,
        session_stored_tokens: 0,
        session_inline_tokens: 0,
        context_error: None,
        tokens: token_cache_status(),
        session_known: known,
    };
    match open_pool().await {
        Ok(pool) => {
            let now = chrono::Utc::now().timestamp();
            let session_for_counts = if known { session } else { None };
            match context_cache_counts(&pool, now, session_for_counts).await {
                Ok(counts) => {
                    snap.live_rows = counts.live_rows;
                    snap.stored_tokens = counts.stored_tokens;
                    snap.expired_rows = counts.expired_rows;
                    snap.session_rows = counts.session_rows;
                    snap.session_stored_tokens = counts.session_stored_tokens;
                    snap.session_inline_tokens = counts.session_inline_tokens;
                }
                Err(_) => snap.context_error = Some("unavailable".to_string()),
            }
        }
        Err(_) => snap.context_error = Some("unavailable".to_string()),
    }
    snap
}

pub fn format_cache_status_lines(
    session: Option<&str>,
    status: &CacheStatusSnapshot,
) -> [String; 2] {
    let group = cache_group_key(session);
    let mut context = format!(
        "cache group={group} lane=context enabled={} threshold={} rows={} stored_tokens={} expired={}",
        if status.enabled { 1 } else { 0 },
        status.threshold,
        status.live_rows,
        status.stored_tokens,
        status.expired_rows,
    );
    if status.session_known {
        context.push_str(&format!(
            " session_rows={} session_stored_tokens={} session_inline_tokens={}",
            status.session_rows, status.session_stored_tokens, status.session_inline_tokens,
        ));
    }
    if status.context_error.is_some() {
        context.push_str(" error=unavailable");
    }
    let mut token = format!(
        "cache group={group} lane=token entries={} capacity={} hits={} misses={} evictions={} tokenizer={}",
        status.tokens.entries,
        status.tokens.capacity,
        status.tokens.hits,
        status.tokens.misses,
        status.tokens.evictions,
        if status.tokens.tokenizer { 1 } else { 0 },
    );
    if !status.tokens.lock_ok {
        token.push_str(" error=lock");
    }
    [context, token]
}

pub fn format_context_store_line(
    session: Option<&str>,
    id: &str,
    original_tokens: i64,
    sent_tokens: i64,
    removed_tokens: i64,
) -> Option<String> {
    if !is_cache_id(id) {
        return None;
    }
    let group = cache_group_key(session);
    Some(format!(
        "cache group={group} lane=context event=store id={id} original_tokens={original_tokens} sent_tokens={sent_tokens} removed_tokens={removed_tokens}"
    ))
}

fn is_cache_id(id: &str) -> bool {
    let Some(hex) = id.strip_prefix("cc_") else {
        return false;
    };
    hex.len() == 16 && hex.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokenizer::TokenCacheStatus;

    fn sample(session_known: bool) -> CacheStatusSnapshot {
        CacheStatusSnapshot {
            enabled: true,
            threshold: 3000,
            live_rows: 4,
            stored_tokens: 12000,
            expired_rows: 1,
            session_rows: 2,
            session_stored_tokens: 8000,
            session_inline_tokens: 40,
            context_error: None,
            tokens: TokenCacheStatus {
                entries: 12,
                capacity: 8192,
                hits: 40,
                misses: 8,
                evictions: 0,
                tokenizer: true,
                lock_ok: true,
            },
            session_known,
        }
    }

    #[test]
    fn group_key_strips_unsafe_characters_and_caps_length() {
        assert_eq!(cache_group_key(None), "global");
        assert_eq!(cache_group_key(Some("")), "global");
        assert_eq!(cache_group_key(Some("../etc/passwd")), "etcpasswd");
        assert_eq!(cache_group_key(Some("ab cd")), "abcd");
        assert_eq!(
            cache_group_key(Some("ff3a3fc2-a82b-48d9-a03e-98e3d54c8d42")),
            "ff3a3fc2-a82b-48d9-a03e-98e3d54c8d42"
        );
        assert_eq!(cache_group_key(Some(&"a".repeat(80))).len(), 64);
    }

    #[test]
    fn status_lines_share_a_group_and_omit_bodies() {
        let secret = "SECRET_BODY_SHOULD_NOT_LEAK /tmp/secret.rs";
        let lines = format_cache_status_lines(Some("sess-1"), &sample(true));
        assert!(lines[0].starts_with("cache group=sess-1 lane=context "));
        assert!(lines[1].starts_with("cache group=sess-1 lane=token "));
        assert!(lines[0].contains("rows=4"));
        assert!(lines[0].contains("stored_tokens=12000"));
        assert!(lines[0].contains("session_rows=2"));
        assert!(lines[1].contains("hits=40"));
        assert!(lines[1].contains("capacity=8192"));
        for line in &lines {
            assert!(!line.contains('\n'));
            assert!(!line.contains(secret));
            assert!(!line.contains("body="));
        }
    }

    #[test]
    fn status_without_a_session_omits_session_counts() {
        let lines = format_cache_status_lines(None, &sample(false));
        assert!(lines[0].contains("group=global"));
        assert!(!lines[0].contains("session_rows"));
        assert!(lines[1].contains("group=global"));
    }

    #[test]
    fn database_and_lock_errors_use_fixed_tokens() {
        let mut status = sample(false);
        status.context_error = Some("sqlite: /Users/secret/usage.db".into());
        status.tokens.lock_ok = false;
        let lines = format_cache_status_lines(Some("s1"), &status);
        assert!(lines[0].contains("error=unavailable"));
        assert!(!lines[0].contains("/Users/secret"));
        assert!(lines[1].contains("error=lock"));
    }

    #[test]
    fn store_line_names_the_id_and_refuses_a_bad_id() {
        let secret = "the full cached reply";
        let line =
            format_context_store_line(Some("sess-1"), "cc_0123456789abcdef", 9000, 120, 8880)
                .unwrap();
        assert_eq!(
            line,
            "cache group=sess-1 lane=context event=store id=cc_0123456789abcdef original_tokens=9000 sent_tokens=120 removed_tokens=8880"
        );
        assert!(!line.contains(secret));
        assert!(format_context_store_line(Some("sess-1"), "cc_short", 1, 1, 0).is_none());
        assert!(
            format_context_store_line(Some("sess-1"), "../cc_0123456789abcdef", 1, 1, 0).is_none()
        );
    }
}
