//! Context selection and snapshot identity.
//!
//! Hard-required blocks are kept whole. Optional blocks are dropped as units.
//! Token counts use the shared tokenizer.

use crate::tokenizer::count_tokens;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextClass {
    HardRequired,
    HighValue,
    Optional,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBlock {
    pub id: String,
    pub class: ContextClass,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSnapshot {
    pub content_hash: String,
    pub session_id: String,
    pub project_id: Option<String>,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub token_count: i64,
    pub part_hashes: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextChange {
    Empty,
    Same,
    Partial,
    Changed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextEfficiency {
    pub raw_context_tokens: i64,
    pub selected_context_tokens: i64,
    pub cache_read_tokens: Option<i64>,
    pub new_context_tokens: Option<i64>,
    pub tokens_avoided: i64,
}

pub fn select_context(blocks: &[ContextBlock], budget_tokens: Option<u32>) -> Vec<ContextBlock> {
    let Some(budget) = budget_tokens else {
        return blocks.to_vec();
    };
    let mut kept = Vec::new();
    let mut used = 0u32;
    for class in [
        ContextClass::HardRequired,
        ContextClass::HighValue,
        ContextClass::Optional,
    ] {
        for block in blocks.iter().filter(|b| b.class == class) {
            let tokens = count_tokens(&block.text) as u32;
            if class == ContextClass::HardRequired || used.saturating_add(tokens) <= budget {
                kept.push(block.clone());
                used = used.saturating_add(tokens);
            }
        }
    }
    kept
}

/// Tokens left for optional preflight blocks after hard-required text is counted.
///
/// `None` means no context budget is configured. The caller keeps its existing caps.
/// A zero room means optional blocks must be omitted. The hard-required text is not trimmed.
pub fn optional_room(already_sent: &str, budget_tokens: Option<u32>) -> Option<u32> {
    let budget = budget_tokens?;
    Some(budget.saturating_sub(count_tokens(already_sent) as u32))
}

pub fn fnv_hash(text: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

pub fn snapshot_from_parts(
    session_id: &str,
    project_id: Option<&str>,
    provider: Option<&str>,
    model: Option<&str>,
    parts: &[(String, String)],
) -> ContextSnapshot {
    let body = parts
        .iter()
        .map(|(id, text)| format!("{id}\n{text}"))
        .collect::<Vec<_>>()
        .join("\n");
    let part_hashes = parts
        .iter()
        .map(|(id, text)| (id.clone(), fnv_hash(text)))
        .collect::<Vec<_>>();
    ContextSnapshot {
        content_hash: fnv_hash(&body),
        session_id: session_id.to_string(),
        project_id: project_id.map(str::to_string),
        provider: provider.map(str::to_string),
        model: model.map(str::to_string),
        token_count: count_tokens(&body) as i64,
        part_hashes,
    }
}

pub fn compare_snapshots(
    previous: Option<&ContextSnapshot>,
    next: &ContextSnapshot,
) -> ContextChange {
    if next.part_hashes.is_empty() && previous.map(|p| p.part_hashes.is_empty()).unwrap_or(true) {
        return ContextChange::Empty;
    }
    let Some(previous) = previous else {
        return if next.part_hashes.is_empty() {
            ContextChange::Empty
        } else {
            ContextChange::Changed
        };
    };
    if previous.session_id == next.session_id && previous.content_hash == next.content_hash {
        return ContextChange::Same;
    }
    let overlap = next.part_hashes.iter().any(|(id, hash)| {
        previous
            .part_hashes
            .iter()
            .any(|(pid, phash)| pid == id && phash == hash)
    });
    let differ = next.part_hashes.iter().any(|(id, hash)| {
        previous
            .part_hashes
            .iter()
            .any(|(pid, phash)| pid == id && phash != hash)
            || previous.part_hashes.iter().all(|(pid, _)| pid != id)
    });
    if overlap && differ {
        ContextChange::Partial
    } else if overlap {
        ContextChange::Same
    } else {
        ContextChange::Changed
    }
}

pub fn efficiency(
    raw: i64,
    selected: i64,
    cache_read: Option<i64>,
    avoided: i64,
) -> ContextEfficiency {
    let new_context = cache_read.map(|cache| selected.saturating_sub(cache).max(0));
    ContextEfficiency {
        raw_context_tokens: raw,
        selected_context_tokens: selected,
        cache_read_tokens: cache_read,
        new_context_tokens: new_context,
        tokens_avoided: avoided,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(id: &str, class: ContextClass, text: &str) -> ContextBlock {
        ContextBlock {
            id: id.into(),
            class,
            text: text.into(),
        }
    }

    #[test]
    fn empty_context_is_empty() {
        let snap = snapshot_from_parts("s", None, None, None, &[]);
        assert_eq!(compare_snapshots(None, &snap), ContextChange::Empty);
        assert_eq!(compare_snapshots(Some(&snap), &snap), ContextChange::Empty);
    }

    #[test]
    fn unchanged_and_changed_context() {
        let parts = vec![("rule".into(), "always ship tests".into())];
        let first = snapshot_from_parts("s", None, None, None, &parts);
        let again = snapshot_from_parts("s", None, None, None, &parts);
        assert_eq!(compare_snapshots(Some(&first), &again), ContextChange::Same);
        let changed =
            snapshot_from_parts("s", None, None, None, &[("rule".into(), "other".into())]);
        assert_eq!(
            compare_snapshots(Some(&first), &changed),
            ContextChange::Changed
        );
    }

    #[test]
    fn partial_change_keeps_one_part() {
        let first = snapshot_from_parts(
            "s",
            None,
            None,
            None,
            &[("a".into(), "one".into()), ("b".into(), "two".into())],
        );
        let next = snapshot_from_parts(
            "s",
            None,
            None,
            None,
            &[("a".into(), "one".into()), ("b".into(), "edited".into())],
        );
        assert_eq!(
            compare_snapshots(Some(&first), &next),
            ContextChange::Partial
        );
    }

    #[test]
    fn hard_required_survives_a_tiny_budget_and_optional_is_dropped_whole() {
        let blocks = vec![
            block("critical", ContextClass::HardRequired, &"rule ".repeat(80)),
            block("optional", ContextClass::Optional, &"noise ".repeat(80)),
        ];
        let kept = select_context(&blocks, Some(1));
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "critical");
        assert_eq!(kept[0].text, blocks[0].text);
    }

    #[test]
    fn large_optional_context_is_omitted_when_the_budget_is_spent() {
        let blocks = vec![
            block("task", ContextClass::HighValue, "fix the parser"),
            block("extra", ContextClass::Optional, &"x".repeat(4_000)),
        ];
        let kept = select_context(&blocks, Some(20));
        assert!(kept.iter().any(|b| b.id == "task"));
        assert!(kept.iter().all(|b| b.id != "extra"));
    }

    #[test]
    fn efficiency_splits_cache_from_selected_tokens() {
        let row = efficiency(1_000, 400, Some(100), 600);
        assert_eq!(row.raw_context_tokens, 1_000);
        assert_eq!(row.selected_context_tokens, 400);
        assert_eq!(row.new_context_tokens, Some(300));
        assert_eq!(row.tokens_avoided, 600);
        assert_eq!(efficiency(10, 4, None, 6).new_context_tokens, None);
    }

    #[test]
    fn no_context_budget_leaves_the_room_unset() {
        assert_eq!(optional_room("always apply stays", None), None);
    }

    #[test]
    fn optional_room_is_zero_once_hard_required_text_fills_the_budget() {
        let hard = "rule ".repeat(80);
        assert_eq!(optional_room(&hard, Some(1)), Some(0));
    }

    #[test]
    fn optional_room_keeps_the_remainder_after_a_short_hard_block() {
        let room = optional_room("ok", Some(12_000)).unwrap();
        assert!(room > 11_000);
    }
}
