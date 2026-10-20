#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
}

pub fn validate_skill_name(name: &str) -> Result<(), String> {
    if name.is_empty() || name.len() > 64 {
        return Err(format!("skill name length must be 1-64: {name}"));
    }
    if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return Err(format!("skill name has an invalid hyphen: {name}"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(format!(
            "skill name must be lowercase letters, digits, and hyphens: {name}"
        ));
    }
    Ok(())
}

pub fn read_skill_meta(markdown: &str) -> Result<SkillMeta, String> {
    read_skill_meta_max(markdown, 1024)
}

/// Catalog installs keep the published description, including ones longer than 1024 characters.
pub fn read_catalog_meta(markdown: &str) -> Result<SkillMeta, String> {
    read_skill_meta_max(markdown, 100_000)
}

fn read_skill_meta_max(markdown: &str, max_description: usize) -> Result<SkillMeta, String> {
    let trimmed = markdown.trim_start();
    if !trimmed.starts_with("---") {
        return Err("SKILL.md must start with YAML frontmatter".into());
    }
    let rest = trimmed.trim_start_matches("---").trim_start();
    let end = rest
        .find("\n---")
        .ok_or_else(|| "SKILL.md is missing the closing ---".to_string())?;
    let yaml = rest[..end].trim();
    let value: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| format!("SKILL.md frontmatter: {e}"))?;
    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let description = value
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    validate_skill_name(&name)?;
    let description_len = description.chars().count();
    if description.is_empty() || description_len > max_description {
        return Err(format!(
            "description must be 1-{max_description} characters"
        ));
    }
    Ok(SkillMeta { name, description })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_names() {
        assert!(validate_skill_name("PDF").is_err());
        assert!(validate_skill_name("-pdf").is_err());
        assert!(validate_skill_name("pdf--x").is_err());
        assert!(validate_skill_name("pdf").is_ok());
    }

    #[test]
    fn reads_minimal_frontmatter() {
        let meta =
            read_skill_meta("---\nname: pdf\ndescription: Read PDFs.\n---\n\nBody\n").unwrap();
        assert_eq!(meta.name, "pdf");
        assert_eq!(meta.description, "Read PDFs.");
    }
}
