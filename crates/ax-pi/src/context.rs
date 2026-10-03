//! Context selection and a selective cache.
//!
//! Selection reuses [`ax_usage::select_context`]: always-apply rules are
//! `HardRequired` and are kept even when they exceed the budget. Everything
//! else is ordered before that call.

use std::collections::HashMap;

use ax_usage::{select_context, ContextBlock, ContextClass};

use crate::economics::estimate_text;
use crate::types::{
    CachedContext, ContextCacheKey, ContextRequest, ContextResult, ContextSection, SectionType,
};

pub struct AxContextEngine {
    max_tokens: u32,
}

impl AxContextEngine {
    pub fn new(max_tokens: u32) -> Self {
        Self { max_tokens }
    }

    pub fn select(
        &self,
        request: &ContextRequest,
        mut sections: Vec<ContextSection>,
    ) -> ContextResult {
        sections.sort_by(|left, right| {
            tier(left)
                .cmp(&tier(right))
                .then(right.priority.cmp(&left.priority))
                .then(
                    density(right)
                        .partial_cmp(&density(left))
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
                .then(left.token_estimate.cmp(&right.token_estimate))
                .then(left.id.cmp(&right.id))
        });
        let blocks: Vec<ContextBlock> = sections
            .iter()
            .map(|section| ContextBlock {
                id: section.id.clone(),
                class: if section.mandatory && section.kind == SectionType::Rule {
                    ContextClass::HardRequired
                } else if section.kind == SectionType::Summary {
                    ContextClass::Optional
                } else {
                    ContextClass::HighValue
                },
                text: section.content.clone(),
            })
            .collect();
        let kept = select_context(&blocks, Some(self.max_tokens));
        let kept_ids: Vec<&str> = kept.iter().map(|block| block.id.as_str()).collect();
        let mut chosen: Vec<ContextSection> = sections
            .into_iter()
            .filter(|section| kept_ids.iter().any(|id| *id == section.id))
            .collect();
        chosen.sort_by_key(|section| {
            kept_ids
                .iter()
                .position(|id| *id == section.id)
                .unwrap_or(usize::MAX)
        });
        for section in &mut chosen {
            section.token_estimate = estimate_text(&section.content);
        }
        let estimated_tokens = chosen
            .iter()
            .map(|section| section.token_estimate)
            .fold(0u32, u32::saturating_add);
        let context_version = cache_key_string(&ContextCacheKey {
            project_id: request.session_id.clone(),
            git_revision: None,
            working_tree_hash: None,
            intent_hash: intent_hash(request.user_intent.as_deref().unwrap_or("")),
            graph_version: chosen
                .iter()
                .map(|section| section.id.as_str())
                .collect::<Vec<_>>()
                .join(","),
            policy_version: String::new(),
            skill_version: String::new(),
        });
        ContextResult {
            context_version,
            sections: chosen,
            estimated_tokens,
            cache_hit: false,
        }
    }
}

fn tier(section: &ContextSection) -> u8 {
    if section.mandatory && section.kind == SectionType::Rule {
        return 0;
    }
    match section.kind {
        SectionType::Skill => 1,
        SectionType::Constraint => 2,
        SectionType::Rule => 2,
        SectionType::Graph => 3,
        SectionType::Memory => 4,
        SectionType::Source => 5,
        SectionType::ToolResult => 6,
        SectionType::Summary => 7,
    }
}

fn density(section: &ContextSection) -> f64 {
    f64::from(section.priority) / f64::from(section.token_estimate.max(1))
}

#[derive(Debug, Default)]
pub struct ContextCache {
    entries: HashMap<String, CachedContext>,
}

impl ContextCache {
    pub fn key_string(key: &ContextCacheKey) -> String {
        cache_key_string(key)
    }

    pub fn get(&self, key: &ContextCacheKey) -> Option<&CachedContext> {
        self.entries.get(&cache_key_string(key))
    }

    pub fn insert(&mut self, key: ContextCacheKey, mut value: CachedContext) {
        value.key = cache_key_string(&key);
        value.project_id = key.project_id;
        value.git_revision = key.git_revision;
        value.policy_version = key.policy_version;
        value.skill_version = key.skill_version;
        value.graph_version = key.graph_version;
        self.entries.insert(value.key.clone(), value);
    }

    pub fn invalidate_paths(&mut self, paths: &[String]) {
        self.entries.retain(|_, entry| {
            !entry.source_hashes.iter().any(|source| {
                paths
                    .iter()
                    .any(|path| source == path || source.ends_with(path) || path.ends_with(source))
            })
        });
    }

    pub fn invalidate_head(&mut self, project_id: &str, new_head: &str) {
        self.entries.retain(|_, entry| {
            entry.project_id != project_id || entry.git_revision.as_deref() == Some(new_head)
        });
    }

    pub fn invalidate_policy(&mut self, project_id: &str, version: &str) {
        self.entries
            .retain(|_, entry| entry.project_id != project_id || entry.policy_version == version);
    }

    pub fn invalidate_skill(&mut self, project_id: &str, version: &str) {
        self.entries
            .retain(|_, entry| entry.project_id != project_id || entry.skill_version == version);
    }

