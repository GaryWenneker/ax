use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RankClass {
    HardRequired,
    HighValue,
    Optional,
}

#[derive(Debug, Clone)]
pub struct RankItem {
    pub id: String,
    pub class: RankClass,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RankNote {
    pub id: String,
    pub keep: bool,
    pub reason: String,
    pub relevance: f64,
    pub measured: bool,
}

/// Reorder optional and high-value blocks by lexical overlap with the task.
/// Hard-required items are always kept and never reordered past each other.
pub fn plan(items: &[RankItem], query: &str) -> Vec<RankNote> {
    let mut seen: Vec<String> = Vec::new();
    let mut notes = Vec::with_capacity(items.len());
    let measured = !query.trim().is_empty();
    let query_tokens = tokens(query);
    for item in items {
        let item_tokens = tokens(&item.text);
        let key = item_tokens.join(" ");
        let duplicate = !key.is_empty() && seen.iter().any(|prior| prior == &key);
        if !duplicate && !key.is_empty() {
            seen.push(key);
        }
        let relevance = token_overlap(&query_tokens, &item_tokens);
        if item.class == RankClass::HardRequired {
            notes.push(RankNote {
                id: item.id.clone(),
                keep: true,
                reason: "hard-required".into(),
                relevance,
                measured,
            });
            continue;
        }
        if duplicate {
            notes.push(RankNote {
                id: item.id.clone(),
                keep: false,
                reason: "duplicate".into(),
                relevance,
                measured,
            });
            continue;
        }
        notes.push(RankNote {
            id: item.id.clone(),
            keep: true,
            reason: "lexical-overlap".into(),
            relevance,
            measured,
        });
    }
    notes
}

pub fn keep_order(items: &[RankItem], notes: &[RankNote]) -> Vec<String> {
    let mut hard = Vec::new();
    let mut high = Vec::new();
    let mut optional = Vec::new();
    for item in items {
        let note = notes.iter().find(|note| note.id == item.id);
        if item.class == RankClass::HardRequired {
            hard.push(item.id.clone());
            continue;
        }
        if note.is_some_and(|note| note.keep) {
            match item.class {
                RankClass::HighValue => high.push(item),
                RankClass::Optional | RankClass::HardRequired => optional.push(item),
            }
        }
    }
    high.sort_by(|a, b| {
        relevance(notes, &b.id)
            .partial_cmp(&relevance(notes, &a.id))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    optional.sort_by(|a, b| {
        relevance(notes, &b.id)
            .partial_cmp(&relevance(notes, &a.id))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut order = hard;
    order.extend(high.into_iter().map(|item| item.id.clone()));
    order.extend(optional.into_iter().map(|item| item.id.clone()));
    order
}

fn relevance(notes: &[RankNote], id: &str) -> f64 {
    notes
        .iter()
        .find(|note| note.id == id)
        .map(|note| note.relevance)
        .unwrap_or(0.0)
}

fn token_overlap(left: &[String], right: &[String]) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let shared = left.iter().filter(|token| right.contains(token)).count();
    shared as f64 / left.len() as f64
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| part.len() > 2)
        .map(|part| part.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, class: RankClass, text: &str) -> RankItem {
        RankItem {
            id: id.into(),
            class,
            text: text.into(),
        }
    }

    #[test]
    fn hard_required_stays_even_when_it_does_not_match() {
        let items = vec![
            item("required", RankClass::HardRequired, "unrelated policy"),
            item("opt", RankClass::Optional, "hash vectors in memory"),
        ];
        let notes = plan(&items, "hash vectors");
        assert!(notes.iter().any(|note| note.id == "required" && note.keep));
        let order = keep_order(&items, &notes);
        assert_eq!(order[0], "required");
    }

    #[test]
    fn duplicate_optional_is_dropped_and_better_overlap_goes_first() {
        let items = vec![
            item("a", RankClass::Optional, "graph edges stay authoritative"),
            item("b", RankClass::Optional, "graph edges stay authoritative"),
            item("c", RankClass::Optional, "memory hash vectors"),
        ];
        let notes = plan(&items, "memory hash");
        assert!(!notes.iter().find(|note| note.id == "b").unwrap().keep);
        assert_eq!(
            notes.iter().find(|note| note.id == "b").unwrap().reason,
            "duplicate"
        );
        let order = keep_order(&items, &notes);
        assert_eq!(order, vec!["c".to_string(), "a".to_string()]);
    }

    #[test]
    fn relevance_is_the_share_of_query_words_found_in_the_block() {
        let items = vec![item("half", RankClass::Optional, "memory vectors")];
        assert_eq!(plan(&items, "memory hash")[0].relevance, 0.5);
    }

    #[test]
    fn words_of_two_letters_or_fewer_are_ignored() {
        let items = vec![item("short", RankClass::Optional, "io db")];
        assert_eq!(plan(&items, "fix io in the db layer")[0].relevance, 0.0);
    }

    #[test]
    fn a_query_of_only_short_words_scores_every_block_zero() {
        let items = vec![item("long", RankClass::Optional, "fix the parser layer")];
        let note = &plan(&items, "io db")[0];
        assert_eq!(note.relevance, 0.0);
        assert!(note.measured);
    }

    #[test]
    fn relevance_is_measured_only_when_there_is_a_query() {
        let items = vec![item("opt", RankClass::Optional, "memory")];
        assert!(plan(&items, "memory")[0].measured);
        assert!(!plan(&items, "  ")[0].measured);
    }
}
