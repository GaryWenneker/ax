//! Preflight follows `[[links]]` in matched rules, skills and memories one hop.

use std::collections::HashSet;

use ax_memory::{MemoryMatch, MemoryRow};
use ax_policy::links::{follow_links, LinkIndex, LinkItem, LinkKind, LinkOrigin, LinkSource};
use ax_policy::types::{MatchResult, MatchedRule, MatchedSkill, PolicyRuleRow, PolicySkillRow};

/// Linked items delivered per preflight at most.
pub const LINK_CAP: usize = 5;

/// Add the rules, skills and memories that matched items link to. Returns true when
/// something was added; the inject block is then rebuilt from the new rule and skill lists.
pub fn expand_links(
    result: &mut MatchResult,
    memories: &mut Vec<MemoryMatch>,
    rules: &[PolicyRuleRow],
    skills: &[PolicySkillRow],
    memory_rows: &[MemoryRow],
) -> bool {
    if !has_links(result, memories) {
        return false;
    }
    let sources = sources(result, memories);
    let index = index(rules, skills, memory_rows);
    let delivered: HashSet<(LinkKind, String)> = sources
        .iter()
        .map(|s| (s.item.kind, s.item.id.clone()))
        .collect();
    let eligible = |item: &LinkItem| match item.kind {
        LinkKind::Rule => rules.iter().any(|r| {
            r.id == item.id && r.enabled && ax_policy::matcher::is_approved_status(&r.status)
        }),
        LinkKind::Skill => skills.iter().any(|s| {
            s.name == item.id && s.enabled && ax_policy::matcher::is_approved_status(&s.status)
        }),
        LinkKind::Memory => memory_rows
            .iter()
            .any(|m| m.id == item.id && m.enabled && m.kind != ax_memory::TURN_KIND),
    };
    let linked = follow_links(&sources, &index, &delivered, eligible, LINK_CAP);
    drop(sources);
    let mut policy_changed = false;
    for (target, from) in &linked {
        let reason = format!("link:{}", from.label());
        match target.kind {
            LinkKind::Rule => {
                if let Some(r) = rules.iter().find(|r| r.id == target.id) {
                    result.rules.push(MatchedRule {
                        id: r.id.clone(),
                        level: r.level.clone(),
                        score: 0,
                        reason,
                        always_apply: false,
                        body: r.body.clone(),
                        properties: r.properties.clone(),
                    });
                    policy_changed = true;
                }
            }
            LinkKind::Skill => {
                if let Some(s) = skills.iter().find(|s| s.name == target.id) {
                    result.skills.push(MatchedSkill {
                        name: s.name.clone(),
                        score: 0,
                        reason,
                        description: s.description.clone(),
                        body: s.body.clone(),
                        always_apply: false,
                        properties: s.properties.clone(),
                    });
                    policy_changed = true;
                }
            }
            LinkKind::Memory => {
                if let Some(m) = memory_rows.iter().find(|m| m.id == target.id) {
                    memories.push(MemoryMatch {
                        memory: m.clone(),
                        score: 0.0,
                    });
                }
            }
        }
    }
    if policy_changed {
        result.inject = ax_policy::format_inject_block(
            &result.rules,
            &result.skills,
            ax_policy::max_inject_chars(),
        );
    }
    !linked.is_empty()
}

/// Resolve with the lightweight title catalog, then load only selected bodies.
pub async fn expand_project_links(
    pool: &sqlx::SqlitePool,
    result: &mut MatchResult,
    memories: &mut Vec<MemoryMatch>,
    rules: &[PolicyRuleRow],
    skills: &[PolicySkillRow],
) -> Result<bool, String> {
    let mut catalog = ax_memory::link_catalog(pool)
        .await
        .map_err(|e| e.to_string())?;
    let selected = {
        let sources = sources(result, memories);
        let index = index(rules, skills, &catalog);
        let delivered = sources
            .iter()
            .map(|s| (s.item.kind, s.item.id.clone()))
            .collect();
        follow_links(
            &sources,
            &index,
            &delivered,
            |item| match item.kind {
                LinkKind::Rule => rules.iter().any(|r| {
                    r.id == item.id
                        && r.enabled
                        && ax_policy::matcher::is_approved_status(&r.status)
                }),
                LinkKind::Skill => skills.iter().any(|s| {
                    s.name == item.id
                        && s.enabled
                        && ax_policy::matcher::is_approved_status(&s.status)
                }),
                LinkKind::Memory => catalog
                    .iter()
                    .any(|m| m.id == item.id && m.enabled && m.kind != ax_memory::TURN_KIND),
            },
            LINK_CAP,
        )
    };
    let ids: Vec<_> = selected
        .iter()
        .filter(|(item, _)| item.kind == LinkKind::Memory)
        .map(|(item, _)| item.id.as_str())
        .collect();
    hydrate_catalog(pool, &mut catalog, &ids).await?;
    Ok(expand_links(result, memories, rules, skills, &catalog))
}

