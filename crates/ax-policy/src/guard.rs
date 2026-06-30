use std::path::Path;

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::index::list_rules;
use crate::matcher::match_policy;
use crate::types::{GuardOp, GuardResult, GuardViolation, MatchInput, PolicyLevel};

pub async fn guard_operation(
    pool: &SqlitePool,
    project_root: &Path,
    path: &Path,
    op: GuardOp,
    content: Option<&[u8]>,
) -> Result<GuardResult, AxError> {
    let rules = list_rules(pool).await?;
    let mut violations = Vec::new();

    let rel = path
        .strip_prefix(project_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let rel_lc = rel.to_lowercase();

    for rule in &rules {
        if PolicyLevel::parse(&rule.level) != Some(PolicyLevel::Critical) {
            continue;
        }
        let id_lc = rule.id.to_lowercase();
        let tags: Vec<String> = rule.tags.iter().map(|t| t.to_lowercase()).collect();

        if id_lc.contains("utf8") || id_lc.contains("encoding") || tags.iter().any(|t| t == "utf8") {
            if let Some(bytes) = content {
                if has_utf16_bom(bytes) || has_null_padded_ascii(bytes) {
                    violations.push(GuardViolation {
                        rule_id: rule.id.clone(),
                        message: "File encoding violates UTF-8 policy (UTF-16 BOM or null-padded ASCII detected)".into(),
                    });
                }
            }
        }

        if id_lc.contains("secret") || tags.iter().any(|t| t == "secrets") {
            if op == GuardOp::Write
                && (rel_lc.ends_with(".env")
                    || rel_lc.contains("credentials")
                    || rel_lc.ends_with(".pem")
                    || rel_lc.ends_with(".key"))
            {
                violations.push(GuardViolation {
                    rule_id: rule.id.clone(),
                    message: format!("Writing sensitive path blocked by rule {}", rule.id),
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
