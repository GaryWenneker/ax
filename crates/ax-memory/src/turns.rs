//! Per-turn memories written by the agent turn hooks: local, recall-only, kept 30 days.

use sqlx::SqlitePool;

use ax_utils::errors::{AxError, DatabaseError};

pub const TURN_KIND: &str = "turn";
pub const TURN_SOURCE: &str = "turn-hook";
pub const TURN_RETENTION_DAYS: i64 = 30;

/// One agent turn that changed files or made a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRecord {
    pub id: String,
    pub title: String,
    pub body: String,
    pub files: Vec<String>,
}

fn db_err(e: impl std::fmt::Display) -> AxError {
    AxError::Database(DatabaseError::new(e.to_string()))
}

/// Insert `record` as a `turn` memory. Returns `false` when a memory with that id exists.
pub async fn save_turn(
    pool: &SqlitePool,
    record: &TurnRecord,
    now_ms: i64,
) -> Result<bool, AxError> {
    let embedding = crate::embed::embedding_to_blob(&crate::embed::embed_text(&format!(
        "{} {}",
        record.title, record.body
    )));
    let files = serde_json::to_string(&record.files).map_err(|e| AxError::Other(e.to_string()))?;
    let result = sqlx::query(
        r#"INSERT OR IGNORE INTO memories (id, kind, title, body, tags, files, confidence, source, created_at, updated_at, embedding, enabled)
           VALUES (?, ?, ?, ?, '[]', ?, 1.0, ?, ?, ?, ?, 1)"#,
    )
    .bind(&record.id)
    .bind(TURN_KIND)
    .bind(&record.title)
    .bind(&record.body)
    .bind(files)
    .bind(TURN_SOURCE)
    .bind(now_ms)
    .bind(now_ms)
    .bind(embedding)
    .execute(pool)
    .await
    .map_err(db_err)?;
    Ok(result.rows_affected() > 0)
}

/// Delete `turn` memories created more than `max_age_days` before `now_ms`. Other kinds are kept.
pub async fn prune_turns(
    pool: &SqlitePool,
    now_ms: i64,
    max_age_days: i64,
) -> Result<u64, AxError> {
    let cutoff = now_ms - max_age_days * 86_400_000;
    let result = sqlx::query("DELETE FROM memories WHERE kind = ? AND created_at < ?")
        .bind(TURN_KIND)
        .bind(cutoff)
        .execute(pool)
        .await
        .map_err(db_err)?;
    Ok(result.rows_affected())
}

const REDACTED: &str = "[redacted]";
const PASSWORD_KEYS: [&str; 3] = ["password", "passwd", "pwd"];
const TOKEN_PREFIXES: [&str; 6] = ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"];

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '/' | '=' | '.')
}

fn looks_like_secret(token: &str) -> bool {
    let len = token.len();
    if token.starts_with("sk-") && len >= 20 {
        return true;
    }
    if TOKEN_PREFIXES.iter().any(|p| token.starts_with(p)) && len >= 24 {
        return true;
    }
    if token.starts_with("AKIA")
        && len == 20
        && token
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    {
        return true;
    }
    if len < 40 {
        return false;
    }
    if token.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    token
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '='))
        && token.chars().any(|c| c.is_ascii_digit())
        && token.chars().any(|c| c.is_ascii_uppercase())
        && token.chars().any(|c| c.is_ascii_lowercase())
}

/// `password=value` → `password=[redacted]`; `None` when `token` is not such an assignment.
fn redact_assignment(token: &str) -> Option<String> {
    let (key, value) = token.split_once('=')?;
    (PASSWORD_KEYS.contains(&key.to_ascii_lowercase().as_str()) && !value.is_empty())
        .then(|| format!("{key}={REDACTED}"))
}

/// Replace likely secrets (API keys, tokens, `password=` values, long hex/base64 runs) with `[redacted]`.
pub fn redact_secrets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_token = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(is_token_char) {
        let separator = &rest[..start];
        out.push_str(separator);
        let after_password_colon = separator.trim() == ":"
            && PASSWORD_KEYS.contains(&previous_token.to_ascii_lowercase().as_str());
        let tail = &rest[start..];
        let end = tail.find(|c: char| !is_token_char(c)).unwrap_or(tail.len());
        let run = &tail[..end];
        let token = run.trim_end_matches('.');
        let dots = &run[token.len()..];
        if after_password_colon && !token.is_empty() {
            out.push_str(REDACTED);
        } else if let Some(redacted) = redact_assignment(token) {
            out.push_str(&redacted);
        } else if looks_like_secret(token) {
            out.push_str(REDACTED);
        } else {
            out.push_str(token);
        }
        out.push_str(dots);
        previous_token = token.to_string();
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}
