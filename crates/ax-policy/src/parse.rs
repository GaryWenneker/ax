use std::collections::HashMap;
use std::path::Path;

use serde_yaml::Value;

use crate::types::{
    PolicyRuleDoc, PolicySkillDoc, RuleFrontmatter, SkillFrontmatter, ValidationError,
};

pub fn split_frontmatter(raw: &str) -> Result<(String, String), ValidationError> {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return Err(field_err("body", "document must start with YAML frontmatter (---)"));
    }
    let rest = trimmed.trim_start_matches("---").trim_start();
    let end = rest.find("\n---").ok_or_else(|| field_err("body", "missing closing ---"))?;
    let yaml = rest[..end].trim();
    let body = rest[end + 4..].trim_start().trim_end().to_string();
    Ok((yaml.to_string(), body))
}

pub fn parse_rule_file(path: &Path, raw: &str) -> Result<PolicyRuleDoc, ValidationError> {
    let (yaml, body) = split_frontmatter(raw)?;
    let fm: RuleFrontmatter = parse_rule_frontmatter(&yaml)?;
    validate_rule(&fm)?;
    Ok(PolicyRuleDoc {
        frontmatter: fm,
        body,
        raw: raw.to_string(),
        source_path: path.to_string_lossy().to_string(),
        stub_path: None,
    })
}

pub fn parse_skill_file(path: &Path, raw: &str) -> Result<PolicySkillDoc, ValidationError> {
    let (yaml, body) = split_frontmatter(raw)?;
    let fm: SkillFrontmatter = parse_skill_frontmatter(&yaml)?;
    validate_skill(&fm)?;
    Ok(PolicySkillDoc {
        frontmatter: fm,
        body,
        raw: raw.to_string(),
        source_path: path.to_string_lossy().to_string(),
        stub_path: None,
    })
}

fn parse_rule_frontmatter(yaml: &str) -> Result<RuleFrontmatter, ValidationError> {
    let v: Value = serde_yaml::from_str(yaml).map_err(|e| field_err("frontmatter", &e.to_string()))?;
    let mut map = HashMap::new();
    if let Value::Mapping(m) = v {
        for (k, val) in m {
            if let Some(key) = k.as_str() {
                map.insert(key.to_string(), val);
            }
        }
    }
    let id = get_str(&map, "id").ok_or_else(|| field_err("id", "required"))?;
    let level = get_str(&map, "level").ok_or_else(|| field_err("level", "required"))?;
    let share = get_bool(&map, "share");
    let mut tags = get_str_list(&map, "tags");
    ensure_shared_tag(&mut tags, share);
    let status = get_str(&map, "status").unwrap_or_else(|| "approved".into());
    let status = crate::types::PolicyItemStatus::parse(&status)
        .unwrap_or(crate::types::PolicyItemStatus::Approved)
        .as_str()
        .to_string();
    let scope = get_str(&map, "scope").unwrap_or_else(|| "project".into());
    let scope = crate::types::PolicyScope::parse(&scope)
        .unwrap_or(crate::types::PolicyScope::Project)
        .as_str()
        .to_string();
    let storage = get_str(&map, "storage").and_then(|s| {
        crate::config::PolicyStorage::parse(&s).map(|p| p.as_str().to_string())
    });
    Ok(RuleFrontmatter {
        id,
        level,
        always_apply: get_bool(&map, "alwaysApply"),
        globs: get_str_list(&map, "globs"),
        triggers: get_str_list(&map, "triggers"),
        tags,
        priority: get_i32(&map, "priority").unwrap_or(50),
        enabled: get_bool_default_true(&map, "enabled"),
        status,
        share,
        scope,
        storage,
        source: get_str(&map, "source").filter(|s| !s.is_empty()),
        root_id: get_str(&map, "rootId")
            .or_else(|| get_str(&map, "root_id"))
            .filter(|s| !s.is_empty()),
    })
}

fn parse_skill_frontmatter(yaml: &str) -> Result<SkillFrontmatter, ValidationError> {
    let v: Value = serde_yaml::from_str(yaml).map_err(|e| field_err("frontmatter", &e.to_string()))?;
    let mut map = HashMap::new();
    if let Value::Mapping(m) = v {
        for (k, val) in m {
            if let Some(key) = k.as_str() {
                map.insert(key.to_string(), val);
            }
        }
    }
    let name = get_str(&map, "name").ok_or_else(|| field_err("name", "required"))?;
    let description = get_str(&map, "description").ok_or_else(|| field_err("description", "required"))?;
    let share = get_bool(&map, "share");
    let mut tags = get_str_list(&map, "tags");
    ensure_shared_tag(&mut tags, share);
    let status = get_str(&map, "status").unwrap_or_else(|| "approved".into());
    let status = crate::types::PolicyItemStatus::parse(&status)
        .unwrap_or(crate::types::PolicyItemStatus::Approved)
        .as_str()
        .to_string();
    let scope = get_str(&map, "scope").unwrap_or_else(|| "project".into());
    let scope = crate::types::PolicyScope::parse(&scope)
        .unwrap_or(crate::types::PolicyScope::Project)
        .as_str()
        .to_string();
    let storage = get_str(&map, "storage").and_then(|s| {
        crate::config::PolicyStorage::parse(&s).map(|p| p.as_str().to_string())
    });
    Ok(SkillFrontmatter {
        name,
        description,
        always_apply: get_bool(&map, "alwaysApply"),
        triggers: get_str_list(&map, "triggers"),
        tags,
        priority: get_i32(&map, "priority").unwrap_or(50),
        context_task: get_str(&map, "contextTask"),
        enabled: get_bool_default_true(&map, "enabled"),
        status,
        share,
        scope,
        storage,
        source: get_str(&map, "source").filter(|s| !s.is_empty()),
        root_id: get_str(&map, "rootId")
            .or_else(|| get_str(&map, "root_id"))
            .filter(|s| !s.is_empty()),
    })
}