async fn hydrate_catalog(
    pool: &sqlx::SqlitePool,
    catalog: &mut [MemoryRow],
    ids: &[&str],
) -> Result<(), String> {
    // Keep title disambiguation stable; unhydrated bodies never become fallback targets.
    for row in catalog.iter_mut() {
        row.enabled = false;
    }
    for id in ids {
        if let Some(row) = ax_memory::get(pool, id).await.map_err(|e| e.to_string())? {
            if let Some(slot) = catalog.iter_mut().find(|m| m.id == *id) {
                *slot = row;
            }
        }
    }
    Ok(())
}

/// Cheap check before loading every rule, skill and memory.
pub fn has_links(result: &MatchResult, memories: &[MemoryMatch]) -> bool {
    sources(result, memories)
        .iter()
        .any(|s| s.body.contains("[["))
}

fn sources<'a>(result: &'a MatchResult, memories: &'a [MemoryMatch]) -> Vec<LinkSource<'a>> {
    let rules = result.rules.iter().map(|r| LinkSource {
        item: LinkItem::new(LinkKind::Rule, LinkOrigin::Project, &r.id, &r.id),
        body: &r.body,
    });
    let skills = result.skills.iter().map(|s| LinkSource {
        item: LinkItem::new(LinkKind::Skill, LinkOrigin::Project, &s.name, &s.name),
        body: &s.body,
    });
    let memories = memories.iter().map(|m| LinkSource {
        item: LinkItem::new(
            LinkKind::Memory,
            LinkOrigin::Project,
            &m.memory.id,
            &m.memory.title,
        ),
        body: &m.memory.body,
    });
    rules.chain(skills).chain(memories).collect()
}

fn origin(scope: &str) -> LinkOrigin {
    if matches!(scope, "company" | "global" | "private_user") {
        LinkOrigin::Global
    } else {
        LinkOrigin::Project
    }
}

