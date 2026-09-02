//! Canonical skill group catalog and resolution for Command Center.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

const CATALOG_JSON: &str = include_str!("../data/skill-groups.json");

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillGroupDef {
    pub id: String,
    pub label: String,
    pub order: i32,
    #[serde(default)]
    pub aliases: Vec<String>,
}

fn parsed_catalog() -> &'static Vec<SkillGroupDef> {
    static CATALOG: OnceLock<Vec<SkillGroupDef>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut groups: Vec<SkillGroupDef> =
            serde_json::from_str(CATALOG_JSON).expect("skill-groups.json must parse");
        groups.sort_by_key(|g| g.order);
        groups
    })
}

/// Full catalog in display order, including groups with zero skills.
pub fn catalog() -> &'static [SkillGroupDef] {
    parsed_catalog()
}

pub fn catalog_json() -> serde_json::Value {
    serde_json::to_value(catalog()).unwrap_or(serde_json::json!([]))
}

/// Resolve a skill to a catalog id. Explicit catalog ids win; unknown ids become `ungrouped`.
pub fn resolve_skill_group(explicit: Option<&str>, name: &str, tags: &[String]) -> String {
    let groups = catalog();
    if let Some(id) = explicit.map(str::trim).filter(|s| !s.is_empty()) {
        if groups.iter().any(|g| g.id == id) {
            return id.to_string();
        }
        return "ungrouped".into();
    }
    let name_l = name.trim().to_ascii_lowercase();
    let tag_l: Vec<String> = tags
        .iter()
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect();
    for g in groups {
        if g.id == "ungrouped" {
            continue;
        }
        for alias in &g.aliases {
            let al = alias.to_ascii_lowercase();
            if name_l == al || tag_l.iter().any(|t| t == &al) {
                return g.id.clone();
            }
        }
    }
    "ungrouped".into()
}

/// Catalog ids that have at least one resolved skill, in catalog order.
pub fn visible_group_ids(resolved_groups: &[String]) -> Vec<String> {
    catalog()
        .iter()
        .filter(|g| resolved_groups.iter().any(|id| id == &g.id))
        .map(|g| g.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_fixed_order() {
        let ids: Vec<&str> = catalog().iter().map(|g| g.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "session-protocol",
                "agent-orchestration",
                "exploration",
                "architecture",
                "design",
                "implementation-quality",
                "apis-contracts",
                "testing",
                "debugging",
                "refactoring",
                "security",
                "performance",
                "data-persistence",
                "conventions",
                "documentation",
                "code-review",
                "pull-requests",
                "git",
                "cicd",
                "release",
                "deploy-ops",
                "observability",
                "project-management",
                "product",
                "communication",
                "integrations",
                "tooling",
                "ungrouped",
            ]
        );
        assert!(catalog().iter().any(|g| g.id == "security"));
        assert!(catalog().iter().any(|g| g.id == "performance"));
        assert!(catalog().iter().any(|g| g.id == "refactoring"));
    }

    #[test]
    fn empty_groups_are_omitted_from_visible_list() {
        let visible = visible_group_ids(&[
            "session-protocol".into(),
            "testing".into(),
            "session-protocol".into(),
        ]);
        assert_eq!(visible, vec!["session-protocol", "testing"]);
        assert!(!visible.contains(&"security".to_string()));
        assert!(!visible.contains(&"ungrouped".to_string()));
    }

    #[test]
    fn empty_groups_remain_in_catalog_for_editor() {
        assert!(catalog().iter().any(|g| g.id == "security" && g.label == "Security"));
    }

    #[test]
    fn explicit_group_wins_over_aliases() {
        let id = resolve_skill_group(
            Some("testing"),
            "startup",
            &["preflight".into()],
        );
        assert_eq!(id, "testing");
    }

    #[test]
    fn legacy_skill_without_group_uses_aliases() {
        let id = resolve_skill_group(
            None,
            "old-coder",
            &["old-coder".into(), "evidence".into()],
        );
        assert_eq!(id, "implementation-quality");
    }

    #[test]
    fn no_group_and_no_alias_is_ungrouped() {
        let id = resolve_skill_group(None, "mystery", &["zzz".into()]);
        assert_eq!(id, "ungrouped");
    }

    #[test]
    fn ungrouped_node_hidden_when_unused() {
        let visible = visible_group_ids(&["testing".into()]);
        assert!(!visible.iter().any(|id| id == "ungrouped"));
    }

    #[test]
    fn invalid_group_id_resolves_to_ungrouped() {
        let id = resolve_skill_group(Some("not-a-real-id"), "mystery", &[]);
        assert_eq!(id, "ungrouped");
    }

    #[test]
    fn seeded_rule_ids_resolve_via_aliases() {
        assert_eq!(
            resolve_skill_group(None, "explore-before-grep", &[]),
            "exploration"
        );
        assert_eq!(
            resolve_skill_group(None, "old-coder-mandatory", &[]),
            "implementation-quality"
        );
        assert_eq!(
            resolve_skill_group(None, "utf8-no-bom", &[]),
            "conventions"
        );
        assert_eq!(resolve_skill_group(None, "mcp-first", &[]), "tooling");
        assert_eq!(
            resolve_skill_group(None, "docs-with-features", &[]),
            "documentation"
        );
    }

    #[test]
    fn first_catalog_alias_in_order_wins() {
        let id = resolve_skill_group(
            None,
            "azdo-pipelines",
            &["azdo".into(), "cicd".into()],
        );
        assert_eq!(id, "cicd");
    }

    #[test]
    fn skill_row_json_keeps_name_and_tags() {
        use crate::types::PolicySkillRow;
        let row = PolicySkillRow {
            name: "startup".into(),
            description: "preflight".into(),
            always_apply: false,
            triggers: vec![],
            tags: vec!["preflight".into()],
            priority: 50,
            context_task: None,
            body: String::new(),
            source_path: String::new(),
            enabled: true,
            status: "approved".into(),
            scope: "project".into(),
            storage: None,
            source: None,
            root_id: None,
            stub_path: None,
            effective_storage: String::new(),
            storage_is_override: false,
            group: "session-protocol".into(),
        };
        let v = serde_json::to_value(&row).unwrap();
        assert_eq!(v["name"], "startup");
        assert_eq!(v["tags"][0], "preflight");
        assert_eq!(v["group"], "session-protocol");
    }
}