fn ensure_shared_tag(tags: &mut Vec<String>, share: bool) {
    if !share {
        return;
    }
    if !tags.iter().any(|t| t.eq_ignore_ascii_case("shared")) {
        tags.push("shared".into());
    }
}

fn get_bool_default_true(map: &HashMap<String, Value>, key: &str) -> bool {
    map.get(key).and_then(|v| v.as_bool()).unwrap_or(true)
}

fn validate_rule(fm: &RuleFrontmatter) -> Result<(), ValidationError> {
    let mut fields = HashMap::new();
    if fm.id.is_empty() {
        fields.insert("id".into(), "required".into());
    } else if !fm.id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        fields.insert("id".into(), "must be kebab-case".into());
    }
    if crate::types::PolicyLevel::parse(&fm.level).is_none() {
        fields.insert("level".into(), "must be CRITICAL, WARNING, or INFO".into());
    }
    if crate::types::PolicyScope::parse(&fm.scope).is_none() {
        fields.insert(
            "scope".into(),
            "must be company, workspace, project, private_user, or private_project".into(),
        );
    }
    if let Some(ref s) = fm.storage {
        if crate::config::PolicyStorage::parse(s).is_none() {
            fields.insert("storage".into(), "must be files or database".into());
        }
    }
    if !fm.always_apply && fm.globs.is_empty() && fm.triggers.is_empty() {
        fields.insert(
            "triggers".into(),
            "turn on Always apply, or set at least one glob or trigger".into(),
        );
    }
    if !fields.is_empty() {
        return Err(ValidationError {
            error: "validation_failed".into(),
            fields,
        });
    }
    Ok(())
}

fn validate_skill(fm: &SkillFrontmatter) -> Result<(), ValidationError> {
    let mut fields = HashMap::new();
    if fm.name.is_empty() {
        fields.insert("name".into(), "required".into());
    }
    if fm.description.is_empty() {
        fields.insert("description".into(), "required".into());
    }
    if !fields.is_empty() {
        return Err(ValidationError {
            error: "validation_failed".into(),
            fields,
        });
    }
    Ok(())
}

pub fn serialize_rule(fm: &RuleFrontmatter, body: &str) -> String {
    let mut lines = vec![
        "---".into(),
        format!("id: {}", fm.id),
        format!("level: {}", fm.level),
        format!("alwaysApply: {}", fm.always_apply),
        format!(
            "globs: {}",
            serde_json::to_string(&fm.globs).unwrap_or_else(|_| "[]".into())
        ),
        format!(
            "triggers: {}",
            serde_json::to_string(&fm.triggers).unwrap_or_else(|_| "[]".into())
        ),
        format!(
            "tags: {}",
            serde_json::to_string(&fm.tags).unwrap_or_else(|_| "[]".into())
        ),
        format!("priority: {}", fm.priority),
        format!("enabled: {}", fm.enabled),
        format!("status: {}", fm.status),
        format!("scope: {}", fm.scope),
    ];
    if fm.share {
        lines.push("share: true".into());
    }
    if let Some(ref s) = fm.storage {
        lines.push(format!("storage: {s}"));
    }
    if let Some(ref s) = fm.source {
        lines.push(format!("source: {}", yaml_string(s)));
    }
    if let Some(ref r) = fm.root_id {
        lines.push(format!("rootId: {r}"));
    }
    lines.push("---".into());
    lines.push(String::new());
    lines.push(body.trim().to_string());
    lines.join("\n")
}

/// Serialize a stub rule file (frontmatter + stub marker body).
pub fn serialize_rule_stub(fm: &RuleFrontmatter) -> String {
    serialize_rule(fm, crate::paths::STUB_BODY_MARKER)
}

/// Serialize a stub skill file (frontmatter + stub marker body).
pub fn serialize_skill_stub(fm: &SkillFrontmatter) -> String {
    serialize_skill(fm, crate::paths::STUB_BODY_MARKER)
}

