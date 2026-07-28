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

        // Generic static gate: ANY CRITICAL rule can opt in by writing one of
        // these directives as a plain line in its body — no code change needed
        // per rule, unlike the two hardcoded checks above.
        //   guard: forbid-path: "**/*.pem"
        //   guard: forbid-content: "eval("            (or /regex/ form)
        //   guard: require-content: "requireAuth("    (scoped by the rule's `globs`)
        for directive in parse_guard_directives(&rule.body) {
            match directive {
                GuardDirective::ForbidPath(glob) => {
                    if path_matches_glob(&glob, &rel) {
                        violations.push(GuardViolation {
                            rule_id: rule.id.clone(),
                            message: format!(
                                "Path matches forbidden pattern '{glob}' (rule {})",
                                rule.id
                            ),
                        });
                    }
                }
                GuardDirective::ForbidContent(matcher) => {
                    if op == GuardOp::Write {
                        if let Some(text) = content.and_then(|b| std::str::from_utf8(b).ok()) {
                            if matcher.is_match(text) {
                                violations.push(GuardViolation {
                                    rule_id: rule.id.clone(),
                                    message: format!(
                                        "Content matches pattern forbidden by rule {}",
                                        rule.id
                                    ),
                                });
                            }
                        }
                    }
                }
                GuardDirective::RequireContent(matcher) => {
                    if op == GuardOp::Write
                        && !rule.globs.is_empty()
                        && any_glob_matches(&rule.globs, &rel)
                    {
                        if let Some(text) = content.and_then(|b| std::str::from_utf8(b).ok()) {
                            if !matcher.is_match(text) {
                                violations.push(GuardViolation {
                                    rule_id: rule.id.clone(),
                                    message: format!(
                                        "Missing content required by rule {} for this path",
                                        rule.id
                                    ),
                                });
                            }
                        }
                    }
                }
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

/// A single value a directive is checked against — either a plain substring
/// or, when the quoted value is wrapped in `/…/`, a regex.
#[derive(Debug, Clone)]
enum GuardMatcher {
    Substring(String),
    Regex(regex::Regex),
}

impl GuardMatcher {
    fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        if raw.len() >= 2 && raw.starts_with('/') && raw.ends_with('/') {
            regex::Regex::new(&raw[1..raw.len() - 1])
                .ok()
                .map(GuardMatcher::Regex)
        } else if !raw.is_empty() {
            Some(GuardMatcher::Substring(raw.to_string()))
        } else {
            None
        }
    }

    fn is_match(&self, haystack: &str) -> bool {
        match self {
            GuardMatcher::Substring(s) => haystack.contains(s.as_str()),
            GuardMatcher::Regex(re) => re.is_match(haystack),
        }
    }
}

/// A declarative, per-rule-body guard directive — lets any CRITICAL rule
/// participate in the static gate without a hardcoded per-rule-id checker.
#[derive(Debug, Clone)]
enum GuardDirective {
    ForbidPath(String),
    ForbidContent(GuardMatcher),
    RequireContent(GuardMatcher),
}

const FORBID_PATH_PREFIX: &str = "forbid-path:";
const FORBID_CONTENT_PREFIX: &str = "forbid-content:";
const REQUIRE_CONTENT_PREFIX: &str = "require-content:";

/// Scan a rule body for `guard: <keyword>: "<value>"` lines. Matching is
/// intentionally narrow (exact `guard:` prefix + a known keyword) so ordinary
/// prose mentioning the word "guard" cannot accidentally opt a rule in.
fn parse_guard_directives(body: &str) -> Vec<GuardDirective> {
    let mut out = Vec::new();
    for raw_line in body.lines() {
        let line = raw_line.trim().trim_start_matches(['-', '*']).trim();
        let lower = line.to_ascii_lowercase();
        let Some(after_guard) = lower.strip_prefix("guard:") else {
            continue;
        };
        let after_guard = after_guard.trim();
        let value_offset = line.len() - after_guard.len();
        let rest = &line[value_offset..];

        let (prefix_len, build): (usize, fn(String) -> Option<GuardDirective>) =
            if after_guard.starts_with(FORBID_PATH_PREFIX) {
                (FORBID_PATH_PREFIX.len(), |v| Some(GuardDirective::ForbidPath(v)))
            } else if after_guard.starts_with(FORBID_CONTENT_PREFIX) {
                (FORBID_CONTENT_PREFIX.len(), |v| {
                    GuardMatcher::parse(&v).map(GuardDirective::ForbidContent)
                })
            } else if after_guard.starts_with(REQUIRE_CONTENT_PREFIX) {
                (REQUIRE_CONTENT_PREFIX.len(), |v| {
                    GuardMatcher::parse(&v).map(GuardDirective::RequireContent)
                })
            } else {
                continue;
            };

        if let Some(value) = extract_quoted(&rest[prefix_len..]) {
            if let Some(directive) = build(value) {
                out.push(directive);
            }
        }
    }
    out
}

/// Pull the contents out of a `"…"` or `'…'` wrapped value.
fn extract_quoted(s: &str) -> Option<String> {
    let s = s.trim();
    let mut chars = s.chars();
    let quote = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let rest = &s[quote.len_utf8()..];
    let end = rest.find(quote)?;
    Some(rest[..end].to_string())
}

fn path_matches_glob(pattern: &str, rel_path: &str) -> bool {
    globset::Glob::new(pattern)
        .map(|g| g.compile_matcher().is_match(rel_path))
        .unwrap_or(false)
}

fn any_glob_matches(patterns: &[String], rel_path: &str) -> bool {
    patterns.iter().any(|p| path_matches_glob(p, rel_path))
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
                tags TEXT, priority INTEGER, body TEXT, source_path TEXT, content_hash TEXT, updated_at INTEGER,
                enabled INTEGER NOT NULL DEFAULT 1, status TEXT NOT NULL DEFAULT 'approved',
                scope TEXT NOT NULL DEFAULT 'project'
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE policy_skills (
                name TEXT PRIMARY KEY, description TEXT, triggers TEXT, tags TEXT,
                priority INTEGER, context_task TEXT, body TEXT, source_path TEXT,
                content_hash TEXT, updated_at INTEGER,
                enabled INTEGER NOT NULL DEFAULT 1, status TEXT NOT NULL DEFAULT 'approved',
                scope TEXT NOT NULL DEFAULT 'project'
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO policy_rules (id, level, always_apply, globs, triggers, tags, priority, body, source_path, content_hash, updated_at, enabled, status, scope)
             VALUES ('utf8-no-bom','CRITICAL',1,'[]','[]','[\"utf8\"]',100,'','','',0,1,'approved','project')",
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
            "INSERT INTO policy_rules (id, level, always_apply, globs, triggers, tags, priority, body, source_path, content_hash, updated_at, enabled, status, scope)
             VALUES ('secrets','CRITICAL',1,'[]','[]','[\"secrets\"]',100,'','','',0,1,'approved','project')",
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

    async fn insert_rule(pool: &SqlitePool, id: &str, globs: &str, body: &str) {
        sqlx::query(
            "INSERT INTO policy_rules (id, level, always_apply, globs, triggers, tags, priority, body, source_path, content_hash, updated_at, enabled, status, scope)
             VALUES (?, 'CRITICAL', 1, ?, '[]', '[]', 100, ?, '', '', 0, 1, 'approved', 'project')",
        )
        .bind(id)
        .bind(globs)
        .bind(body)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn generic_gate_ignores_rules_without_directives() {
        // A plain CRITICAL rule with no guard: directive must never block —
        // the generic gate is opt-in, not a blanket "CRITICAL = blocked" rule.
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(&pool, "no-op-rule", "[]", "Just a reminder, nothing enforceable here.").await;
        let root = dir.path();
        let target = root.join("anything.rs");
        let result = guard_operation(&pool, root, &target, GuardOp::Write, Some(b"fn main() {}"))
            .await
            .unwrap();
        assert!(result.allowed);
    }

    #[tokio::test]
    async fn forbid_path_directive_blocks_matching_glob() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(&pool, "no-pem", "[]", "Never commit key material.\nguard: forbid-path: \"**/*.pem\"\n").await;
        let root = dir.path();
        let target = root.join("certs").join("server.pem");
        let result = guard_operation(&pool, root, &target, GuardOp::Write, None)
            .await
            .unwrap();
        assert!(!result.allowed);
        assert_eq!(result.violations[0].rule_id, "no-pem");
    }

    #[tokio::test]
    async fn forbid_content_directive_blocks_matching_write() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(&pool, "no-eval", "[]", "guard: forbid-content: \"eval(\"").await;
        let root = dir.path();
        let target = root.join("app.js");
        let bad = guard_operation(&pool, root, &target, GuardOp::Write, Some(b"eval(userInput)"))
            .await
            .unwrap();
        assert!(!bad.allowed);
        let good = guard_operation(&pool, root, &target, GuardOp::Write, Some(b"JSON.parse(userInput)"))
            .await
            .unwrap();
        assert!(good.allowed);
    }

    #[tokio::test]
    async fn forbid_content_directive_supports_regex() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(&pool, "no-secret-key", "[]", "guard: forbid-content: \"/sk-[A-Za-z0-9]{10,}/\"").await;
        let root = dir.path();
        let target = root.join("config.ts");
        let bad = guard_operation(&pool, root, &target, GuardOp::Write, Some(b"const key = 'sk-abcdefghijklmnop';"))
            .await
            .unwrap();
        assert!(!bad.allowed);
    }

    #[tokio::test]
    async fn require_content_directive_scoped_by_rule_globs() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(
            &pool,
            "routes-need-auth",
            "[\"src/api/**\"]",
            "guard: require-content: \"requireAuth(\"",
        )
        .await;
        let root = dir.path();

        let route = root.join("src").join("api").join("users.ts");
        let missing = guard_operation(&pool, root, &route, GuardOp::Write, Some(b"export function handler() {}"))
            .await
            .unwrap();
        assert!(!missing.allowed);

        let present = guard_operation(&pool, root, &route, GuardOp::Write, Some(b"requireAuth(); export function handler() {}"))
            .await
            .unwrap();
        assert!(present.allowed);

        // Out-of-scope path (doesn't match the rule's globs) must not be blocked.
        let other = root.join("src").join("lib.rs");
        let out_of_scope = guard_operation(&pool, root, &other, GuardOp::Write, Some(b"fn main() {}"))
            .await
            .unwrap();
        assert!(out_of_scope.allowed);
    }
}
