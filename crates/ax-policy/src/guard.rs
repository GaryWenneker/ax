use std::path::{Component, Path, PathBuf};

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
    guard_operation_with_extra_skills(pool, project_root, path, op, content, Vec::new()).await
}

pub async fn guard_operation_with_extra_skills(
    pool: &SqlitePool,
    project_root: &Path,
    path: &Path,
    op: GuardOp,
    content: Option<&[u8]>,
    extra_skills: Vec<crate::types::PolicySkillRow>,
) -> Result<GuardResult, AxError> {
    let (rules, skills) = cached_rules_and_skills(pool).await?;
    let skills = if extra_skills.is_empty() {
        skills
    } else {
        std::sync::Arc::new(crate::matcher::merge_skills(
            (*skills).clone(),
            extra_skills,
        ))
    };
    let mut violations = Vec::new();
    let mut warnings = Vec::new();

    let rel = match project_relative(project_root, path) {
        Some(rel) => rel,
        None => {
            return Ok(GuardResult {
                allowed: false,
                violations: vec![GuardViolation {
                    rule_id: OUTSIDE_PROJECT_RULE.into(),
                    message: format!(
                        "{} is outside the project {}; ax_guard only allows paths inside it",
                        path.display(),
                        project_root.display()
                    ),
                }],
                warnings: Vec::new(),
            });
        }
    };
    let rel_lc = rel.to_lowercase();
    let require_skill_exempt = is_require_skill_exempt(&rel);

    for rule in rules.iter() {
        if PolicyLevel::parse(&rule.level) != Some(PolicyLevel::Critical) {
            continue;
        }
        let id_lc = rule.id.to_lowercase();
        let tags: Vec<String> = rule.tags.iter().map(|t| t.to_lowercase()).collect();

        if id_lc.contains("utf8") || id_lc.contains("encoding") || tags.iter().any(|t| t == "utf8")
        {
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

        if (id_lc.contains("secret") || tags.iter().any(|t| t == "secrets"))
            && is_sensitive_path(&rel_lc)
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

        // Generic static gate: ANY CRITICAL rule can opt in by writing one of
        // these directives as a plain line in its body — no code change needed
        // per rule, unlike the two hardcoded checks above.
        //   guard: forbid-path: "**/*.pem"
        //   guard: forbid-content: "eval("            (or /regex/ form; scoped by
        //                                             the rule's `globs` if it has any)
        //   guard: require-content: "requireAuth("    (scoped by the rule's `globs`)
        //   guard: require-skill: "old-coder"         (skill must exist, be approved, alwaysApply)
        //
        // A directive or rule glob that does not parse blocks the write
        // (outside policy files, which stay writable so the rule can be fixed):
        // a CRITICAL rule must never be skipped without anyone noticing.
        let (directives, mut problems) = parse_guard_directives(&rule.body);
        if directives.iter().any(|d| {
            matches!(
                d,
                GuardDirective::ForbidContent(_) | GuardDirective::RequireContent(_)
            )
        }) {
            problems.extend(rule.globs.iter().filter_map(|g| {
                build_glob(g)
                    .err()
                    .map(|e| format!("rule glob '{g}' is invalid: {e}"))
            }));
        }
        for problem in problems {
            let message = format!("CRITICAL rule {} has an invalid guard: {problem}", rule.id);
            if require_skill_exempt {
                warnings.push(message);
            } else {
                violations.push(GuardViolation {
                    rule_id: rule.id.clone(),
                    message: format!("{message}; fix the rule before writing"),
                });
            }
        }
        // Content that is not valid UTF-8 is still checked, with invalid
        // bytes replaced, so one stray byte cannot hide a forbidden pattern.
        let text = content.map(String::from_utf8_lossy);
        for directive in directives {
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
                    // Scoped by the rule's globs when it has any, project-wide when
                    // it does not. The asymmetry with require-content below is
                    // deliberate: "never write this" is meaningful everywhere (the
                    // secrets rule carries no globs), while "always write this" only
                    // makes sense somewhere.
                    if op == GuardOp::Write
                        && (rule.globs.is_empty() || any_glob_matches(&rule.globs, &rel))
                    {
                        match text.as_deref() {
                            Some(text) if matcher.is_match(text) => {
                                violations.push(GuardViolation {
                                    rule_id: rule.id.clone(),
                                    message: format!(
                                        "Content matches pattern forbidden by rule {}",
                                        rule.id
                                    ),
                                });
                            }
                            Some(_) => {}
                            None => warnings.push(content_not_checked(&rule.id)),
                        }
                    }
                }
                GuardDirective::RequireContent(matcher) => {
                    if op == GuardOp::Write
                        && !rule.globs.is_empty()
                        && any_glob_matches(&rule.globs, &rel)
                    {
                        match text.as_deref() {
                            Some(text) if !matcher.is_match(text) => {
                                violations.push(GuardViolation {
                                    rule_id: rule.id.clone(),
                                    message: format!(
                                        "Missing content required by rule {} for this path",
                                        rule.id
                                    ),
                                });
                            }
                            Some(_) => {}
                            None => warnings.push(content_not_checked(&rule.id)),
                        }
                    }
                }
                GuardDirective::RequireSkill(name) => {
                    if matches!(op, GuardOp::Write | GuardOp::Delete)
                        && !require_skill_exempt
                        && !skill_satisfies_require(&skills, &name)
                    {
                        violations.push(GuardViolation {
                                rule_id: rule.id.clone(),
                                message: format!(
                                    "Required skill '{name}' is missing, disabled, not approved, or not alwaysApply (rule {})",
                                    rule.id
                                ),
                            });
                    }
                }
            }
        }
    }

    Ok(GuardResult {
        allowed: violations.is_empty(),
        violations,
        warnings,
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
    /// `Ok(None)` for an empty value; `Err` for a `/regex/` that does not compile.
    fn parse(raw: &str) -> Result<Option<Self>, String> {
        let raw = raw.trim();
        if raw.len() >= 2 && raw.starts_with('/') && raw.ends_with('/') {
            regex::Regex::new(&raw[1..raw.len() - 1])
                .map(|re| Some(GuardMatcher::Regex(re)))
                .map_err(|e| format!("regex {raw} is invalid: {e}"))
        } else if !raw.is_empty() {
            Ok(Some(GuardMatcher::Substring(raw.to_string())))
        } else {
            Ok(None)
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
    RequireSkill(String),
}

const FORBID_PATH_PREFIX: &str = "forbid-path:";
const FORBID_CONTENT_PREFIX: &str = "forbid-content:";
const REQUIRE_CONTENT_PREFIX: &str = "require-content:";
const REQUIRE_SKILL_PREFIX: &str = "require-skill:";

/// Scan a rule body for `guard: <keyword>: "<value>"` lines. Matching is
/// intentionally narrow (exact `guard:` prefix + a known keyword) so ordinary
/// prose mentioning the word "guard" cannot accidentally opt a rule in.
/// Returns the directives that parse and a message for each one that does not
/// (an invalid regex or glob).
fn parse_guard_directives(body: &str) -> (Vec<GuardDirective>, Vec<String>) {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for raw_line in body.lines() {
        let line = raw_line
            .trim()
            .trim_start_matches(['-', '*', '`'])
            .trim_end_matches('`')
            .trim();
        let lower = line.to_ascii_lowercase();
        let Some(after_guard) = lower.strip_prefix("guard:") else {
            continue;
        };
        let after_guard = after_guard.trim();
        let value_offset = line.len() - after_guard.len();
        let rest = &line[value_offset..];

        type Build = fn(String) -> Result<Option<GuardDirective>, String>;
        let (prefix_len, build): (usize, Build) = if after_guard.starts_with(FORBID_PATH_PREFIX) {
            (FORBID_PATH_PREFIX.len(), |v| {
                build_glob(&v)
                    .map(|_| Some(GuardDirective::ForbidPath(v.clone())))
                    .map_err(|e| format!("forbid-path glob '{v}' is invalid: {e}"))
            })
        } else if after_guard.starts_with(FORBID_CONTENT_PREFIX) {
            (FORBID_CONTENT_PREFIX.len(), |v| {
                GuardMatcher::parse(&v).map(|m| m.map(GuardDirective::ForbidContent))
            })
        } else if after_guard.starts_with(REQUIRE_CONTENT_PREFIX) {
            (REQUIRE_CONTENT_PREFIX.len(), |v| {
                GuardMatcher::parse(&v).map(|m| m.map(GuardDirective::RequireContent))
            })
        } else if after_guard.starts_with(REQUIRE_SKILL_PREFIX) {
            (REQUIRE_SKILL_PREFIX.len(), |v| {
                let name = v.trim();
                Ok((!name.is_empty()).then(|| GuardDirective::RequireSkill(name.to_string())))
            })
        } else {
            continue;
        };

        if let Some(value) = extract_quoted(&rest[prefix_len..]) {
            match build(value) {
                Ok(Some(directive)) => out.push(directive),
                Ok(None) => {}
                Err(problem) => problems.push(problem),
            }
        }
    }
    (out, problems)
}

fn content_not_checked(rule_id: &str) -> String {
    format!("content not checked for rule {rule_id}: the write came without content")
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

/// `rule_id` of the violation for a path that resolves outside the project.
const OUTSIDE_PROJECT_RULE: &str = "outside-project";

/// Glob matching follows the file system: case-insensitive on macOS and
/// Windows, where `SRC/api` and `src/api` name the same folder.
const CASE_INSENSITIVE_FS: bool = cfg!(any(target_os = "macos", windows));

/// Removes `.` and resolves `..` without touching the file system.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(component);
                }
            }
            other => out.push(other),
        }
    }
    out
}

