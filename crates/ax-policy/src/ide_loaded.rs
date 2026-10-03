//! Which always-apply rules and skills an IDE already loads by itself.
//!
//! Cursor puts `.cursor/rules/*.mdc` and `.cursor/skills/*/SKILL.md` with
//! `alwaysApply: true` into every chat. When that file carries the same body
//! ax would inject, sending it again only costs tokens.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::format::{rule_key, skill_key};
use crate::parse::split_frontmatter;
use crate::types::{MatchedRule, MatchedSkill};

/// Keys (`rule:<id>` / `skill:<name>`) whose identical body the IDE loads itself.
/// `roots` are searched in order, usually the project root and the home directory.
pub fn ide_loaded_keys(
    roots: &[PathBuf],
    rules: &[MatchedRule],
    skills: &[MatchedSkill],
) -> HashSet<String> {
    let mut keys = HashSet::new();
    for r in rules.iter().filter(|r| r.always_apply) {
        let found = roots.iter().any(|root| {
            file_matches(
                &root.join(".cursor/rules").join(format!("{}.mdc", r.id)),
                &r.body,
            )
        });
        if found {
            keys.insert(rule_key(&r.id));
        }
    }
    for s in skills.iter().filter(|s| s.always_apply) {
        let found = roots.iter().any(|root| {
            file_matches(
                &root.join(".cursor/skills").join(&s.name).join("SKILL.md"),
                &s.body,
            )
        });
        if found {
            keys.insert(skill_key(&s.name));
        }
    }
    keys
}

fn file_matches(path: &Path, body: &str) -> bool {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    let Ok((frontmatter, file_body)) = split_frontmatter(&raw) else {
        return false;
    };
    let always = frontmatter
        .lines()
        .any(|line| line.trim().replace(' ', "") == "alwaysApply:true");
    always && file_body.trim() == body.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(id: &str, body: &str) -> MatchedRule {
        MatchedRule {
            id: id.into(),
            level: "CRITICAL".into(),
            body: body.into(),
            score: 0,
            reason: "always".into(),
            always_apply: true,
            properties: Default::default(),
        }
    }

    fn skill(name: &str, body: &str) -> MatchedSkill {
        MatchedSkill {
            name: name.into(),
            description: "d".into(),
            body: body.into(),
            score: 0,
            reason: "always".into(),
            always_apply: true,
            properties: Default::default(),
        }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn identical_always_apply_rule_is_loaded() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join(".cursor/rules/ax.mdc"),
            "---\nalwaysApply: true\n---\n\n# ax\n\nCall preflight.\n",
        );
        let keys = ide_loaded_keys(
            &[dir.path().to_path_buf()],
            &[rule("ax", "# ax\n\nCall preflight.")],
            &[],
        );
        assert!(keys.contains("rule:ax"), "{keys:?}");
    }

    #[test]
    fn different_body_is_not_loaded() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join(".cursor/rules/ax.mdc"),
            "---\nalwaysApply: true\n---\n# ax\n\nOld text.\n",
        );
        let keys = ide_loaded_keys(
            &[dir.path().to_path_buf()],
            &[rule("ax", "# ax\n\nNew text.")],
            &[],
        );
        assert!(keys.is_empty(), "{keys:?}");
    }

    #[test]
    fn rule_without_always_apply_is_not_loaded() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join(".cursor/rules/ax.mdc"),
            "---\nalwaysApply: false\n---\nSame.\n",
        );
        let keys = ide_loaded_keys(&[dir.path().to_path_buf()], &[rule("ax", "Same.")], &[]);
        assert!(keys.is_empty(), "{keys:?}");
    }

    #[test]
    fn skill_found_in_second_root() {
        let project = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        write(
            &home.path().join(".cursor/skills/old-coder/SKILL.md"),
            "---\nname: old-coder\nalwaysApply: true\n---\n\n# Old Coder\n\nBody.\n",
        );
        let keys = ide_loaded_keys(
            &[project.path().to_path_buf(), home.path().to_path_buf()],
            &[],
            &[skill("old-coder", "# Old Coder\n\nBody.")],
        );
        assert_eq!(keys, HashSet::from([skill_key("old-coder")]));
    }

    #[test]
    fn contextual_rule_is_never_counted() {
        let dir = tempfile::tempdir().unwrap();
        write(
            &dir.path().join(".cursor/rules/x.mdc"),
            "---\nalwaysApply: true\n---\nSame.\n",
        );
        let mut r = rule("x", "Same.");
        r.always_apply = false;
        let keys = ide_loaded_keys(&[dir.path().to_path_buf()], &[r], &[]);
        assert!(keys.is_empty(), "{keys:?}");
    }
}
