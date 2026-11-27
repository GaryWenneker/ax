//! Remote policy share configuration — per-project `ax.json` only.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const CONFIG_FILENAME: &str = "ax.json";

/// No default OneDrive share folder: set `share.onedrive.shareUrl` in ax.json.
pub const DEFAULT_ONEDRIVE_SHARE_URL: &str = "";

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
    ///
    /// Accepted on input (settings API, or an older `ax.json`), but never
    /// serialized: it is kept per repo URL in `~/.ax/auth/git-share.json`
    /// (owner-only) and filled in when the config is loaded. An empty token
    /// on save keeps the stored one.
    #[serde(default, skip_serializing)]
    pub token: String,
    /// Output only: a token is stored for `repo_url`.
    #[serde(default, skip_deserializing)]
    pub has_token: bool,
    /// Input only: remove the stored token for `repo_url` on save.
    #[serde(default, skip_serializing)]
    pub clear_token: bool,
}

impl Default for GithubShareConfig {
    fn default() -> Self {
        Self {
            repo_url: String::new(),
            branch: default_branch(),
            subpath: default_subpath(),
            token: String::new(),
            has_token: false,
            clear_token: false,
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

/// Load share config from project `ax.json` (defaults when missing), with the
/// git API token from the user's token store.
pub fn load_share_config(project_root: &Path) -> ShareConfig {
    load_share_config_with(project_root, &git_token_store_path())
}

/// [`load_share_config`] with an explicit token store file.
pub fn load_share_config_with(project_root: &Path, token_store: &Path) -> ShareConfig {
    let cfg = ShareConfig::default();
    let mut cfg = match read_share_section(&project_root.join(CONFIG_FILENAME)) {
        Some(local) => merge_share(cfg, local),
        None => cfg,
    };
    cfg.github.clear_token = false;
    if cfg.github.token.trim().is_empty() {
        cfg.github.token = read_git_token(token_store, &cfg.github.repo_url).unwrap_or_default();
    }
    cfg.github.has_token = !cfg.github.token.trim().is_empty();
    cfg
}

/// Path to the project share config file (`ax.json`).
pub fn project_config_path(project_root: &Path) -> PathBuf {
    project_root.join(CONFIG_FILENAME)
}

/// File holding git API tokens per repo URL, readable by the owner only.
pub fn git_token_store_path() -> PathBuf {
    crate::paths::auth_dir().join("git-share.json")
}

fn token_key(repo_url: &str) -> String {
    repo_url.trim().to_string()
}

fn read_git_token(store: &Path, repo_url: &str) -> Option<String> {
    let text = std::fs::read_to_string(store).ok()?;
    let root: serde_json::Value = serde_json::from_str(&text).ok()?;
    root.get("tokens")?
        .get(token_key(repo_url))?
        .as_str()
        .map(str::to_string)
}

/// Sets (`Some`) or removes (`None`) the token for `repo_url` in `store`.
fn write_git_token(store: &Path, repo_url: &str, token: Option<&str>) -> Result<(), String> {
    if let Some(dir) = store.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        restrict(dir, 0o700);
    }
    let key = token_key(repo_url);
    update_json_object(store, |obj| {
        let tokens = obj.entry("tokens").or_insert_with(|| serde_json::json!({}));
        if !tokens.is_object() {
            *tokens = serde_json::json!({});
        }
        let tokens = tokens.as_object_mut().expect("tokens is an object");
        match token {
            Some(t) => {
                tokens.insert(key, serde_json::Value::String(t.to_string()));
            }
            None => {
                tokens.remove(&key);
            }
        }
        Ok(())
    })?;
    restrict(store, 0o600);
    Ok(())
}

#[cfg(unix)]
fn restrict(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode));
}

#[cfg(not(unix))]
fn restrict(_path: &Path, _mode: u32) {}

/// Save share config to project `ax.json`. A git API token goes to the
/// user's token store instead, never into `ax.json`.
pub fn write_project_share_config(
    project_root: &Path,
    config: &ShareConfig,
) -> Result<PathBuf, String> {
    write_project_share_config_with(project_root, config, &git_token_store_path())
}

/// [`write_project_share_config`] with an explicit token store file.
pub fn write_project_share_config_with(
    project_root: &Path,
    config: &ShareConfig,
    token_store: &Path,
) -> Result<PathBuf, String> {
    let path = project_config_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let github = &config.github;
    if github.clear_token {
        write_git_token(token_store, &github.repo_url, None)?;
    } else if !github.token.trim().is_empty() {
        write_git_token(token_store, &github.repo_url, Some(github.token.trim()))?;
    }
    write_share_at(&path, config)?;
    Ok(path)
}