/// Resolves symbolic links in the deepest part of `path` that exists and
/// keeps the rest as written, so a file that does not exist yet still gets
/// the real location of its folder.
fn resolve_existing_prefix(path: &Path) -> PathBuf {
    let mut existing = path.to_path_buf();
    let mut rest = Vec::new();
    loop {
        if let Ok(real) = existing.canonicalize() {
            let mut out = real;
            for part in rest.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (
            existing.file_name().map(|n| n.to_os_string()),
            existing.parent(),
        ) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// The project-relative spelling of `path` with `/` separators, after `.`,
/// `..` and symbolic links are resolved. A relative `path` is taken relative
/// to the project root. `None` when the path ends up outside the project.
fn project_relative(project_root: &Path, path: &Path) -> Option<String> {
    let absolute_root = std::path::absolute(project_root).unwrap_or(project_root.to_path_buf());
    let root = lexical_normalize(&absolute_root);
    let joined = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let lexical = lexical_normalize(&joined);
    let rel = match (root.canonicalize(), lexical.starts_with(&root)) {
        (Ok(real_root), _) => resolve_existing_prefix(&lexical)
            .strip_prefix(&real_root)
            .ok()?
            .to_path_buf(),
        (Err(_), true) => lexical.strip_prefix(&root).ok()?.to_path_buf(),
        (Err(_), false) => return None,
    };
    Some(rel.to_string_lossy().replace('\\', "/"))
}

fn build_glob(pattern: &str) -> Result<globset::Glob, globset::Error> {
    globset::GlobBuilder::new(pattern)
        .case_insensitive(CASE_INSENSITIVE_FS)
        .build()
}

/// An invalid pattern never matches here; [`guard_operation_with_extra_skills`]
/// reports it as a broken guard before matching.
fn path_matches_glob(pattern: &str, rel_path: &str) -> bool {
    build_glob(pattern)
        .map(|g| g.compile_matcher().is_match(rel_path))
        .unwrap_or(false)
}

fn any_glob_matches(patterns: &[String], rel_path: &str) -> bool {
    patterns.iter().any(|p| path_matches_glob(p, rel_path))
}

/// Policy and template paths stay writable so seed/index can repair a missing skill.
fn is_require_skill_exempt(rel: &str) -> bool {
    let r = rel.replace('\\', "/");
    r.starts_with(".ax/policy/")
        || r.contains("/.ax/policy/")
        || r.starts_with(".agents/")
        || r.contains("/.agents/")
        || r.starts_with("crates/ax-policy/templates/")
}

fn skill_satisfies_require(skills: &[crate::types::PolicySkillRow], name: &str) -> bool {
    skills.iter().any(|s| {
        s.name.eq_ignore_ascii_case(name)
            && s.enabled
            && (s.status.is_empty() || s.status.eq_ignore_ascii_case("approved"))
            && s.always_apply
    })
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
                scope TEXT NOT NULL DEFAULT 'project',
                storage TEXT, source TEXT, root_id TEXT, stub_path TEXT, skill_group TEXT,
                properties TEXT NOT NULL DEFAULT '{}'
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "CREATE TABLE policy_skills (
                name TEXT PRIMARY KEY, description TEXT, always_apply INTEGER NOT NULL DEFAULT 0,
                triggers TEXT, tags TEXT,
                priority INTEGER, context_task TEXT, body TEXT, source_path TEXT,
                content_hash TEXT, updated_at INTEGER,
                enabled INTEGER NOT NULL DEFAULT 1, status TEXT NOT NULL DEFAULT 'approved',
                scope TEXT NOT NULL DEFAULT 'project',
                storage TEXT, source TEXT, root_id TEXT, stub_path TEXT, skill_group TEXT,
                properties TEXT NOT NULL DEFAULT '{}'
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
        insert_rule(
            &pool,
            "no-op-rule",
            "[]",
            "Just a reminder, nothing enforceable here.",
        )
        .await;
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
        insert_rule(
            &pool,
            "no-pem",
            "[]",
            "Never commit key material.\nguard: forbid-path: \"**/*.pem\"\n",
        )
        .await;
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
        let bad = guard_operation(
            &pool,
            root,
            &target,
            GuardOp::Write,
            Some(b"eval(userInput)"),
        )
        .await
        .unwrap();
        assert!(!bad.allowed);
        let good = guard_operation(
            &pool,
            root,
            &target,
            GuardOp::Write,
            Some(b"JSON.parse(userInput)"),
        )
        .await
        .unwrap();
        assert!(good.allowed);
    }

    #[tokio::test]
    async fn forbid_content_directive_supports_regex() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(
            &pool,
            "no-secret-key",
            "[]",
            "guard: forbid-content: \"/sk-[A-Za-z0-9]{10,}/\"",
        )
        .await;
        let root = dir.path();
        let target = root.join("config.ts");
        let bad = guard_operation(
            &pool,
            root,
            &target,
            GuardOp::Write,
            Some(b"const key = 'sk-abcdefghijklmnop';"),
        )
        .await
        .unwrap();
        assert!(!bad.allowed);
    }

    /// A rule that names globs means those files, not the whole project.
    ///
    /// `graph-only-query-path` bans `read_to_string` in six query-path modules
    /// while the indexer next door *must* read files. An unscoped forbid-content
    /// blocks writing the indexer, the gate test that lists the banned spellings,
    /// and the mutation script — so the rule that protects one directory makes
    /// the repo unworkable, and the guard gets switched off.
    #[tokio::test]
    async fn forbid_content_directive_is_scoped_by_rule_globs() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(
            &pool,
            "graph-only",
            "[\"crates/ax-context/src/explore.rs\"]",
            "guard: forbid-content: \"read_to_string\"",
        )
        .await;
        let root = dir.path();

        let in_scope = root
            .join("crates")
            .join("ax-context")
            .join("src")
            .join("explore.rs");
        let blocked = guard_operation(
            &pool,
            root,
            &in_scope,
            GuardOp::Write,
            Some(b"std::fs::read_to_string(p)"),
        )
        .await
        .unwrap();
        assert!(
            !blocked.allowed,
            "the rule's own file must still be guarded"
        );

        let out_of_scope = root
            .join("crates")
            .join("ax-extraction")
            .join("src")
            .join("orchestrator.rs");
        let allowed = guard_operation(
            &pool,
            root,
            &out_of_scope,
            GuardOp::Write,
            Some(b"std::fs::read_to_string(p)"),
        )
        .await
        .unwrap();
        assert!(
            allowed.allowed,
            "a file outside the rule's globs must not be blocked: {:?}",
            allowed.violations
        );
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
        let missing = guard_operation(
            &pool,
            root,
            &route,
            GuardOp::Write,
            Some(b"export function handler() {}"),
        )
        .await
        .unwrap();
        assert!(!missing.allowed);

        let present = guard_operation(
            &pool,
            root,
            &route,
            GuardOp::Write,
            Some(b"requireAuth(); export function handler() {}"),
        )
        .await
        .unwrap();
        assert!(present.allowed);

        // Out-of-scope path (doesn't match the rule's globs) must not be blocked.
        let other = root.join("src").join("lib.rs");
        let out_of_scope =
            guard_operation(&pool, root, &other, GuardOp::Write, Some(b"fn main() {}"))
                .await
                .unwrap();
        assert!(out_of_scope.allowed);
    }

    async fn insert_skill(
        pool: &SqlitePool,
        name: &str,
        always_apply: i32,
        enabled: i32,
        status: &str,
    ) {
        sqlx::query(
            "INSERT INTO policy_skills (name, description, always_apply, triggers, tags, priority, context_task, body, source_path, content_hash, updated_at, enabled, status, scope)
             VALUES (?, '', ?, '[]', '[]', 50, NULL, '', '', '', 0, ?, ?, 'company')",
        )
        .bind(name)
        .bind(always_apply)
        .bind(enabled)
        .bind(status)
        .execute(pool)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn require_skill_blocks_when_skill_missing() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(
            &pool,
            "old-coder-mandatory",
            "[]",
            "guard: require-skill: \"old-coder\"\n",
        )
        .await;
        let root = dir.path();
        let target = root.join("src").join("lib.rs");
        let result = guard_operation(&pool, root, &target, GuardOp::Write, Some(b"fn f() {}"))
            .await
            .unwrap();
        assert!(!result.allowed);
        assert_eq!(result.violations[0].rule_id, "old-coder-mandatory");
        assert!(result.violations[0].message.contains("old-coder"));
    }

    #[tokio::test]
    async fn require_skill_blocks_when_skill_not_always_apply() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(
            &pool,
            "old-coder-mandatory",
            "[]",
            "guard: require-skill: \"old-coder\"\n",
        )
        .await;
        insert_skill(&pool, "old-coder", 0, 1, "approved").await;
        let root = dir.path();
        let target = root.join("app.rs");
        let result = guard_operation(&pool, root, &target, GuardOp::Write, Some(b"fn f() {}"))
            .await
            .unwrap();
        assert!(!result.allowed);
        assert!(result.violations[0].message.contains("alwaysApply"));
    }

    #[tokio::test]
    async fn require_skill_parses_backtick_wrapped_directive() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(
            &pool,
            "old-coder-mandatory",
            "[]",
            "`guard: require-skill: \"old-coder\"`\n",
        )
        .await;
        insert_skill(&pool, "old-coder", 1, 1, "approved").await;
        let root = dir.path();
        let present = guard_operation(
            &pool,
            root,
            &root.join("other.rs"),
            GuardOp::Write,
            Some(b"fn f() {}"),
        )
        .await
        .unwrap();
        assert!(present.allowed, "{:?}", present.violations);
        sqlx::query("DELETE FROM policy_skills WHERE name = 'old-coder'")
            .execute(&pool)
            .await
            .unwrap();
        let missing = guard_operation(
            &pool,
            root,
            &root.join("other.rs"),
            GuardOp::Write,
            Some(b"fn f() {}"),
        )
        .await
        .unwrap();
        assert!(!missing.allowed);
    }

    #[tokio::test]
    async fn require_skill_allows_when_skill_is_always_apply() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(
            &pool,
            "old-coder-mandatory",
            "[]",
            "guard: require-skill: \"old-coder\"\n",
        )
        .await;
        insert_skill(&pool, "old-coder", 1, 1, "approved").await;
        let root = dir.path();
        let target = root.join("app.rs");
        let result = guard_operation(&pool, root, &target, GuardOp::Write, Some(b"fn f() {}"))
            .await
            .unwrap();
        assert!(result.allowed, "{:?}", result.violations);
    }

    #[tokio::test]
    async fn require_skill_exempts_policy_paths() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(
            &pool,
            "old-coder-mandatory",
            "[]",
            "guard: require-skill: \"old-coder\"\n",
        )
        .await;
        let root = dir.path();
        let target = root.join(".ax").join("policy").join("rules").join("x.mdc");
        let result = guard_operation(
            &pool,
            root,
            &target,
            GuardOp::Write,
            Some(b"---\nid: x\n---\n"),
        )
        .await
        .unwrap();
        assert!(result.allowed, "{:?}", result.violations);
    }

    async fn pool_with_auth_rule() -> (TempDir, SqlitePool) {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(
            &pool,
            "routes-need-auth",
            "[\"src/api/**\"]",
            "guard: require-content: \"requireAuth(\"",
        )
        .await;
        std::fs::create_dir_all(dir.path().join("src/api")).unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        (dir, pool)
    }

    #[tokio::test]
    async fn dot_dot_segments_do_not_escape_rule_globs() {
        let (dir, pool) = pool_with_auth_rule().await;
        let root = dir.path();
        for spelled in [
            root.join("docs/../src/api/users.ts"),
            root.join("./src/./api/users.ts"),
            std::path::PathBuf::from("src/api/users.ts"),
        ] {
            let result = guard_operation(
                &pool,
                root,
                &spelled,
                GuardOp::Write,
                Some(b"export function handler() {}"),
            )
            .await
            .unwrap();
            assert!(!result.allowed, "{} must be guarded", spelled.display());
        }
    }

    #[tokio::test]
    async fn paths_outside_the_project_are_refused() {
        let (dir, pool) = pool_with_auth_rule().await;
        let root = dir.path().join("src");
        let outside = root.join("../docs/../../elsewhere.ts");
        let result = guard_operation(&pool, &root, &outside, GuardOp::Write, Some(b"x"))
            .await
            .unwrap();
        assert!(!result.allowed);
        assert!(
            result.violations[0].message.contains("outside the project"),
            "{:?}",
            result.violations
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_link_out_of_the_project_is_refused() {
        let (dir, pool) = pool_with_auth_rule().await;
        let outside = TempDir::new().unwrap();
        let project = dir.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::os::unix::fs::symlink(outside.path(), project.join("link")).unwrap();
        let result = guard_operation(
            &pool,
            &project,
            &project.join("link/x.ts"),
            GuardOp::Write,
            Some(b"x"),
        )
        .await
        .unwrap();
        assert!(!result.allowed, "{:?}", result.violations);
    }

    #[cfg(any(target_os = "macos", windows))]
    #[tokio::test]
    async fn globs_ignore_case_on_case_insensitive_platforms() {
        let (dir, pool) = pool_with_auth_rule().await;
        let root = dir.path();
        let result = guard_operation(
            &pool,
            root,
            &root.join("SRC/API/users.ts"),
            GuardOp::Write,
            Some(b"export function handler() {}"),
        )
        .await
        .unwrap();
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn a_broken_regex_directive_blocks_instead_of_vanishing() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(
            &pool,
            "no-secret-key",
            "[]",
            "guard: forbid-content: \"/sk-[A-Z{10,}/\"",
        )
        .await;
        let root = dir.path();
        let result = guard_operation(
            &pool,
            root,
            &root.join("config.ts"),
            GuardOp::Write,
            Some(b"const key = 'sk-ABCDEFGHIJKL';"),
        )
        .await
        .unwrap();
        assert!(!result.allowed);
        assert!(
            result.violations[0].message.contains("invalid"),
            "{:?}",
            result.violations
        );
        // The rule file itself stays writable so the rule can be repaired.
        let repair = guard_operation(
            &pool,
            root,
            &root.join(".agents/rules/no-secret-key.mdc"),
            GuardOp::Write,
            Some(b"---\nid: no-secret-key\n---\n"),
        )
        .await
        .unwrap();
        assert!(repair.allowed, "{:?}", repair.violations);
        assert!(!repair.warnings.is_empty());
    }

    #[tokio::test]
    async fn a_broken_glob_blocks_instead_of_never_matching() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(&pool, "no-keys", "[]", "guard: forbid-path: \"keys/[abc\"").await;
        insert_rule(
            &pool,
            "scoped",
            "[\"src/[api\"]",
            "guard: require-content: \"requireAuth(\"",
        )
        .await;
        let root = dir.path();
        let result = guard_operation(&pool, root, &root.join("a.rs"), GuardOp::Write, Some(b"x"))
            .await
            .unwrap();
        let ids: Vec<&str> = result
            .violations
            .iter()
            .map(|v| v.rule_id.as_str())
            .collect();
        assert!(ids.contains(&"no-keys"), "{ids:?}");
        assert!(ids.contains(&"scoped"), "{ids:?}");
    }

    #[tokio::test]
    async fn forbid_content_checks_content_that_is_not_utf8() {
        let (dir, pool) = pool_with_utf8_rule().await;
        insert_rule(&pool, "no-eval", "[]", "guard: forbid-content: \"eval(\"").await;
        let root = dir.path();
        let result = guard_operation(
            &pool,
            root,
            &root.join("app.js"),
            GuardOp::Write,
            Some(b"\xff eval(userInput)"),
        )
        .await
        .unwrap();
        assert!(!result.allowed);
    }

    #[tokio::test]
    async fn a_write_without_content_reports_unchecked_content_rules() {
        let (dir, pool) = pool_with_auth_rule().await;
        let root = dir.path();
        let result = guard_operation(
            &pool,
            root,
            &root.join("src/api/users.ts"),
            GuardOp::Write,
            None,
        )
        .await
        .unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("not checked")),
            "{:?}",
            result.warnings
        );
    }
}
