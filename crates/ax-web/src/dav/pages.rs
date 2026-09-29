//! Vault paths and page text ↔ policy/memory rows. No IO.

use std::path::Path;

use ax_memory::{MemoryRow, RememberInput};
use ax_policy::{
    parse_rule_file, parse_skill_file, serialize_rule, serialize_skill, PolicyRuleDoc,
    PolicySkillDoc, RuleFrontmatter, SkillFrontmatter, ValidationError,
};
use serde::{Deserialize, Serialize};

pub const DRAFTS_PAGE: &str = "DRAFTS.md";
pub const GLOBAL_DIR: &str = "global";
pub const APP_JSON: &str = ".obsidian/app.json";
pub const FOLDERS_DIR: &str = crate::vault_folders::FOLDERS_DIR;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    Rules,
    Skills,
    Memories,
    GlobalRules,
    GlobalSkills,
}

impl Area {
    pub const ALL: [Area; 5] = [
        Area::Rules,
        Area::Skills,
        Area::Memories,
        Area::GlobalRules,
        Area::GlobalSkills,
    ];
    /// The folders at the vault root.
    pub const TOP: [Area; 3] = [Area::Rules, Area::Skills, Area::Memories];
    pub const GLOBAL: [Area; 2] = [Area::GlobalRules, Area::GlobalSkills];

    pub fn dir(self) -> &'static str {
        match self {
            Area::Rules => "rules",
            Area::Skills => "skills",
            Area::Memories => "memories",
            Area::GlobalRules => "global/rules",
            Area::GlobalSkills => "global/skills",
        }
    }

    pub fn is_global(self) -> bool {
        matches!(self, Area::GlobalRules | Area::GlobalSkills)
    }
}

/// What a vault path refers to. `path` is relative, without leading or trailing `/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Root,
    /// `global/`, holding `global/rules/` and `global/skills/`.
    GlobalDir,
    AreaDir(Area),
    Page(Area, String),
    Drafts,
    /// `folders/`, holding one directory per configured vault folder.
    FoldersDir,
    /// `folders/<name>/<rest>`: a file or directory on disk. `rest` is empty for the folder itself.
    Folder(String, String),
    /// macOS metadata (`._*`, `.DS_Store`): accepted and discarded.
    Junk,
    /// Anything else (`.obsidian/**`, attachments): bytes stored in `dav_files`.
    Stored(String),
}