pub fn serialize_skill(fm: &SkillFrontmatter, body: &str) -> String {
    let mut lines = vec![
        "---".into(),
        format!("name: {}", fm.name),
        format!("description: {}", yaml_string(&fm.description)),
        format!("alwaysApply: {}", fm.always_apply),
    ];
    if !fm.triggers.is_empty() {
        lines.push(format!(
            "triggers: {}",
            serde_json::to_string(&fm.triggers).unwrap_or_else(|_| "[]".into())
        ));
    }
    if !fm.tags.is_empty() {
        lines.push(format!(
            "tags: {}",
            serde_json::to_string(&fm.tags).unwrap_or_else(|_| "[]".into())
        ));
    }
    lines.push(format!("priority: {}", fm.priority));
    lines.push(format!("enabled: {}", fm.enabled));
    lines.push(format!("status: {}", fm.status));
    lines.push(format!("scope: {}", fm.scope));
    if fm.share {
        lines.push("share: true".into());
    }
    if let Some(ref s) = fm.storage {
        lines.push(format!("storage: {s}"));
    }
    if let Some(ref s) = fm.source {
        lines.push(format!("source: {}", yaml_string(s)));
    }
    if let Some(ref r) = fm.root_id {
        lines.push(format!("rootId: {r}"));
    }
    if let Some(ref t) = fm.context_task {
        lines.push(format!("contextTask: {}", yaml_string(t)));
    }
    lines.push("---".into());
    lines.push(String::new());
    lines.push(body.trim().to_string());
    lines.join("\n")
}

fn yaml_string(s: &str) -> String {
    if s.contains('\n') || s.contains(':') {
        format!("|\n  {}", s.replace('\n', "\n  "))
    } else {
        format!("\"{}\"", s.replace('"', "\\\""))
    }
}

fn get_str(map: &HashMap<String, Value>, key: &str) -> Option<String> {
    map.get(key).and_then(|v| match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    })
}

fn get_bool(map: &HashMap<String, Value>, key: &str) -> bool {
    map.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn get_i32(map: &HashMap<String, Value>, key: &str) -> Option<i32> {
    map.get(key).and_then(|v| v.as_i64()).map(|n| n as i32)
}

fn get_str_list(map: &HashMap<String, Value>, key: &str) -> Vec<String> {
    map.get(key)
        .and_then(|v| match v {
            Value::Sequence(seq) => Some(
                seq.iter()
                    .filter_map(|i| i.as_str().map(String::from))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

fn field_err(field: &str, msg: &str) -> ValidationError {
    let mut fields = HashMap::new();
    fields.insert(field.into(), msg.into());
    ValidationError {
        error: "validation_failed".into(),
        fields,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_rule() {
        let raw = "---\nid: utf8\nlevel: CRITICAL\nalwaysApply: true\n---\n\nNever UTF-16.";
        let doc = parse_rule_file(Path::new("utf8.mdc"), raw).unwrap();
        assert_eq!(doc.frontmatter.id, "utf8");
        assert!(doc.frontmatter.always_apply);
    }

    #[test]
    fn api_json_accepts_camel_case_frontmatter() {
        use crate::types::RuleFrontmatter;
        #[derive(serde::Deserialize)]
        struct Payload {
            frontmatter: RuleFrontmatter,
            #[allow(dead_code)]
            body: String,
        }
        let json = r#"{"frontmatter":{"id":"hello-world","level":"CRITICAL","alwaysApply":true,"globs":[],"triggers":[],"tags":[],"priority":50},"body":"Hello"}"#;
        let p: Payload = serde_json::from_str(json).unwrap();
        assert!(p.frontmatter.always_apply);
        assert_eq!(p.frontmatter.id, "hello-world");
    }

    #[test]
    fn serialize_roundtrip_with_globs() {
        use crate::types::RuleFrontmatter;
        let fm = RuleFrontmatter {
            id: "hello-world".into(),
            level: "CRITICAL".into(),
            always_apply: true,
            globs: vec!["**/*.tsx".into(), "**/*.css".into()],
            triggers: vec!["mobile".into()],
            tags: vec!["hello".into()],
            priority: 50,
            enabled: true,
            status: "approved".into(),
            share: false,
            scope: "project".into(),
            storage: None,
            source: None,
            root_id: None,
        };
        let raw = serialize_rule(&fm, "Always say Hello World");
        let doc = parse_rule_file(Path::new("hello-world.mdc"), &raw).unwrap();
        assert_eq!(doc.frontmatter.globs.len(), 2);
        assert!(doc.frontmatter.always_apply);
    }

    #[test]
    fn parse_skill_always_apply_roundtrip() {
        let raw = "---\nname: old-coder\ndescription: evidence first\nalwaysApply: true\n---\n\nFollow the gauntlet.\n";
        let doc = parse_skill_file(Path::new("SKILL.md"), raw).unwrap();
        assert!(doc.frontmatter.always_apply);
        let round = serialize_skill(&doc.frontmatter, &doc.body);
        let again = parse_skill_file(Path::new("SKILL.md"), &round).unwrap();
        assert!(again.frontmatter.always_apply);
        assert!(round.contains("alwaysApply: true"));
    }
}