    pub fn invalidate_graph(&mut self, project_id: &str, version: &str) {
        self.entries
            .retain(|_, entry| entry.project_id != project_id || entry.graph_version == version);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn entries(&self) -> Vec<&CachedContext> {
        self.entries.values().collect()
    }
}

pub fn intent_hash(intent: &str) -> String {
    let normalized = intent
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    blake3::hash(normalized.as_bytes()).to_hex().to_string()
}

pub fn cache_key_string(key: &ContextCacheKey) -> String {
    let material = format!(
        "{}|{}|{}|{}|{}|{}|{}",
        key.project_id,
        key.git_revision.as_deref().unwrap_or(""),
        key.working_tree_hash.as_deref().unwrap_or(""),
        key.intent_hash,
        key.graph_version,
        key.policy_version,
        key.skill_version
    );
    blake3::hash(material.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(id: &str, kind: SectionType, content: &str, mandatory: bool) -> ContextSection {
        ContextSection {
            id: id.into(),
            kind,
            priority: 1,
            token_estimate: 1,
            content: content.into(),
            mandatory,
            source_paths: Vec::new(),
        }
    }

    #[test]
    fn mandatory_rule_survives_a_tiny_budget() {
        let engine = AxContextEngine::new(1);
        let result = engine.select(
            &ContextRequest {
                session_id: "s".into(),
                turn_id: "t".into(),
                user_intent: Some("fix".into()),
                repository_root: None,
                changed_files: Vec::new(),
                previous_tool_names: Vec::new(),
            },
            vec![
                section(
                    "rule",
                    SectionType::Rule,
                    "always apply this rule about secrets",
                    true,
                ),
                section(
                    "bg",
                    SectionType::Summary,
                    &"background ".repeat(400),
                    false,
                ),
            ],
        );
        assert!(result.sections.iter().any(|section| section.id == "rule"));
        assert!(result.sections.iter().all(|section| section.id != "bg"));
    }

    #[test]
    fn priority_order_prefers_skills_over_background() {
        let engine = AxContextEngine::new(8);
        let result = engine.select(
            &ContextRequest {
                session_id: "s".into(),
                turn_id: "t".into(),
                user_intent: None,
                repository_root: None,
                changed_files: Vec::new(),
                previous_tool_names: Vec::new(),
            },
            vec![
                section(
                    "bg",
                    SectionType::Summary,
                    "optional background that is long enough",
                    false,
                ),
                section("skill", SectionType::Skill, "skill", false),
            ],
        );
        assert_eq!(result.sections[0].id, "skill");
    }

    #[test]
    fn file_change_invalidates_only_affected_cache_entries() {
        let mut cache = ContextCache::default();
        let base = ContextCacheKey {
            project_id: "p".into(),
            git_revision: Some("aaa".into()),
            working_tree_hash: None,
            intent_hash: intent_hash("one"),
            graph_version: "g".into(),
            policy_version: "1".into(),
            skill_version: "1".into(),
        };
        let mut other = base.clone();
        other.intent_hash = intent_hash("two");
        cache.insert(
            base.clone(),
            cached(&base, vec!["src/UserService.cs".into()]),
        );
        cache.insert(other.clone(), cached(&other, vec!["docs/readme.md".into()]));
        cache.invalidate_paths(&["src/UserService.cs".into()]);
        assert!(cache.get(&base).is_none());
        assert!(cache.get(&other).is_some());
    }

    #[test]
    fn policy_change_keeps_other_projects() {
        let mut cache = ContextCache::default();
        let mut here = sample_key("here");
        here.policy_version = "1".into();
        let mut there = sample_key("there");
        there.policy_version = "1".into();
        cache.insert(here.clone(), cached(&here, Vec::new()));
        cache.insert(there.clone(), cached(&there, Vec::new()));
        cache.invalidate_policy("here", "2");
        assert!(cache.get(&here).is_none());
        assert!(cache.get(&there).is_some());
        cache.invalidate_head("there", "bbb");
        assert!(cache.get(&there).is_none());
        cache.insert(there.clone(), cached(&there, Vec::new()));
        cache.invalidate_skill("there", "9");
        assert!(cache.get(&there).is_none());
        cache.insert(there.clone(), cached(&there, Vec::new()));
        cache.invalidate_graph("there", "9");
        assert!(cache.get(&there).is_none());
        assert_eq!(cache.len(), 0);
        assert_eq!(ContextCache::key_string(&here), cache_key_string(&here));
    }

    #[test]
    fn same_intent_has_a_stable_cache_key() {
        let key = sample_key("p");
        assert_eq!(cache_key_string(&key), cache_key_string(&key));
        let mut changed = key.clone();
        changed.intent_hash = intent_hash("other");
        assert_ne!(cache_key_string(&key), cache_key_string(&changed));
    }

    fn sample_key(project: &str) -> ContextCacheKey {
        ContextCacheKey {
            project_id: project.into(),
            git_revision: Some("aaa".into()),
            working_tree_hash: None,
            intent_hash: intent_hash("find user"),
            graph_version: "g".into(),
            policy_version: "1".into(),
            skill_version: "1".into(),
        }
    }

    fn cached(key: &ContextCacheKey, paths: Vec<String>) -> CachedContext {
        CachedContext {
            key: String::new(),
            created_at: 1,
            sections: Vec::new(),
            estimated_tokens: 0,
            source_hashes: paths,
            project_id: key.project_id.clone(),
            git_revision: key.git_revision.clone(),
            policy_version: key.policy_version.clone(),
            skill_version: key.skill_version.clone(),
            graph_version: key.graph_version.clone(),
        }
    }
}