fn index(
    rules: &[PolicyRuleRow],
    skills: &[PolicySkillRow],
    memory_rows: &[MemoryRow],
) -> LinkIndex {
    let entries: Vec<(&str, &str, i64)> = memory_rows
        .iter()
        .map(|m| (m.id.as_str(), m.title.as_str(), m.created_at))
        .collect();
    let rules = rules
        .iter()
        .map(|r| LinkItem::new(LinkKind::Rule, origin(&r.scope), &r.id, &r.id));
    let skills = skills
        .iter()
        .map(|s| LinkItem::new(LinkKind::Skill, origin(&s.scope), &s.name, &s.name));
    let memories = ax_policy::links::unique_stems(&entries)
        .into_iter()
        .map(|(id, stem)| LinkItem::new(LinkKind::Memory, LinkOrigin::Project, &id, &stem));
    LinkIndex::new(rules.chain(skills).chain(memories).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rule(id: &str, body: &str, enabled: bool) -> PolicyRuleRow {
        serde_json::from_value(json!({
            "id": id, "level": "WARNING", "alwaysApply": false, "globs": [], "triggers": [],
            "tags": [], "priority": 50, "body": body, "sourcePath": "", "enabled": enabled,
        }))
        .unwrap()
    }

    fn skill(name: &str, body: &str, status: &str) -> PolicySkillRow {
        serde_json::from_value(json!({
            "name": name, "description": format!("{name} skill"), "triggers": [], "tags": [],
            "priority": 50, "body": body, "sourcePath": "", "status": status,
        }))
        .unwrap()
    }

    fn memory(id: &str, title: &str, body: &str, kind: &str) -> MemoryRow {
        MemoryRow {
            id: id.into(),
            kind: kind.into(),
            title: title.into(),
            body: body.into(),
            tags: vec![],
            files: vec![],
            confidence: 1.0,
            source: "manual".into(),
            enabled: true,
            created_at: 1,
            updated_at: 1,
        }
    }

    fn matched_rule(r: &PolicyRuleRow, always: bool) -> MatchedRule {
        MatchedRule {
            id: r.id.clone(),
            level: r.level.clone(),
            score: 100,
            reason: "alwaysApply".into(),
            always_apply: always,
            body: r.body.clone(),
            properties: r.properties.clone(),
        }
    }

    fn matched_skill(s: &PolicySkillRow) -> MatchedSkill {
        MatchedSkill {
            name: s.name.clone(),
            score: 25,
            reason: "trigger:x".into(),
            description: s.description.clone(),
            body: s.body.clone(),
            always_apply: false,
            properties: s.properties.clone(),
        }
    }

    struct World {
        rules: Vec<PolicyRuleRow>,
        skills: Vec<PolicySkillRow>,
        memories: Vec<MemoryRow>,
    }

    fn world() -> World {
        World {
            rules: vec![
                rule(
                    "always-check-pr-builds",
                    "Use skills like [[pre-pr-check]] and [[pr]].",
                    true,
                ),
                rule("disabled-rule", "Off.", false),
                rule(
                    "links-disabled",
                    "See [[rules/disabled-rule]] and [[unapproved]].",
                    true,
                ),
                rule("plain", "No links here.", true),
            ],
            skills: vec![
                skill("pre-pr-check", "Run checks. Then [[tdd]].", "approved"),
                skill("pr", "Open a draft PR.", "approved"),
                skill("tdd", "Red green refactor.", "approved"),
                skill("unapproved", "Pending.", "pending"),
            ],
            memories: vec![
                memory(
                    "m1",
                    "Use SQLite",
                    "SQLite is the source of truth. See [[rules/plain]].",
                    "decision",
                ),
                memory("m2", "Turn log", "turn", "turn"),
            ],
        }
    }

    fn result_with(rules: Vec<MatchedRule>, skills: Vec<MatchedSkill>) -> MatchResult {
        let inject = ax_policy::format_inject_block(&rules, &skills, ax_policy::max_inject_chars());
        MatchResult {
            rules,
            skills,
            inject,
        }
    }

    fn names(result: &MatchResult) -> Vec<String> {
        result
            .rules
            .iter()
            .map(|r| format!("rule/{} ({})", r.id, r.reason))
            .chain(
                result
                    .skills
                    .iter()
                    .map(|s| format!("skill/{} ({})", s.name, s.reason)),
            )
            .collect()
    }

    #[test]
    fn explicit_global_scopes_remain_distinct_from_project_scope() {
        for scope in ["company", "global", "private_user"] {
            assert_eq!(origin(scope), LinkOrigin::Global);
        }
        assert_eq!(origin("project"), LinkOrigin::Project);
    }

    #[tokio::test]
    async fn deletion_between_catalog_and_fetch_cannot_select_an_unhydrated_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let ax = ax_core::Ax::init(dir.path()).await.unwrap();
        let first = ax_memory::remember(
            ax.db_pool(),
            ax_memory::RememberInput {
                title: "First".into(),
                body: "first body".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let second = ax_memory::remember(
            ax.db_pool(),
            ax_memory::RememberInput {
                title: "Second".into(),
                body: "second body".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let mut catalog = ax_memory::link_catalog(ax.db_pool()).await.unwrap();
        sqlx::query("DELETE FROM memories WHERE id = ?")
            .bind(&first.id)
            .execute(ax.db_pool())
            .await
            .unwrap();
        hydrate_catalog(ax.db_pool(), &mut catalog, &[&first.id])
            .await
            .unwrap();
        assert!(catalog.iter().all(|m| !m.enabled));
        assert!(catalog
            .iter()
            .any(|m| m.id == second.id && m.body.is_empty()));
    }

    #[test]
    fn p1_matched_rule_delivers_linked_skills() {
        let w = world();
        let mut result = result_with(vec![matched_rule(&w.rules[0], true)], vec![]);
        let mut memories = vec![];
        assert!(expand_links(
            &mut result,
            &mut memories,
            &w.rules,
            &w.skills,
            &w.memories
        ));
        assert_eq!(
            names(&result),
            vec![
                "rule/always-check-pr-builds (alwaysApply)",
                "skill/pre-pr-check (link:rule/always-check-pr-builds)",
                "skill/pr (link:rule/always-check-pr-builds)",
            ]
        );
        assert!(result.inject.contains("Run checks."), "{}", result.inject);
        assert!(result.inject.contains("Open a draft PR."));
        assert!(!result.skills[0].always_apply);
    }

    #[test]
    fn p2_already_matched_is_not_delivered_twice() {
        let w = world();
        let mut result = result_with(
            vec![matched_rule(&w.rules[0], true)],
            vec![matched_skill(&w.skills[1])],
        );
        let mut memories = vec![];
        expand_links(&mut result, &mut memories, &w.rules, &w.skills, &w.memories);
        let prs = result.skills.iter().filter(|s| s.name == "pr").count();
        assert_eq!(prs, 1);
        assert_eq!(result.skills.len(), 2);
    }

    #[test]
    fn p3_links_inside_linked_items_are_not_followed() {
        let w = world();
        let mut result = result_with(vec![matched_rule(&w.rules[0], true)], vec![]);
        let mut memories = vec![];
        expand_links(&mut result, &mut memories, &w.rules, &w.skills, &w.memories);
        assert!(
            result.skills.iter().all(|s| s.name != "tdd"),
            "{:?}",
            names(&result)
        );
    }

    #[test]
    fn p4_at_most_five_linked_items() {
        let mut w = world();
        let many: Vec<String> = (0..8).map(|i| format!("[[s{i}]]")).collect();
        w.rules.push(rule("hub", &many.join(" "), true));
        for i in 0..8 {
            w.skills.push(skill(&format!("s{i}"), "x", "approved"));
        }
        let hub = w.rules.last().unwrap().clone();
        let mut result = result_with(vec![matched_rule(&hub, false)], vec![]);
        let mut memories = vec![];
        expand_links(&mut result, &mut memories, &w.rules, &w.skills, &w.memories);
        let linked: Vec<&str> = result.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(linked, vec!["s0", "s1", "s2", "s3", "s4"]);
    }

    #[test]
    fn p5_disabled_or_unapproved_targets_are_never_delivered() {
        let w = world();
        let mut result = result_with(vec![matched_rule(&w.rules[2], false)], vec![]);
        let mut memories = vec![];
        assert!(!expand_links(
            &mut result,
            &mut memories,
            &w.rules,
            &w.skills,
            &w.memories
        ));
        assert_eq!(names(&result), vec!["rule/links-disabled (alwaysApply)"]);
    }

    #[test]
    fn p6_memories_link_both_ways() {
        let mut w = world();
        w.rules.push(rule(
            "remember",
            "Background: [[Use SQLite]] and [[Turn log]].",
            true,
        ));
        let src = w.rules.last().unwrap().clone();
        let mut result = result_with(vec![matched_rule(&src, false)], vec![]);
        let mut memories = vec![];
        expand_links(&mut result, &mut memories, &w.rules, &w.skills, &w.memories);
        let titles: Vec<&str> = memories.iter().map(|m| m.memory.title.as_str()).collect();
        assert_eq!(
            titles,
            vec!["Use SQLite"],
            "turn memories are never delivered"
        );

        let mut result = result_with(vec![], vec![]);
        let mut memories = vec![MemoryMatch {
            memory: w.memories[0].clone(),
            score: 1.0,
        }];
        expand_links(&mut result, &mut memories, &w.rules, &w.skills, &w.memories);
        assert_eq!(names(&result), vec!["rule/plain (link:memory/m1)"]);
        assert_eq!(memories.len(), 1);
    }

    #[test]
    fn p7_and_p8_nothing_to_follow_changes_nothing() {
        let w = world();
        for body in ["[[missing-thing]] only", "No links here."] {
            let mut r = w.rules[3].clone();
            r.body = body.into();
            let before = result_with(vec![matched_rule(&r, true)], vec![]);
            let mut result = before.clone();
            let mut memories = vec![];
            assert!(!expand_links(
                &mut result,
                &mut memories,
                &w.rules,
                &w.skills,
                &w.memories
            ));
            assert_eq!(result.inject, before.inject);
            assert_eq!(names(&result), names(&before));
            assert!(memories.is_empty());
        }
    }
}