pub fn classify(path: &str) -> Target {
    let path = path.trim_matches('/');
    if path.is_empty() {
        return Target::Root;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.starts_with("._") || name == ".DS_Store" {
        return Target::Junk;
    }
    if path == DRAFTS_PAGE {
        return Target::Drafts;
    }
    if path == GLOBAL_DIR {
        return Target::GlobalDir;
    }
    if path == FOLDERS_DIR {
        return Target::FoldersDir;
    }
    if let Some(rest) = path
        .strip_prefix(FOLDERS_DIR)
        .and_then(|r| r.strip_prefix('/'))
    {
        let (name, rest) = rest.split_once('/').unwrap_or((rest, ""));
        return Target::Folder(name.to_string(), rest.to_string());
    }
    for area in Area::ALL {
        if path == area.dir() {
            return Target::AreaDir(area);
        }
        let Some(rest) = path
            .strip_prefix(area.dir())
            .and_then(|r| r.strip_prefix('/'))
        else {
            continue;
        };
        if !rest.contains('/') && rest.len() > 3 && rest.ends_with(".md") {
            return Target::Page(area, rest[..rest.len() - 3].to_string());
        }
        break;
    }
    Target::Stored(path.to_string())
}

/// Obsidian's settings as served. Missing keys get defaults that file new notes under
/// `memories/`; a key the user set is never changed, and anything that is not a JSON
/// object is served as stored.
pub fn app_json(stored: Option<&[u8]>) -> Vec<u8> {
    let defaults = [
        ("newFileLocation", serde_json::json!("folder")),
        ("newFileFolderPath", serde_json::json!("memories")),
        ("alwaysUpdateLinks", serde_json::json!(true)),
    ];
    let Some(bytes) = stored else {
        let obj: serde_json::Map<String, serde_json::Value> = defaults
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        return serde_json::to_vec_pretty(&obj).unwrap_or_default();
    };
    let Ok(serde_json::Value::Object(mut obj)) = serde_json::from_slice(bytes) else {
        return bytes.to_vec();
    };
    let mut added = false;
    for (key, value) in defaults {
        if !obj.contains_key(key) {
            obj.insert(key.to_string(), value);
            added = true;
        }
    }
    if !added {
        return bytes.to_vec();
    }
    serde_json::to_vec_pretty(&obj).unwrap_or_else(|_| bytes.to_vec())
}

pub fn page_path(area: Area, stem: &str) -> String {
    format!("{}/{stem}.md", area.dir())
}

fn has_frontmatter(text: &str) -> bool {
    text.trim_start().starts_with("---")
}

fn describe(v: &ValidationError) -> String {
    if v.fields.is_empty() {
        return v.error.clone();
    }
    let mut fields: Vec<_> = v.fields.iter().map(|(k, m)| format!("{k}: {m}")).collect();
    fields.sort();
    format!("{} ({})", v.error, fields.join("; "))
}

pub fn rule_page(doc: &PolicyRuleDoc) -> String {
    serialize_rule(&doc.frontmatter, &doc.body)
}

pub fn skill_page(doc: &PolicySkillDoc) -> String {
    serialize_skill(&doc.frontmatter, &doc.body)
}

/// Turn a written rule page into a save. `Err` is the draft reason.
pub fn rule_from_page(
    stem: &str,
    text: &str,
    existing: Option<&PolicyRuleDoc>,
) -> Result<(RuleFrontmatter, String), String> {
    if text.trim().is_empty() {
        return Err("empty page".into());
    }
    let path = Path::new("page.md");
    if has_frontmatter(text) {
        let doc = parse_rule_file(path, text).map_err(|v| describe(&v))?;
        if doc.frontmatter.id != stem {
            return Err(format!(
                "id does not match file name (id: {}, file: {stem}.md)",
                doc.frontmatter.id
            ));
        }
        return Ok((doc.frontmatter, doc.body));
    }
    if let Some(doc) = existing {
        return Ok((doc.frontmatter.clone(), text.trim().to_string()));
    }
    let raw = format!("---\nid: {stem}\nlevel: INFO\ntriggers: [\"{stem}\"]\n---\n\n{text}");
    let doc = parse_rule_file(path, &raw).map_err(|v| describe(&v))?;
    Ok((doc.frontmatter, doc.body))
}

/// Turn a written skill page into a save. `Err` is the draft reason.
pub fn skill_from_page(
    stem: &str,
    text: &str,
    existing: Option<&PolicySkillDoc>,
) -> Result<(SkillFrontmatter, String), String> {
    if text.trim().is_empty() {
        return Err("empty page".into());
    }
    let path = Path::new("SKILL.md");
    if has_frontmatter(text) {
        let doc = parse_skill_file(path, text).map_err(|v| describe(&v))?;
        if doc.frontmatter.name != stem {
            return Err(format!(
                "name does not match file name (name: {}, file: {stem}.md)",
                doc.frontmatter.name
            ));
        }
        return Ok((doc.frontmatter, doc.body));
    }
    if let Some(doc) = existing {
        return Ok((doc.frontmatter.clone(), text.trim().to_string()));
    }
    let raw = format!("---\nname: {stem}\ndescription: {stem}\n---\n\n{text}");
    let doc = parse_skill_file(path, &raw).map_err(|v| describe(&v))?;
    Ok((doc.frontmatter, doc.body))
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct MemoryFrontmatter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    files: Option<Vec<String>>,
}

pub fn memory_page(row: &MemoryRow) -> String {
    let fm = MemoryFrontmatter {
        id: Some(row.id.clone()),
        kind: Some(row.kind.clone()),
        tags: Some(row.tags.clone()),
        files: Some(row.files.clone()),
    };
    let yaml = serde_yaml::to_string(&fm).unwrap_or_default();
    format!("---\n{}---\n\n{}", yaml, row.body.trim())
}

/// A parsed memory page. `None` fields were absent and keep the stored value.
#[derive(Debug)]
pub struct MemoryDraft {
    pub id: Option<String>,
    pub title: String,
    pub body: String,
    pub kind: Option<String>,
    pub tags: Option<Vec<String>>,
    pub files: Option<Vec<String>>,
}

impl MemoryDraft {
    pub fn into_input(self, existing: Option<&MemoryRow>) -> RememberInput {
        RememberInput {
            title: self.title,
            body: self.body,
            kind: self.kind.or_else(|| existing.map(|m| m.kind.clone())),
            tags: self
                .tags
                .or_else(|| existing.map(|m| m.tags.clone()))
                .unwrap_or_default(),
            files: self
                .files
                .or_else(|| existing.map(|m| m.files.clone()))
                .unwrap_or_default(),
            source: Some("obsidian".into()),
        }
    }
}

pub fn memory_from_page(stem: &str, text: &str) -> Result<MemoryDraft, String> {
    let (fm, body) = if has_frontmatter(text) {
        let (yaml, body) = ax_policy::split_frontmatter(text).map_err(|v| describe(&v))?;
        let fm: MemoryFrontmatter = if yaml.trim().is_empty() {
            MemoryFrontmatter::default()
        } else {
            serde_yaml::from_str(&yaml).map_err(|e| format!("frontmatter: {e}"))?
        };
        (fm, body)
    } else {
        (MemoryFrontmatter::default(), text.trim().to_string())
    };
    if body.trim().is_empty() {
        return Err("empty page".into());
    }
    Ok(MemoryDraft {
        id: fm.id.filter(|s| !s.trim().is_empty()),
        title: stem.to_string(),
        body,
        kind: fm.kind,
        tags: fm.tags,
        files: fm.files,
    })
}

/// File stem per memory id. Clashing titles get ` (2)`, ` (3)` in creation order.
pub fn memory_stems(rows: &[MemoryRow]) -> Vec<(String, String)> {
    let entries: Vec<(&str, &str, i64)> = rows
        .iter()
        .map(|r| (r.id.as_str(), r.title.as_str(), r.created_at))
        .collect();
    ax_policy::links::unique_stems(&entries)
}

pub fn drafts_page(drafts: &[(String, String)]) -> String {
    let mut out = String::from(
        "# Drafts\n\nPages that were not saved to ax. Fix the page and save it again.\n\n",
    );
    if drafts.is_empty() {
        out.push_str("No drafts.\n");
    }
    for (path, reason) in drafts {
        let link = path.trim_end_matches(".md");
        out.push_str(&format!("- [[{link}]]: {reason}\n"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn rule(id: &str, body: &str) -> PolicyRuleDoc {
        let raw = format!("---\nid: {id}\nlevel: WARNING\nalwaysApply: true\n---\n\n{body}");
        parse_rule_file(Path::new("x.mdc"), &raw).unwrap()
    }

    #[test]
    fn classify_paths() {
        assert_eq!(classify(""), Target::Root);
        assert_eq!(classify("rules"), Target::AreaDir(Area::Rules));
        assert_eq!(
            classify("rules/a-b.md"),
            Target::Page(Area::Rules, "a-b".into())
        );
        assert_eq!(
            classify("memories/Some idea.md"),
            Target::Page(Area::Memories, "Some idea".into())
        );
        assert_eq!(classify("DRAFTS.md"), Target::Drafts);
        assert_eq!(classify("rules/._a.md"), Target::Junk);
        assert_eq!(classify(".DS_Store"), Target::Junk);
        assert_eq!(
            classify(".obsidian/app.json"),
            Target::Stored(".obsidian/app.json".into())
        );
        assert_eq!(
            classify("rules/img.png"),
            Target::Stored("rules/img.png".into())
        );
        assert_eq!(
            classify("rules/sub/a.md"),
            Target::Stored("rules/sub/a.md".into())
        );
        assert_eq!(classify("rules/.md"), Target::Stored("rules/.md".into()));
        assert_eq!(classify("global"), Target::GlobalDir);
        assert_eq!(classify("global/"), Target::GlobalDir);
        assert_eq!(classify("folders"), Target::FoldersDir);
        assert_eq!(
            classify("folders/notes"),
            Target::Folder("notes".into(), String::new())
        );
        assert_eq!(
            classify("folders/notes/sub/a.md"),
            Target::Folder("notes".into(), "sub/a.md".into())
        );
        assert_eq!(
            classify("global/skills"),
            Target::AreaDir(Area::GlobalSkills)
        );
        assert_eq!(
            classify("global/rules/a-b.md"),
            Target::Page(Area::GlobalRules, "a-b".into())
        );
        assert_eq!(
            classify("global/other.md"),
            Target::Stored("global/other.md".into())
        );
        assert_eq!(
            classify("rulesx/a.md"),
            Target::Stored("rulesx/a.md".into())
        );
    }

    #[test]
    fn rule_page_round_trips() {
        let doc = rule("english-only", "Write English.");
        let (fm, body) = rule_from_page("english-only", &rule_page(&doc), None).unwrap();
        assert_eq!(fm.id, "english-only");
        assert_eq!(fm.level, "WARNING");
        assert_eq!(body, "Write English.");
    }

    #[test]
    fn rule_body_only_keeps_existing_frontmatter() {
        let doc = rule("english-only", "old");
        let (fm, body) = rule_from_page("english-only", "new body\n", Some(&doc)).unwrap();
        assert_eq!(fm.level, "WARNING");
        assert!(fm.always_apply);
        assert_eq!(body, "new body");
    }

    #[test]
    fn rule_id_mismatch_is_rejected() {
        let other = rule_page(&rule("other-rule", "x"));
        let err = rule_from_page("english-only", &other, None).unwrap_err();
        assert!(err.contains("id does not match file name"), "{err}");
    }

    #[test]
    fn rule_invalid_frontmatter_is_rejected() {
        let err = rule_from_page(
            "a",
            "---\nid: a\nlevel: LOUD\nalwaysApply: true\n---\nx",
            None,
        )
        .unwrap_err();
        assert!(err.contains("level"), "{err}");
    }

    #[test]
    fn new_rule_from_body_only() {
        let (fm, body) = rule_from_page("new-rule", "Do the thing.", None).unwrap();
        assert_eq!(fm.id, "new-rule");
        assert_eq!(fm.level, "INFO");
        assert_eq!(fm.triggers, vec!["new-rule".to_string()]);
        assert_eq!(body, "Do the thing.");
    }

    #[test]
    fn empty_or_bad_name_is_rejected() {
        assert_eq!(rule_from_page("x", "  \n", None).unwrap_err(), "empty page");
        assert!(rule_from_page("Untitled", "text", None)
            .unwrap_err()
            .contains("kebab-case"));
    }

    #[test]
    fn skill_rules_match_rule_rules() {
        let (fm, body) = skill_from_page("deploy", "Steps.", None).unwrap();
        assert_eq!(
            (fm.name.as_str(), fm.description.as_str(), body.as_str()),
            ("deploy", "deploy", "Steps.")
        );
        let page = skill_page(
            &parse_skill_file(
                Path::new("SKILL.md"),
                "---\nname: other\ndescription: d\n---\n\nb",
            )
            .unwrap(),
        );
        assert!(skill_from_page("deploy", &page, None)
            .unwrap_err()
            .contains("name does not match"));
    }

    fn memory(id: &str, title: &str, created_at: i64) -> MemoryRow {
        MemoryRow {
            id: id.into(),
            kind: "decision".into(),
            title: title.into(),
            body: "Body text".into(),
            tags: vec!["a".into()],
            files: vec!["src/x.rs".into()],
            confidence: 1.0,
            source: "manual".into(),
            enabled: true,
            created_at,
            updated_at: created_at,
        }
    }

    #[test]
    fn memory_page_round_trips() {
        let row = memory("m1", "Title", 1);
        let draft = memory_from_page("Title", &memory_page(&row)).unwrap();
        assert_eq!(draft.id.as_deref(), Some("m1"));
        let input = draft.into_input(None);
        assert_eq!(input.kind.as_deref(), Some("decision"));
        assert_eq!(input.tags, vec!["a".to_string()]);
        assert_eq!(input.files, vec!["src/x.rs".to_string()]);
        assert_eq!(input.body, "Body text");
        assert_eq!(input.title, "Title");
    }

    #[test]
    fn memory_without_frontmatter_keeps_stored_fields() {
        let draft = memory_from_page("Some idea", "just text").unwrap();
        assert!(draft.id.is_none());
        let input = draft.into_input(Some(&memory("m1", "Some idea", 1)));
        assert_eq!(input.body, "just text");
        assert_eq!(input.kind.as_deref(), Some("decision"));
        assert_eq!(input.tags, vec!["a".to_string()]);
        assert_eq!(input.files, vec!["src/x.rs".to_string()]);
        assert!(memory_from_page("x", "---\nid: m1\n---\n")
            .unwrap_err()
            .contains("empty"));
    }

    #[test]
    fn memory_stems_are_safe_and_unique() {
        let rows = vec![
            memory("b", "a/b: c?", 2),
            memory("a", "a/b: c?", 1),
            memory("c", "", 3),
        ];
        let stems: HashMap<_, _> = memory_stems(&rows).into_iter().collect();
        assert_eq!(stems["a"], "a-b- c-");
        assert_eq!(stems["b"], "a-b- c- (2)");
        assert_eq!(stems["c"], "untitled");
    }
}