fn write_share_at(path: &Path, config: &ShareConfig) -> Result<(), String> {
    let mut share = serde_json::to_value(config).map_err(|e| e.to_string())?;
    if let Some(github) = share.get_mut("github").and_then(|g| g.as_object_mut()) {
        github.remove("hasToken");
    }
    // The project's ax.json sits in the repository root next to committed
    // files, so it is written atomically without the sibling `.lock` file
    // that `update_json_object` leaves behind.
    let current = match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    let next = edited_json_object(path, current, |obj| {
        obj.insert("share".into(), share);
        Ok(())
    })
    .map_err(|e| e.to_string())?;
    ax_utils::atomic_write(path, &next).map_err(|e| e.to_string())
}

/// Read-modify-write of a JSON config object under the shared file lock
/// (`<path>.lock`, which stays next to the file; use it for files under
/// `~/.ax`). A missing (or empty) file starts from `{}`. A file that does
/// not parse, or whose root is not an object, is left untouched and
/// reported, so a typo in a hand-edited config never costs the user their
/// other settings.
pub(crate) fn update_json_object(
    path: &Path,
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<(), String>,
) -> Result<(), String> {
    ax_utils::update_locked(path, |current| {
        edited_json_object(path, current, edit).map(Some)
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// The new bytes of the JSON object at `path` (current content `current`)
/// after `edit`; see [`update_json_object`] for the rules.
fn edited_json_object(
    path: &Path,
    current: Option<Vec<u8>>,
    edit: impl FnOnce(&mut serde_json::Map<String, serde_json::Value>) -> Result<(), String>,
) -> std::io::Result<Vec<u8>> {
    use std::io;
    let mut root = match current {
        Some(bytes) if !bytes.iter().all(u8::is_ascii_whitespace) => {
            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(value) if value.is_object() => value,
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                            "{} is not a JSON object; fix it and try again (it was not changed)",
                            path.display()
                        ),
                    ))
                }
                Err(error) => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!(
                        "{} is not valid JSON ({error}); fix it and try again (it was not changed)",
                        path.display()
                    ),
                    ))
                }
            }
        }
        _ => serde_json::json!({}),
    };
    let obj = root.as_object_mut().expect("root is an object");
    edit(obj).map_err(io::Error::other)?;
    let text = serde_json::to_string_pretty(&root).map_err(io::Error::other)? + "\n";
    Ok(text.into_bytes())
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
    fn share_settings_never_replace_an_unparsable_project_config() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(CONFIG_FILENAME);
        let broken = "{\"index\": {\"exclude\": [\"target\"],},}\n";
        fs::write(&path, broken).unwrap();
        let error = write_project_share_config(tmp.path(), &ShareConfig::default()).unwrap_err();
        assert!(error.contains(CONFIG_FILENAME), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), broken);
    }

    #[test]
    fn share_settings_keep_the_other_project_keys() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join(CONFIG_FILENAME);
        fs::write(&path, r#"{"index":{"exclude":["target"]}}"#).unwrap();
        write_project_share_config(tmp.path(), &ShareConfig::default()).unwrap();
        let root: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(root["index"]["exclude"][0], "target");
        assert!(root["share"].is_object());
    }

    #[test]
    fn share_settings_leave_no_lock_file_in_the_project() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("git-share.json");
        fs::write(project.path().join(CONFIG_FILENAME), "{}").unwrap();
        write_project_share_config_with(project.path(), &ShareConfig::default(), &store).unwrap();
        write_project_share_config_with(project.path(), &ShareConfig::default(), &store).unwrap();
        let names: Vec<String> = fs::read_dir(project.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec![CONFIG_FILENAME.to_string()], "{names:?}");
    }

    #[test]
    fn import_mode_flags() {
        assert_eq!(ShareImportMode::Review.import_flags(), (true, false));
        assert_eq!(ShareImportMode::Merge.import_flags(), (false, false));
        assert_eq!(ShareImportMode::Force.import_flags(), (false, true));
    }

    #[test]
    fn import_mode_parse_and_as_str() {
        assert_eq!(
            ShareImportMode::parse("review"),
            Some(ShareImportMode::Review)
        );
        assert_eq!(
            ShareImportMode::parse("merge"),
            Some(ShareImportMode::Merge)
        );
        assert_eq!(
            ShareImportMode::parse("force"),
            Some(ShareImportMode::Force)
        );
        assert_eq!(
            ShareImportMode::parse("overwrite"),
            Some(ShareImportMode::Force)
        );
        assert_eq!(ShareImportMode::parse("invalid"), None);
        assert_eq!(ShareImportMode::Review.as_str(), "review");
        assert_eq!(ShareImportMode::Merge.as_str(), "merge");
        assert_eq!(ShareImportMode::Force.as_str(), "force");
    }

    #[test]
    fn share_provider_parse() {
        assert_eq!(
            ShareProvider::parse("onedrive"),
            Some(ShareProvider::Onedrive)
        );
        assert_eq!(
            ShareProvider::parse("sharepoint"),
            Some(ShareProvider::Onedrive)
        );
        assert_eq!(ShareProvider::parse("github"), Some(ShareProvider::Github));
        assert_eq!(ShareProvider::parse("git"), Some(ShareProvider::Github));
        assert_eq!(ShareProvider::parse("dropbox"), None);
    }

    #[test]
    fn default_onedrive_url_is_empty() {
        assert_eq!(ShareConfig::default().onedrive.share_url, "");
        let project_dir = TempDir::new().unwrap();
        write_share_json(
            &project_dir.path().join(CONFIG_FILENAME),
            serde_json::json!({ "provider": "onedrive", "onedrive": { "shareUrl": "  " } }),
        );
        assert_eq!(
            load_from_project(project_dir.path())
                .onedrive
                .share_url
                .trim(),
            ""
        );
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
        assert_eq!(
            cfg.onedrive.share_url,
            "https://contoso.sharepoint.com/team"
        );
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
        assert_eq!(
            cfg.onedrive.share_url,
            "https://contoso.sharepoint.com/global"
        );
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
        assert_eq!(
            cfg.github.repo_url,
            "https://github.com/team/policy-pack.git"
        );
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

    fn github_with_token(token: &str) -> ShareConfig {
        let mut config = ShareConfig {
            provider: ShareProvider::Github,
            ..Default::default()
        };
        config.github.repo_url = "https://gitlab.example.com/team/policy.git".into();
        config.github.token = token.into();
        config
    }

    #[test]
    fn the_git_token_is_never_written_to_ax_json_nor_returned_by_the_api() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        write_project_share_config_with(
            project.path(),
            &github_with_token("glpat-secret-1"),
            &store,
        )
        .unwrap();
        let on_disk = fs::read_to_string(project.path().join(CONFIG_FILENAME)).unwrap();
        assert!(!on_disk.contains("glpat-secret-1"), "{on_disk}");
        assert!(!on_disk.contains("hasToken"), "{on_disk}");
        let api = serde_json::to_value(load_share_config_with(project.path(), &store)).unwrap();
        assert!(!api.to_string().contains("glpat-secret-1"), "{api}");
        assert_eq!(api["github"]["hasToken"], true, "{api}");
        // Sync still gets the token.
        assert_eq!(
            load_share_config_with(project.path(), &store).github.token,
            "glpat-secret-1"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&store).unwrap().permissions().mode();
            assert_eq!(
                mode & 0o077,
                0,
                "token store is readable by others: {mode:o}"
            );
        }
    }

    #[test]
    fn an_empty_token_keeps_the_stored_one_and_clear_token_removes_it() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        write_project_share_config_with(project.path(), &github_with_token("glpat-2"), &store)
            .unwrap();
        // The UI saves other settings without a token.
        write_project_share_config_with(project.path(), &github_with_token(""), &store).unwrap();
        assert_eq!(
            load_share_config_with(project.path(), &store).github.token,
            "glpat-2"
        );

        let mut clear = github_with_token("");
        clear.github.clear_token = true;
        write_project_share_config_with(project.path(), &clear, &store).unwrap();
        let cfg = load_share_config_with(project.path(), &store);
        assert_eq!(cfg.github.token, "");
        assert!(!cfg.github.has_token);
    }

    #[test]
    fn a_token_left_in_an_older_ax_json_moves_to_the_store_on_save() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        write_share_json(
            &project.path().join(CONFIG_FILENAME),
            serde_json::json!({
                "provider": "github",
                "github": { "repoUrl": "https://gitlab.example.com/team/policy.git", "token": "glpat-old" }
            }),
        );
        let cfg = load_share_config_with(project.path(), &store);
        assert_eq!(cfg.github.token, "glpat-old");
        write_project_share_config_with(project.path(), &cfg, &store).unwrap();
        let on_disk = fs::read_to_string(project.path().join(CONFIG_FILENAME)).unwrap();
        assert!(!on_disk.contains("glpat-old"), "{on_disk}");
        assert_eq!(
            load_share_config_with(project.path(), &store).github.token,
            "glpat-old"
        );
    }
}
