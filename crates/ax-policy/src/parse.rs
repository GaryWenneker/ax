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
    Ok(RuleFrontmatter {
        id,
        level,
        always_apply: get_bool(&map, "alwaysApply"),
        globs: get_str_list(&map, "globs"),
        triggers: get_str_list(&map, "triggers"),
        tags: get_str_list(&map, "tags"),
        priority: get_i32(&map, "priority").unwrap_or(50),
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
    Ok(SkillFrontmatter {
        name,
        description,
        triggers: get_str_list(&map, "triggers"),
        tags: get_str_list(&map, "tags"),
        priority: get_i32(&map, "priority").unwrap_or(50),
        context_task: get_str(&map, "contextTask"),
    })
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
    if !fm.always_apply && fm.globs.is_empty() && fm.triggers.is_empty() {
        fields.insert(
            "triggers".into(),
            "need alwaysApply, globs, or triggers".into(),
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
    let yaml = format!(
        "---\nid: {}\nlevel: {}\nalwaysApply: {}\nglobs: {}\ntriggers: {}\ntags: {}\npriority: {}\n---\n\n{}",
        fm.id,
        fm.level,
        fm.always_apply,
        serde_json::to_string(&fm.globs).unwrap_or_else(|_| "[]".into()),
        serde_json::to_string(&fm.triggers).unwrap_or_else(|_| "[]".into()),
        serde_json::to_string(&fm.tags).unwrap_or_else(|_| "[]".into()),
        fm.priority,
        body.trim()
    );
    yaml
}

pub fn serialize_skill(fm: &SkillFrontmatter, body: &str) -> String {
    let mut lines = vec![
        "---".into(),
        format!("name: {}", fm.name),
        format!("description: {}", yaml_string(&fm.description)),
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
}
