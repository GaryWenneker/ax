//! Structured extractors for deterministic learning (Phase 8).

use sqlx::SqlitePool;

use ax_utils::errors::AxError;

use crate::types::{MemoryRow, RememberInput};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lesson {
    pub kind: String,
    pub symptom: String,
    pub cause: String,
    pub fix: String,
}

/// Build a bug-fix lesson from an episode goal/outcome pair.
pub fn extract_bug_fix_lesson(goal: &str, outcome: &str) -> Option<Lesson> {
    let goal = goal.trim();
    let outcome = outcome.trim();
    if goal.is_empty() {
        return None;
    }
    let lower_goal = goal.to_ascii_lowercase();
    let looks_like_bug = ["bug", "fix", "null", "panic", "error", "fail", "fout"]
        .iter()
        .any(|w| lower_goal.contains(w));
    if !looks_like_bug {
        return None;
    }
    Some(Lesson {
        kind: "bug_fix".into(),
        symptom: goal.to_string(),
        cause: String::new(),
        fix: outcome.to_string(),
    })
}

/// Decision from a commit subject/body pair.
pub fn extract_commit_decision(subject: &str, body: &str) -> Option<Lesson> {
    let subject = subject.trim();
    let body = body.trim();
    if subject.is_empty() {
        return None;
    }
    let lower = format!("{subject} {body}").to_ascii_lowercase();
    let looks = ["because", "decided", "chose", "instead of", "besloten"]
        .iter()
        .any(|w| lower.contains(w));
    if !looks {
        return None;
    }
    Some(Lesson {
        kind: "decision".into(),
        symptom: subject.to_string(),
        cause: String::new(),
        fix: body.to_string(),
    })
}

/// Failed approach from a revert subject.
pub fn extract_revert_lesson(subject: &str) -> Option<Lesson> {
    let subject = subject.trim();
    if subject.is_empty() {
        return None;
    }
    let lower = subject.to_ascii_lowercase();
    if !(lower.contains("revert") || lower.contains("rollback")) {
        return None;
    }
    Some(Lesson {
        kind: "failed_approach".into(),
        symptom: subject.to_string(),
        cause: "reverted".into(),
        fix: String::new(),
    })
}

/// Persist a lesson into the vault as a typed memory.
pub async fn remember_lesson(pool: &SqlitePool, lesson: &Lesson) -> Result<MemoryRow, AxError> {
    let title = format!("{}: {}", lesson.kind, lesson.symptom);
    let body = format!(
        "symptom: {}\ncause: {}\nfix: {}",
        lesson.symptom, lesson.cause, lesson.fix
    );
    crate::store::remember(
        pool,
        RememberInput {
            title,
            body,
            kind: Some("lesson".into()),
            tags: vec![lesson.kind.clone(), "extracted".into()],
            files: vec![],
            source: Some("extractor".into()),
        },
    )
    .await
}

/// Raise confidence when two lessons share the same symptom subject.
pub async fn corroborate_lessons(pool: &SqlitePool, a: &str, b: &str) -> Result<bool, AxError> {
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() || b.is_empty() || a == b {
        return Ok(false);
    }
    let _ = crate::store::link_memories(pool, a, b, "corroborates").await;
    let result = sqlx::query(
        "UPDATE memories SET confidence = MIN(1.0, confidence + 0.1) WHERE id IN (?, ?)",
    )
    .bind(a)
    .bind(b)
    .execute(pool)
    .await
    .map_err(|e| AxError::Other(e.to_string()))?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::{extract_bug_fix_lesson, extract_commit_decision, extract_revert_lesson};

    #[test]
    fn extracts_a_null_panic_lesson() {
        let lesson = extract_bug_fix_lesson("null panic in parse", "green after guard").unwrap();
        assert_eq!(lesson.kind, "bug_fix");
        assert!(lesson.symptom.to_ascii_lowercase().contains("null"));
        assert!(!lesson.fix.is_empty());
        assert!(extract_bug_fix_lesson("", "green").is_none());
        assert!(extract_bug_fix_lesson("rename module", "done").is_none());
    }

    #[test]
    fn extracts_commit_decision_and_revert() {
        let d = extract_commit_decision("use hash embed", "decided against onnx because offline")
            .unwrap();
        assert_eq!(d.kind, "decision");
        assert!(extract_commit_decision("typo", "fix typo").is_none());
        let r = extract_revert_lesson("Revert \"bad cache\"").unwrap();
        assert_eq!(r.kind, "failed_approach");
        assert!(extract_revert_lesson("cleanup").is_none());
    }
}
