//! Remote policy share configuration — per-project `ax.json` only.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CONFIG_FILENAME: &str = "ax.json";

/// Default io team OneDrive share folder for shared ax policy.
pub const DEFAULT_ONEDRIVE_SHARE_URL: &str =
    "https://ioworkspace-my.sharepoint.com/:f:/r/personal/gary_wenneker_iodigital_com/Documents/.ax";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShareProvider {
    #[default]
    Onedrive,
    Github,
}

impl ShareProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Onedrive => "onedrive",
            Self::Github => "github",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "onedrive" | "one_drive" | "sharepoint" => Some(Self::Onedrive),
            "github" | "git" => Some(Self::Github),
            _ => None,
        }
    }
}

/// Maps to `(require_review, force)` for pack import — mirrors ax-web Policy Sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShareImportMode {
    #[default]
    Review,
    Merge,
    Force,
}

impl ShareImportMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Merge => "merge",
            Self::Force => "force",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_lowercase().as_str() {
            "review" => Some(Self::Review),
            "merge" => Some(Self::Merge),
            "force" | "overwrite" => Some(Self::Force),
            _ => None,
        }
    }

    pub fn import_flags(self) -> (bool, bool) {
        match self {
            Self::Review => (true, false),
            Self::Merge => (false, false),
            Self::Force => (false, true),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareContentConfig {
    #[serde(default = "default_true")]
    pub rules: bool,
    #[serde(default = "default_true")]
    pub skills: bool,
    #[serde(default)]
    pub memory: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OneDriveShareConfig {
    #[serde(default = "default_onedrive_url")]
    pub share_url: String,
}

impl Default for OneDriveShareConfig {
    fn default() -> Self {
        Self {
            share_url: default_onedrive_url(),
        }
    }
}

fn default_onedrive_url() -> String {
    DEFAULT_ONEDRIVE_SHARE_URL.to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubShareConfig {
    #[serde(default)]
    pub repo_url: String,
    #[serde(default = "default_branch")]
    pub branch: String,
    #[serde(default = "default_subpath")]
    pub subpath: String,
    /// Optional API token (e.g. GitLab personal/project access token). When
    /// set and `repo_url` is an http(s) URL, sync uses that host's REST API
    /// instead of the raw git protocol — needed for instances that gate
    /// SSH/git-smart-HTTP behind an SSO proxy but expose a scoped `/api/v4`
    /// surface for automation (common on locked-down self-hosted GitLab).
    #[serde(default)]
    pub token: String,
}

impl Default for GithubShareConfig {
    fn default() -> Self {
        Self {
            repo_url: String::new(),
            branch: default_branch(),
            subpath: default_subpath(),
            token: String::new(),
        }
    }
}

fn default_branch() -> String {
    "main".to_string()
}

fn default_subpath() -> String {
    ".ax".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareConfig {
    #[serde(default)]
    pub provider: ShareProvider,
    #[serde(default)]
    pub content: ShareContentConfig,
    #[serde(default)]
    pub import_mode: ShareImportMode,
    #[serde(default = "default_auto_sync")]
    pub auto_sync_minutes: u32,
    #[serde(default)]
    pub onedrive: OneDriveShareConfig,
    #[serde(default)]
    pub github: GithubShareConfig,
}

fn default_auto_sync() -> u32 {
    15
}

impl Default for ShareConfig {
    fn default() -> Self {
        Self {
            provider: ShareProvider::Onedrive,
            content: ShareContentConfig {
                rules: true,
                skills: true,
                memory: false,
            },
            import_mode: ShareImportMode::Review,
            auto_sync_minutes: 15,
            onedrive: OneDriveShareConfig::default(),
            github: GithubShareConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ShareSectionFile {
    #[serde(flatten)]
    inner: Option<ShareConfigPatch>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct ShareConfigPatch {
    provider: Option<ShareProvider>,
    content: Option<ShareContentConfig>,
    import_mode: Option<ShareImportMode>,
    auto_sync_minutes: Option<u32>,
    onedrive: Option<OneDriveShareConfig>,
    github: Option<GithubShareConfig>,
}

fn merge_share(base: ShareConfig, patch: ShareConfigPatch) -> ShareConfig {
    ShareConfig {
        provider: patch.provider.unwrap_or(base.provider),
        content: patch.content.unwrap_or(base.content),
        import_mode: patch.import_mode.unwrap_or(base.import_mode),
        auto_sync_minutes: patch.auto_sync_minutes.unwrap_or(base.auto_sync_minutes),
        onedrive: patch.onedrive.unwrap_or(base.onedrive),
        github: patch.github.unwrap_or(base.github),
    }
}

fn read_share_section(path: &Path) -> Option<ShareConfigPatch> {
    let content = std::fs::read_to_string(path).ok()?;
    let root: serde_json::Value = serde_json::from_str(&content).ok()?;
    root.get("share")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// Load share config from project `ax.json` (defaults when missing).
pub fn load_share_config(project_root: &Path) -> ShareConfig {
    let mut cfg = ShareConfig::default();
    let project_path = project_root.join(CONFIG_FILENAME);
    if let Some(local) = read_share_section(&project_path) {
        cfg = merge_share(cfg, local);
    }
    if cfg.onedrive.share_url.trim().is_empty() {
        cfg.onedrive.share_url = DEFAULT_ONEDRIVE_SHARE_URL.to_string();
    }
    cfg
}

/// Path to the project share config file (`ax.json`).
pub fn project_config_path(project_root: &Path) -> PathBuf {
    project_root.join(CONFIG_FILENAME)
}

/// Save share config to project `ax.json`.
pub fn write_project_share_config(project_root: &Path, config: &ShareConfig) -> Result<PathBuf, String> {
    let path = project_config_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_share_at(&path, config)?;
    Ok(path)
}

fn write_share_at(path: &Path, config: &ShareConfig) -> Result<(), String> {
    let mut root: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let obj = root
        .as_object_mut()
        .ok_or_else(|| "config root must be a JSON object".to_string())?;
    obj.insert(
        "share".into(),
        serde_json::to_value(config).map_err(|e| e.to_string())?,
    );
    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())? + "\n";
    std::fs::write(path, text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn load_from_project(project: &Path) -> ShareConfig {
        load_share_config(project)
    }

    fn write_share_json(path: &Path, share: serde_json::Value) {
        let root = serde_json::json!({ "share": share });
        fs::write(path, serde_json::to_string_pretty(&root).unwrap() + "\n").unwrap();
    }

    #[test]
    fn import_mode_flags() {
        assert_eq!(ShareImportMode::Review.import_flags(), (true, false));
        assert_eq!(ShareImportMode::Merge.import_flags(), (false, false));
        assert_eq!(ShareImportMode::Force.import_flags(), (false, true));
    }

    #[test]
    fn import_mode_parse_and_as_str() {
        assert_eq!(ShareImportMode::parse("review"), Some(ShareImportMode::Review));
        assert_eq!(ShareImportMode::parse("merge"), Some(ShareImportMode::Merge));
        assert_eq!(ShareImportMode::parse("force"), Some(ShareImportMode::Force));
        assert_eq!(ShareImportMode::parse("overwrite"), Some(ShareImportMode::Force));
        assert_eq!(ShareImportMode::parse("invalid"), None);
        assert_eq!(ShareImportMode::Review.as_str(), "review");
        assert_eq!(ShareImportMode::Merge.as_str(), "merge");
        assert_eq!(ShareImportMode::Force.as_str(), "force");
    }

    #[test]
    fn share_provider_parse() {
        assert_eq!(ShareProvider::parse("onedrive"), Some(ShareProvider::Onedrive));
        assert_eq!(ShareProvider::parse("sharepoint"), Some(ShareProvider::Onedrive));
        assert_eq!(ShareProvider::parse("github"), Some(ShareProvider::Github));
        assert_eq!(ShareProvider::parse("git"), Some(ShareProvider::Github));
        assert_eq!(ShareProvider::parse("dropbox"), None);
    }

    #[test]
    fn default_onedrive_url() {
        let cfg = ShareConfig::default();
        assert!(cfg.onedrive.share_url.contains("sharepoint.com"));
    }

    #[test]
    fn project_config_overrides_defaults() {
        let project_dir = TempDir::new().unwrap();
        write_share_json(
            &project_dir.path().join(CONFIG_FILENAME),
            serde_json::json!({
                "provider": "onedrive",
                "importMode": "merge",
                "autoSyncMinutes": 5,
                "onedrive": { "shareUrl": "https://contoso.sharepoint.com/team" }
            }),
        );

        let cfg = load_from_project(project_dir.path());
        assert_eq!(cfg.provider, ShareProvider::Onedrive);
        assert_eq!(cfg.import_mode, ShareImportMode::Merge);
        assert_eq!(cfg.auto_sync_minutes, 5);
        assert_eq!(cfg.onedrive.share_url, "https://contoso.sharepoint.com/team");
    }

    #[test]
    fn global_config_is_ignored() {
        let global_dir = TempDir::new().unwrap();
        let global_path = global_dir.path().join("config.json");
        write_share_json(
            &global_path,
            serde_json::json!({
                "importMode": "force",
                "autoSyncMinutes": 99
            }),
        );

        let project_dir = TempDir::new().unwrap();
        write_share_json(
            &project_dir.path().join(CONFIG_FILENAME),
            serde_json::json!({
                "importMode": "review",
                "autoSyncMinutes": 15
            }),
        );

        let cfg = load_from_project(project_dir.path());
        assert_eq!(cfg.import_mode, ShareImportMode::Review);
        assert_eq!(cfg.auto_sync_minutes, 15);
    }

    #[test]
    fn merge_global_then_project_override() {
        let project_dir = TempDir::new().unwrap();
        write_share_json(
            &project_dir.path().join(CONFIG_FILENAME),
            serde_json::json!({
                "provider": "onedrive",
                "importMode": "merge",
                "autoSyncMinutes": 5,
                "onedrive": { "shareUrl": "https://contoso.sharepoint.com/global" },
                "github": {
                    "repoUrl": "https://github.com/acme/ax-policy.git",
                    "branch": "develop"
                }
            }),
        );

        let cfg = load_from_project(project_dir.path());
        assert_eq!(cfg.provider, ShareProvider::Onedrive);
        assert_eq!(cfg.import_mode, ShareImportMode::Merge);
        assert_eq!(cfg.auto_sync_minutes, 5);
        assert_eq!(cfg.onedrive.share_url, "https://contoso.sharepoint.com/global");
        assert_eq!(cfg.github.repo_url, "https://github.com/acme/ax-policy.git");
        assert_eq!(cfg.github.branch, "develop");
    }

    #[test]
    fn project_can_switch_provider() {
        let project_dir = TempDir::new().unwrap();
        write_share_json(
            &project_dir.path().join(CONFIG_FILENAME),
            serde_json::json!({
                "provider": "github",
                "github": {
                    "repoUrl": "https://github.com/team/policy-pack.git",
                    "subpath": "packs/default"
                }
            }),
        );

        let cfg = load_from_project(project_dir.path());
        assert_eq!(cfg.provider, ShareProvider::Github);
        assert_eq!(cfg.github.repo_url, "https://github.com/team/policy-pack.git");
        assert_eq!(cfg.github.subpath, "packs/default");
    }

    #[test]
    fn content_config_from_project() {
        let project_dir = TempDir::new().unwrap();
        write_share_json(
            &project_dir.path().join(CONFIG_FILENAME),
            serde_json::json!({
                "content": { "rules": false, "skills": true, "memory": false }
            }),
        );

        let cfg = load_from_project(project_dir.path());
        assert!(!cfg.content.rules);
        assert!(cfg.content.skills);
        assert!(!cfg.content.memory);
    }
}
