use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockEntry {
    pub source: String,
    pub source_type: String,
    pub sha: String,
    pub hash: String,
    pub skill_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LockFile {
    pub version: u32,
    pub skills: BTreeMap<String, LockEntry>,
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            version: 1,
            skills: BTreeMap::new(),
        }
    }
}

impl LockFile {
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
        serde_json::from_str(&raw).map_err(|e| format!("skills lock: {e}"))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let raw = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, raw).map_err(|e| e.to_string())
    }

    /// Catalog ids for skills whose directory is still on disk.
    /// `anthropics/skills` + `pdf` is `anthropics/skills/pdf`, not every skill named pdf.
    pub fn installed_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        for (name, entry) in &self.skills {
            let path = Path::new(&entry.skill_path);
            if !path.join("SKILL.md").is_file() {
                continue;
            }
            ids.push(catalog_id(name, &entry.source));
        }
        ids
    }
}

pub fn catalog_id(name: &str, source: &str) -> String {
    let source = source.trim().trim_matches('/');
    if source.is_empty() || source == name || source.ends_with(&format!("/{name}")) {
        if source.is_empty() {
            name.to_string()
        } else {
            source.to_string()
        }
    } else {
        format!("{source}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("skills-lock.json");
        let mut lock = LockFile::default();
        lock.skills.insert(
            "pdf".into(),
            LockEntry {
                source: "acme/skills".into(),
                source_type: "github".into(),
                sha: "abc".into(),
                hash: "dead".into(),
                skill_path: "skills/pdf".into(),
            },
        );
        lock.save(&path).unwrap();
        let loaded = LockFile::load(&path).unwrap();
        assert_eq!(loaded.skills["pdf"].sha, "abc");
        assert_eq!(loaded.installed_ids(), Vec::<String>::new());
        fs::create_dir_all(dir.path().join("skills/pdf")).unwrap();
        fs::write(dir.path().join("skills/pdf/SKILL.md"), "---\nname: pdf\n---\n").unwrap();
        lock.skills.get_mut("pdf").unwrap().skill_path =
            dir.path().join("skills/pdf").display().to_string();
        assert_eq!(
            lock.installed_ids(),
            vec!["acme/skills/pdf".to_string()]
        );
        assert_eq!(catalog_id("pdf", "anthropics/skills/pdf"), "anthropics/skills/pdf");
    }
}
