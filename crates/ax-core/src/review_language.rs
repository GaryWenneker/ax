//! Language of comments an agent posts on a pull request.
//!
//! `reviews.commentLanguage` in `<project>/ax.json` wins over the same key in
//! `~/.ax/config.json`. When neither sets it, the language is English.

/// `(code, name shown in the Settings dropdown)`, in dropdown order.
pub const REVIEW_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("nl", "Nederlands"),
    ("de", "Deutsch"),
    ("fr", "Français"),
    ("es", "Español"),
    ("pt", "Português"),
    ("it", "Italiano"),
    ("pl", "Polski"),
    ("sv", "Svenska"),
    ("da", "Dansk"),
    ("tr", "Türkçe"),
];

/// The resolved language: its code and the name the agent writes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewLanguage {
    pub code: &'static str,
    pub name: &'static str,
}

/// Resolves the language from the raw JSON of the two config files.
/// `None` means the file is absent. A present key with a value outside
/// [`REVIEW_LANGUAGES`] is an error that names every allowed code.
pub fn resolve_review_language(
    project_json: Option<&str>,
    global_json: Option<&str>,
) -> Result<ReviewLanguage, String> {
    let local = read_code(project_json)?;
    let global = read_code(global_json)?;
    let code = local.or(global).unwrap_or_else(|| "en".to_string());
    language(&code).ok_or_else(|| invalid(&code))
}

fn read_code(json: Option<&str>) -> Result<Option<String>, String> {
    let Some(json) = json else {
        return Ok(None);
    };
    let value: serde_json::Value = serde_json::from_str(json).unwrap_or(serde_json::Value::Null);
    match value.pointer("/reviews/commentLanguage") {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(code)) => {
            let code = code.trim();
            if language(code).is_none() {
                Err(invalid(code))
            } else {
                Ok(Some(code.to_string()))
            }
        }
        Some(_) => Err(invalid("(not a string)")),
    }
}

fn language(code: &str) -> Option<ReviewLanguage> {
    for &(known, name) in REVIEW_LANGUAGES {
        if known == code {
            return Some(ReviewLanguage { code: known, name });
        }
    }
    None
}

fn invalid(code: &str) -> String {
    let allowed = REVIEW_LANGUAGES
        .iter()
        .map(|(c, _)| *c)
        .collect::<Vec<_>>()
        .join(", ");
    format!("reviews.commentLanguage \"{code}\" is not allowed; use one of: {allowed}")
}

/// Reads `ax.json` in `project_root` and `~/.ax/config.json`. A missing file counts as unset.
/// A file that is not JSON counts as unset. An invalid code is an error.
pub fn resolve_for_project(project_root: &std::path::Path) -> Result<ReviewLanguage, String> {
    let project = std::fs::read_to_string(project_root.join("ax.json")).ok();
    let global = global_config_json();
    resolve_review_language(project.as_deref(), global.as_deref())
}

fn global_config_json() -> Option<String> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    std::fs::read_to_string(
        std::path::PathBuf::from(home)
            .join(".ax")
            .join("config.json"),
    )
    .ok()
}

/// Writes `reviews.commentLanguage` into the project's `ax.json`, keeping every other key.
pub fn write_project_review_language(
    project_root: &std::path::Path,
    code: &str,
) -> Result<ReviewLanguage, String> {
    let lang = language(code).ok_or_else(|| invalid(code))?;
    let path = project_root.join("ax.json");
    let mut value = match std::fs::read_to_string(&path) {
        Ok(text) => serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|_| "ax.json is not valid JSON".to_string())?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => serde_json::json!({}),
        Err(e) => return Err(e.to_string()),
    };
    let root = value
        .as_object_mut()
        .ok_or_else(|| "ax.json is not a JSON object".to_string())?;
    let reviews = root
        .entry("reviews")
        .or_insert_with(|| serde_json::json!({}));
    let reviews = reviews
        .as_object_mut()
        .ok_or_else(|| "ax.json reviews is not a JSON object".to_string())?;
    reviews.insert(
        "commentLanguage".to_string(),
        serde_json::Value::String(lang.code.to_string()),
    );
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("{text}\n")).map_err(|e| e.to_string())?;
    Ok(lang)
}

/// One preflight line, for example `PR review comments: Nederlands (reviews.commentLanguage=nl)`.
pub fn review_language_line(language: ReviewLanguage) -> String {
    format!(
        "PR review comments: {} (reviews.commentLanguage={})",
        language.name, language.code
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l1_unset_is_english() {
        let lang = resolve_review_language(None, None).unwrap();
        assert_eq!(lang.code, "en");
        assert_eq!(lang.name, "English");
    }

    #[test]
    fn l2_global_dutch_applies_when_the_project_has_no_key() {
        let lang =
            resolve_review_language(Some("{}"), Some(r#"{"reviews":{"commentLanguage":"nl"}}"#))
                .unwrap();
        assert_eq!(lang.code, "nl");
    }

    #[test]
    fn l3_project_wins_over_global() {
        let lang = resolve_review_language(
            Some(r#"{"reviews":{"commentLanguage":"nl"}}"#),
            Some(r#"{"reviews":{"commentLanguage":"en"}}"#),
        )
        .unwrap();
        assert_eq!(lang.code, "nl");
    }

    #[test]
    fn l4_unknown_code_names_the_allowed_list() {
        let err = resolve_review_language(Some(r#"{"reviews":{"commentLanguage":"ja"}}"#), None)
            .unwrap_err();
        assert!(err.contains("ja"), "{err}");
        assert!(err.contains("en"), "{err}");
        assert!(err.contains("tr"), "{err}");
    }

    #[test]
    fn l8_saving_writes_the_code_and_keeps_other_keys() {
        let dir = std::env::temp_dir().join(format!("ax-review-lang-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("ax.json"),
            "{\n  \"policy\": { \"storage\": \"files\" }\n}\n",
        )
        .unwrap();
        let saved = write_project_review_language(&dir, "nl").unwrap();
        assert_eq!(saved.name, "Nederlands");
        let text = std::fs::read_to_string(dir.join("ax.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["reviews"]["commentLanguage"], "nl");
        assert_eq!(value["policy"]["storage"], "files");
        let resolved = resolve_for_project(&dir).unwrap();
        assert_eq!(resolved.code, "nl");
        assert_eq!(
            review_language_line(resolved),
            "PR review comments: Nederlands (reviews.commentLanguage=nl)"
        );
        let err = write_project_review_language(&dir, "ja").unwrap_err();
        assert!(err.contains("ja"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_dropdown_language_resolves_to_its_own_name() {
        for (code, name) in REVIEW_LANGUAGES {
            let json = format!(r#"{{"reviews":{{"commentLanguage":"{code}"}}}}"#);
            let lang = resolve_review_language(Some(&json), None).unwrap();
            assert_eq!((lang.code, lang.name), (*code, *name));
        }
    }
}
