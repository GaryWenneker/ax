use std::path::Path;

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::matcher::{cached_rules_and_skills, match_policy};
use crate::types::{GuardOp, GuardResult, GuardViolation, MatchInput, PolicyLevel};

pub async fn guard_operation(
    pool: &SqlitePool,
    project_root: &Path,
    path: &Path,
    op: GuardOp,
    content: Option<&[u8]>,
) -> Result<GuardResult, AxError> {
    let (rules, _skills) = cached_rules_and_skills(pool).await?;
    let mut violations = Vec::new();

    let rel = path
        .strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let rel_lc = rel.to_lowercase();

    for rule in rules.iter() {
        if PolicyLevel::parse(&rule.level) != Some(PolicyLevel::Critical) {
            continue;
        }
        let id_lc = rule.id.to_lowercase();
        let tags: Vec<String> = rule.tags.iter().map(|t| t.to_lowercase()).collect();

        if id_lc.contains("utf8") || id_lc.contains("encoding") || tags.iter().any(|t| t == "utf8") {
            if let Some(bytes) = content {
                if has_utf8_bom(bytes) {
                    violations.push(GuardViolation {
                        rule_id: rule.id.clone(),
                        message: "File encoding violates UTF-8 policy (UTF-8 BOM detected)".into(),
                    });
                } else if has_utf16_bom(bytes) || has_null_padded_ascii(bytes) {
                    violations.push(GuardViolation {
                        rule_id: rule.id.clone(),
                        message: "File encoding violates UTF-8 policy (UTF-16 BOM or null-padded ASCII detected)".into(),
                    });
                }
            }
        }

        if id_lc.contains("secret") || tags.iter().any(|t| t == "secrets") {
            if is_sensitive_path(&rel_lc)
                && matches!(op, GuardOp::Write | GuardOp::Delete)
            {
                let verb = match op {
                    GuardOp::Write => "Writing",
                    GuardOp::Delete => "Deleting",
                };
                violations.push(GuardViolation {
                    rule_id: rule.id.clone(),
                    message: format!("{verb} sensitive path blocked by rule {}", rule.id),
                });
            }
        }
    }

    Ok(GuardResult {
        allowed: violations.is_empty(),
        violations,
    })
}

pub async fn guard_with_context(
    pool: &SqlitePool,
    input: &MatchInput,
    path: &Path,
    op: GuardOp,
    content: Option<&[u8]>,
) -> Result<GuardResult, AxError> {
    let _ = match_policy(pool, input).await?;
    guard_operation(pool, &input.cwd, path, op, content).await
}

fn is_sensitive_path(rel_lc: &str) -> bool {
    rel_lc.ends_with(".env")
        || rel_lc.contains("credentials")
        || rel_lc.ends_with(".pem")
        || rel_lc.ends_with(".key")
}

fn has_utf8_bom(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xEF, 0xBB, 0xBF])
}

fn has_utf16_bom(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF])
}

fn has_null_padded_ascii(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let sample = bytes.len().min(64);
    let mut nulls = 0usize;
    for chunk in bytes[..sample].chunks(2) {
        if chunk.len() == 2 && chunk[0].is_ascii() && chunk[1] == 0 {
            nulls += 1;
        }
    }
    nulls > sample / 8
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::TempDir;

    async fn pool_with_utf8_rule() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = SqlitePoolOptions::new()
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true),
            )
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE policy_rules (
                id TEXT PRIMARY KEY, level TEXT, always_apply INTEGER, globs TEXT, triggers TEXT,
                tags TEXT, priority INTEGER, body TEXT, source_path TEXT, content_hash TEXT, updated_at INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE policy_skills (
                name TEXT PRIMARY KEY, description TEXT, triggers TEXT, tags TEXT,
                priority INTEGER, context_task TEXT, body TEXT, source_path TEXT,
                content_hash TEXT, updated_at INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO policy_rules VALUES ('utf8-no-bom','CRITICAL',1,'[]','[]','[\"utf8\"]',100,'','', '', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        (dir, pool)
    }

    #[tokio::test]
    async fn blocks_sensitive_delete() {
        let (dir, pool) = pool_with_utf8_rule().await;
        sqlx::query(
            "INSERT INTO policy_rules VALUES ('secrets','CRITICAL',1,'[]','[]','[\"secrets\"]',100,'','', '', 0)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let root = dir.path();
        let target = root.join(".env");
        let result = guard_operation(&pool, root, &target, GuardOp::Delete, None)
            .await
            .unwrap();
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn blocks_utf8_bom_in_proposed_content() {
        let (dir, pool) = pool_with_utf8_rule().await;
        let root = dir.path();
        let target = root.join("new.rs");
        let bytes = [0xEF, 0xBB, 0xBF, b'x'];
        let result = guard_operation(&pool, root, &target, GuardOp::Write, Some(&bytes))
            .await
            .unwrap();
        assert!(!result.allowed);
    }
}
