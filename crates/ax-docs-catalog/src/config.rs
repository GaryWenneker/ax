//! Configuration for documentation catalog sync (`docsCatalog` in ax.json).

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct DocsCatalogConfig {
    pub name: String,
    pub wiki_remote: Option<String>,
    pub wiki_local: PathBuf,
    pub wiki_apps_subdir: String,
    pub wiki_root_url: String,
    pub wiki_integrations_dir: String,
    pub wiki_products_page: Option<String>,
    pub wiki_products_skip: Vec<String>,
    pub skill: Option<String>,
    pub jsonl_path: PathBuf,
    pub docs_root: PathBuf,
    pub skills_root: PathBuf,
    pub scripts_root: PathBuf,
}

#[derive(Debug, Deserialize, Default)]
struct AxJsonRoot {
    #[serde(rename = "docsCatalog")]
    docs_catalog: Option<DocsCatalogJson>,
}

#[derive(Debug, Deserialize, Default)]
struct DocsCatalogJson {
    name: Option<String>,
    wiki_remote: Option<String>,
    wiki_local: Option<String>,
    wiki_apps_subdir: Option<String>,
    wiki_root_url: Option<String>,
    wiki_integrations_dir: Option<String>,
    wiki_products_page: Option<String>,
    wiki_products_skip: Option<Vec<String>>,
    skill: Option<String>,
    jsonl_path: Option<String>,
    docs_root: Option<String>,
    skills_root: Option<String>,
    scripts_root: Option<String>,
}

impl DocsCatalogConfig {
    pub fn load(project_root: &Path) -> Result<Self, String> {
        let j = read_ax_json(project_root).unwrap_or_default();
        let path_or =
            |v: &Option<String>, default: &str| project_root.join(v.as_deref().unwrap_or(default));
        let folder_name = project_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();
        Ok(Self {
            name: j.name.clone().unwrap_or(folder_name),
            wiki_remote: j.wiki_remote.clone(),
            wiki_local: path_or(&j.wiki_local, ".current/wiki"),
            wiki_apps_subdir: j.wiki_apps_subdir.clone().unwrap_or_default(),
            wiki_root_url: j
                .wiki_root_url
                .clone()
                .or_else(|| j.wiki_remote.clone())
                .unwrap_or_default(),
            wiki_integrations_dir: j
                .wiki_integrations_dir
                .clone()
                .unwrap_or_else(|| "Integrations".into()),
            wiki_products_page: j.wiki_products_page.clone(),
            wiki_products_skip: j.wiki_products_skip.clone().unwrap_or_default(),
            skill: j.skill.clone(),
            jsonl_path: path_or(&j.jsonl_path, ".ax/memory/documentation-catalog.jsonl"),
            docs_root: path_or(&j.docs_root, ".docs"),
            skills_root: path_or(&j.skills_root, ".agents/skills"),
            scripts_root: path_or(&j.scripts_root, ".scripts"),
        })
    }
}

fn read_ax_json(project_root: &Path) -> Option<DocsCatalogJson> {
    for name in ["ax.json", ".ax.json"] {
        let Ok(content) = std::fs::read_to_string(project_root.join(name)) else {
            continue;
        };
        let Ok(root) = serde_json::from_str::<AxJsonRoot>(&content) else {
            continue;
        };
        if let Some(cfg) = root.docs_catalog {
            return Some(cfg);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ax-docs-catalog-config-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn defaults_without_ax_json_have_no_wiki_and_no_client_values() {
        let root = temp_root("defaults").join("my-project");
        std::fs::create_dir_all(&root).unwrap();
        let cfg = DocsCatalogConfig::load(&root).unwrap();
        assert_eq!(cfg.name, "my-project");
        assert_eq!(cfg.wiki_remote, None);
        assert_eq!(cfg.wiki_local, root.join(".current/wiki"));
        assert_eq!(cfg.wiki_apps_subdir, "");
        assert_eq!(cfg.wiki_root_url, "");
        assert_eq!(cfg.wiki_integrations_dir, "Integrations");
        assert_eq!(cfg.wiki_products_page, None);
        assert!(cfg.wiki_products_skip.is_empty());
        assert_eq!(cfg.skill, None);
        assert_eq!(
            cfg.jsonl_path,
            root.join(".ax/memory/documentation-catalog.jsonl")
        );
        let _ = std::fs::remove_dir_all(root.parent().unwrap());
    }

    #[test]
    fn ax_json_values_override_defaults() {
        let root = temp_root("override");
        std::fs::write(
            root.join("ax.json"),
            r#"{"docsCatalog":{
                "name":"Contoso",
                "wiki_remote":"https://example.com/contoso.wiki.git",
                "wiki_local":".wiki",
                "wiki_apps_subdir":"Apps",
                "wiki_integrations_dir":"Links",
                "wiki_products_page":"Products.md",
                "wiki_products_skip":["Name -- Shared"],
                "skill":"contoso-docs"
            }}"#,
        )
        .unwrap();
        let cfg = DocsCatalogConfig::load(&root).unwrap();
        assert_eq!(cfg.name, "Contoso");
        assert_eq!(
            cfg.wiki_remote.as_deref(),
            Some("https://example.com/contoso.wiki.git")
        );
        assert_eq!(cfg.wiki_local, root.join(".wiki"));
        assert_eq!(cfg.wiki_apps_subdir, "Apps");
        assert_eq!(cfg.wiki_root_url, "https://example.com/contoso.wiki.git");
        assert_eq!(cfg.wiki_integrations_dir, "Links");
        assert_eq!(cfg.wiki_products_page.as_deref(), Some("Products.md"));
        assert_eq!(cfg.wiki_products_skip, vec!["Name -- Shared".to_string()]);
        assert_eq!(cfg.skill.as_deref(), Some("contoso-docs"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn explicit_wiki_root_url_wins_over_remote() {
        let root = temp_root("rooturl");
        std::fs::write(
            root.join(".ax.json"),
            r#"{"docsCatalog":{"wiki_remote":"https://example.com/w.git","wiki_root_url":"https://example.com/wiki"}}"#,
        )
        .unwrap();
        let cfg = DocsCatalogConfig::load(&root).unwrap();
        assert_eq!(cfg.wiki_root_url, "https://example.com/wiki");
        let _ = std::fs::remove_dir_all(&root);
    }
}
