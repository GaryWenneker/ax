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
    /// Reads `docsCatalog` from ax.json. Every configured path must stay inside
    /// `project_root` (no absolute paths, no `..`, no symbolic link out of the
    /// project), and `wiki_remote` must not look like a git option, because the
    /// file usually comes from the repository being synced.
    pub fn load(project_root: &Path) -> Result<Self, String> {
        let j = read_ax_json(project_root).unwrap_or_default();
        let path_or = |key: &str, v: &Option<String>, default: &str| {
            project_path(project_root, key, v.as_deref().unwrap_or(default))
        };
        if let Some(remote) = j.wiki_remote.as_deref() {
            check_remote(remote)?;
        }
        // These are joined onto wiki_local, so they must stay relative too.
        for (key, value) in [
            ("wiki_apps_subdir", &j.wiki_apps_subdir),
            ("wiki_integrations_dir", &j.wiki_integrations_dir),
            ("wiki_products_page", &j.wiki_products_page),
        ] {
            if let Some(v) = value.as_deref().filter(|v| !v.trim().is_empty()) {
                ax_utils::safe_relative(v).map_err(|e| {
                    format!("docsCatalog.{key} must be a path inside the wiki: {e}")
                })?;
            }
        }
        let folder_name = project_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();
        Ok(Self {
            name: j.name.clone().unwrap_or(folder_name),
            wiki_remote: j.wiki_remote.clone(),
            wiki_local: path_or("wiki_local", &j.wiki_local, ".current/wiki")?,
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
            jsonl_path: path_or(
                "jsonl_path",
                &j.jsonl_path,
                ".ax/memory/documentation-catalog.jsonl",
            )?,
            docs_root: path_or("docs_root", &j.docs_root, ".docs")?,
            skills_root: path_or("skills_root", &j.skills_root, ".agents/skills")?,
            scripts_root: path_or("scripts_root", &j.scripts_root, ".scripts")?,
        })
    }
}

/// Resolves a configured path under `project_root`; `.` means the root itself.
fn project_path(project_root: &Path, key: &str, value: &str) -> Result<PathBuf, String> {
    if value.trim().trim_end_matches('/') == "." {
        return Ok(project_root.to_path_buf());
    }
    ax_utils::contained_join(project_root, value)
        .map_err(|e| format!("docsCatalog.{key} must stay inside the project: {e}"))
}

/// Refuses a remote git would read as a command-line option.
fn check_remote(remote: &str) -> Result<(), String> {
    let r = remote.trim();
    if r.is_empty() || r.starts_with('-') {
        return Err(format!(
            "docsCatalog.wiki_remote is not a repository URL: {remote:?}"
        ));
    }
    Ok(())
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

    fn load_with(tag: &str, catalog: &str) -> Result<DocsCatalogConfig, String> {
        let root = temp_root(tag);
        std::fs::write(
            root.join("ax.json"),
            format!(r#"{{"docsCatalog":{catalog}}}"#),
        )
        .unwrap();
        let res = DocsCatalogConfig::load(&root);
        let _ = std::fs::remove_dir_all(&root);
        res
    }

    #[test]
    fn absolute_jsonl_path_is_rejected() {
        let err = load_with("abs-jsonl", r#"{"jsonl_path":"/tmp/ax-victim-rc"}"#).unwrap_err();
        assert!(err.contains("jsonl_path"), "{err}");
    }

    #[test]
    fn parent_dir_wiki_local_is_rejected() {
        let err = load_with("dotdot-wiki", r#"{"wiki_local":"../../elsewhere"}"#).unwrap_err();
        assert!(err.contains("wiki_local"), "{err}");
    }

    #[test]
    fn every_configured_path_must_stay_inside_the_project() {
        for key in ["docs_root", "skills_root", "scripts_root"] {
            let catalog = format!(r#"{{"{key}":"/etc"}}"#);
            let err = load_with(&format!("abs-{key}"), &catalog).unwrap_err();
            assert!(err.contains(key), "{err}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlink_out_of_the_project_is_rejected() {
        let root = temp_root("symlink");
        let outside = temp_root("symlink-target");
        std::os::unix::fs::symlink(&outside, root.join("out")).unwrap();
        std::fs::write(
            root.join("ax.json"),
            r#"{"docsCatalog":{"jsonl_path":"out/catalog.jsonl"}}"#,
        )
        .unwrap();
        let err = DocsCatalogConfig::load(&root).unwrap_err();
        assert!(err.contains("jsonl_path"), "{err}");
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn wiki_sub_paths_must_stay_relative() {
        for key in [
            "wiki_apps_subdir",
            "wiki_integrations_dir",
            "wiki_products_page",
        ] {
            let catalog = format!(r#"{{"{key}":"/etc/passwd"}}"#);
            let err = load_with(&format!("wiki-{key}"), &catalog).unwrap_err();
            assert!(err.contains(key), "{err}");
        }
    }

    #[test]
    fn remote_that_looks_like_an_option_is_rejected() {
        let err = load_with(
            "dash-remote",
            r#"{"wiki_remote":"--upload-pack=touch /tmp/pwned"}"#,
        )
        .unwrap_err();
        assert!(err.contains("wiki_remote"), "{err}");
    }

    #[test]
    fn dot_means_the_project_root() {
        let root = temp_root("dot");
        std::fs::write(root.join("ax.json"), r#"{"docsCatalog":{"docs_root":"."}}"#).unwrap();
        let cfg = DocsCatalogConfig::load(&root).unwrap();
        assert_eq!(cfg.docs_root, root);
        let _ = std::fs::remove_dir_all(&root);
    }
}
