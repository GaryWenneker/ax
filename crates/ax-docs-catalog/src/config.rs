//! Configuration for documentation catalog sync (ax.json or VfPf defaults).

use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct DocsCatalogConfig {
    pub wiki_remote: String,
    pub wiki_local: PathBuf,
    pub wiki_apps_subdir: String,
    pub wiki_root_url: String,
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

#[derive(Debug, Deserialize)]
struct DocsCatalogJson {
    wiki_remote: Option<String>,
    wiki_local: Option<String>,
    wiki_apps_subdir: Option<String>,
    wiki_root_url: Option<String>,
    jsonl_path: Option<String>,
    docs_root: Option<String>,
    skills_root: Option<String>,
    scripts_root: Option<String>,
}

impl DocsCatalogConfig {
    pub fn load(project_root: &Path) -> Result<Self, String> {
        let from_json = read_ax_json(project_root);
        Ok(Self {
            wiki_remote: from_json
                .as_ref()
                .and_then(|j| j.wiki_remote.clone())
                .unwrap_or_else(default_wiki_remote),
            wiki_local: project_root.join(
                from_json
                    .as_ref()
                    .and_then(|j| j.wiki_local.as_deref())
                    .unwrap_or(".current/Frontends-Algemeen.wiki"),
            ),
            wiki_apps_subdir: from_json
                .as_ref()
                .and_then(|j| j.wiki_apps_subdir.clone())
                .unwrap_or_else(|| "Frontends-applicaties".into()),
            wiki_root_url: from_json
                .as_ref()
                .and_then(|j| j.wiki_root_url.clone())
                .unwrap_or_else(default_wiki_root_url),
            jsonl_path: project_root.join(
                from_json
                    .as_ref()
                    .and_then(|j| j.jsonl_path.as_deref())
                    .unwrap_or(".ax/memory/documentation-catalog.jsonl"),
            ),
            docs_root: project_root.join(
                from_json
                    .as_ref()
                    .and_then(|j| j.docs_root.as_deref())
                    .unwrap_or(".docs"),
            ),
            skills_root: project_root.join(
                from_json
                    .as_ref()
                    .and_then(|j| j.skills_root.as_deref())
                    .unwrap_or(".agents/skills"),
            ),
            scripts_root: project_root.join(
                from_json
                    .as_ref()
                    .and_then(|j| j.scripts_root.as_deref())
                    .unwrap_or(".scripts"),
            ),
        })
    }
}

fn read_ax_json(project_root: &Path) -> Option<DocsCatalogJson> {
    for name in ["ax.json", ".ax.json"] {
        let path = project_root.join(name);
        let content = std::fs::read_to_string(&path).ok()?;
        let root: AxJsonRoot = serde_json::from_str(&content).ok()?;
        if let Some(cfg) = root.docs_catalog {
            return Some(cfg);
        }
    }
    None
}

fn default_wiki_remote() -> String {
    "https://dev.azure.com/VfPf-NL/Frontends-Algemeen/_git/Frontends-Algemeen.wiki".into()
}

fn default_wiki_root_url() -> String {
    "https://dev.azure.com/VfPf-NL/Frontends-Algemeen/_wiki/wikis/Frontends-Algemeen.wiki/752/Frontends-applicaties".into()
}
