//! Remote policy share configuration — per-project `ax.json` only.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::secret::ShareTokenStore;

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
    /// serialized: it is kept in the OS credential store, keyed by project
    /// root and `repo_url`, and filled in when the config is loaded. An empty
    /// token on save keeps the stored one.
    #[serde(default, skip_serializing)]
    pub token: String,
    /// Output only: a token is stored for `repo_url`.
    #[serde(default, skip_deserializing, rename = "tokenSet")]
    pub token_set: bool,
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
            token_set: false,
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
/// git API token from the OS credential store.
pub fn load_share_config(project_root: &Path) -> ShareConfig {
    load_share_config_with(
        project_root,
        crate::secret::default_store(),
        &git_token_store_path(),
    )
}

/// [`load_share_config`] with an explicit credential store and an explicit
/// old token file (`~/.ax/auth/git-share.json`). Loading never writes.
pub fn load_share_config_with(
    project_root: &Path,
    secrets: &dyn ShareTokenStore,
    token_store: &Path,
) -> ShareConfig {
    let config_path = project_root.join(CONFIG_FILENAME);
    let cfg = ShareConfig::default();
    let mut cfg = match read_share_section(&config_path) {
        Some(local) => merge_share(cfg, local),
        None => cfg,
    };
    cfg.github.clear_token = false;
    cfg.github.token = load_git_token(
        project_root,
        &cfg.github.repo_url,
        cfg.github.token.trim(),
        secrets,
        token_store,
    );
    cfg.github.token_set = !cfg.github.token.is_empty();
    cfg
}

/// The git API token for `repo_url`, read without writing anything: loading
/// happens on plain reads (`GET /api/policy/share/config`, also in read-only
/// mode). The credential store comes first; a token still in `ax.json`
/// (`inline`) or in the old token file is used as it is until the next save
/// moves it (see [`migrate_old_token`]).
fn load_git_token(
    project_root: &Path,
    repo_url: &str,
    inline: &str,
    secrets: &dyn ShareTokenStore,
    token_store: &Path,
) -> String {
    if !inline.is_empty() {
        return inline.to_string();
    }
    let key = crate::secret::token_key(project_root, repo_url);
    match secrets.get(&key) {
        Ok(Some(token)) => token,
        Ok(None) => read_git_token(token_store, repo_url).unwrap_or_default(),
        Err(e) => {
            crate::secret::warn_unavailable_once(&e);
            read_git_token(token_store, repo_url).unwrap_or_default()
        }
    }
}

/// On a save without a new token, copies a token that only exists in an old
/// location into this project's credential-store entry.
///
/// A token in `ax.json` must move, because the save rewrites the `share`
/// section without it: when the credential store refuses it, the save fails
/// instead of dropping the token. It moves even when the store already holds
/// a different token, because loading prefers the `ax.json` one, so that is
/// the token sync has been using. A token in the old token file is copied but
/// the file entry stays, because that file is keyed by repo URL only and
/// other projects on the same URL may still read it; an explicit
/// `clearToken` removes it.
fn migrate_old_token(
    config_path: &Path,
    key: &str,
    repo_url: &str,
    secrets: &dyn ShareTokenStore,
    token_store: &Path,
) -> Result<(), String> {
    let inline = read_share_section(config_path)
        .and_then(|share| share.github)
        .map(|github| github.token.trim().to_string())
        .unwrap_or_default();
    let stored = secrets.get(key);
    if !inline.is_empty() {
        if matches!(&stored, Ok(Some(current)) if *current == inline) {
            return Ok(());
        }
        return secrets.set(key, &inline).map_err(String::from);
    }
    if let Ok(Some(_)) = stored {
        return Ok(());
    }
    if let Some(token) = read_git_token(token_store, repo_url) {
        if let Err(e) = secrets.set(key, &token) {
            crate::secret::warn_unavailable_once(&e);
        }
    }
    Ok(())
}

/// Path to the project share config file (`ax.json`).
pub fn project_config_path(project_root: &Path) -> PathBuf {
    project_root.join(CONFIG_FILENAME)
}

/// Old file that held git API tokens per repo URL (owner-only). A token found
/// there is read when a project has none in the credential store, and copied
/// there on that project's next save.
pub fn git_token_store_path() -> PathBuf {
    crate::paths::auth_dir().join("git-share.json")
}

fn old_token_key(repo_url: &str) -> String {
    repo_url.trim().to_string()
}

fn read_git_token(store: &Path, repo_url: &str) -> Option<String> {
    let text = std::fs::read_to_string(store).ok()?;
    let root: serde_json::Value = serde_json::from_str(&text).ok()?;
    root.get("tokens")?
        .get(old_token_key(repo_url))?
        .as_str()
        .map(str::to_string)
}

/// Sets (`Some`, tests only: ax no longer writes tokens to this file) or
/// removes (`None`) the token for `repo_url` in the old token file.
fn write_git_token(store: &Path, repo_url: &str, token: Option<&str>) -> Result<(), String> {
    if token.is_none() && !store.exists() {
        return Ok(());
    }
    if let Some(dir) = store.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        restrict(dir, 0o700);
    }
    let key = old_token_key(repo_url);
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

/// Save share config to project `ax.json`. A git API token goes to the OS
/// credential store instead, never into a plain-text file.
pub fn write_project_share_config(
    project_root: &Path,
    config: &ShareConfig,
) -> Result<PathBuf, String> {
    write_project_share_config_with(
        project_root,
        config,
        crate::secret::default_store(),
        &git_token_store_path(),
    )
}

/// [`write_project_share_config`] with an explicit credential store and old
/// token file.
pub fn write_project_share_config_with(
    project_root: &Path,
    config: &ShareConfig,
    secrets: &dyn ShareTokenStore,
    token_store: &Path,
) -> Result<PathBuf, String> {
    let path = project_config_path(project_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let github = &config.github;
    let key = crate::secret::token_key(project_root, &github.repo_url);
    // A token stored for the repo URL this save replaces is removed, not
    // moved: it belongs to that repository's host. Nothing else would ever
    // remove it, since every later lookup and clear uses the new URL.
    let previous_url = read_share_section(&path)
        .and_then(|share| share.github)
        .map(|github| github.repo_url.trim().to_string())
        .filter(|url| !url.is_empty() && url != github.repo_url.trim());
    let remove_previous = || match &previous_url {
        Some(url) => remove_token(secrets, &crate::secret::token_key(project_root, url)),
        None => Ok(()),
    };
    if github.clear_token {
        // The plain-text locations are always cleared. A store that may hold
        // the token and cannot delete it fails the save, since that token
        // would come back on the next load (also a store that is only locked
        // or busy right now). Only when this machine has no credential store
        // at all is there nothing to delete, and the clear succeeds.
        let deleted = match secrets.get(&key) {
            Ok(None) => Ok(()),
            Ok(Some(_)) | Err(_) => remove_token(secrets, &key),
        };
        let previous = remove_previous();
        write_git_token(token_store, &github.repo_url, None)?;
        write_share_at(&path, config)?;
        deleted?;
        previous?;
        return Ok(path);
    }
    remove_previous()?;
    if !github.token.trim().is_empty() {
        // No plain-text fallback: a token that cannot go to the credential
        // store is refused.
        secrets.set(&key, github.token.trim())?;
    } else {
        migrate_old_token(&path, &key, &github.repo_url, secrets, token_store)?;
    }
    write_share_at(&path, config)?;
    Ok(path)
}

/// Deletes the credential-store entry `key`. Only a machine without any
/// credential store counts as done (with a one-time warning); a store that
/// refuses the delete (locked, busy, denied) is an error.
fn remove_token(secrets: &dyn ShareTokenStore, key: &str) -> Result<(), String> {
    match secrets.delete(key) {
        Ok(()) => Ok(()),
        Err(e) if e.is_absent() => {
            crate::secret::warn_unavailable_once(&e);
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

fn write_share_at(path: &Path, config: &ShareConfig) -> Result<(), String> {
    let mut share = serde_json::to_value(config).map_err(|e| e.to_string())?;
    if let Some(github) = share.get_mut("github").and_then(|g| g.as_object_mut()) {
        github.remove("tokenSet");
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

    use crate::secret::{MemoryStore, SecretError};

    fn load_from_project(project: &Path) -> ShareConfig {
        let home = TempDir::new().unwrap();
        load_share_config_with(
            project,
            &MemoryStore::default(),
            &home.path().join("git-share.json"),
        )
    }

    /// A credential store that is not there (locked keychain, no secret
    /// service on a headless Linux box).
    struct Unavailable;
    fn unavailable() -> SecretError {
        SecretError::Absent("OS credential store unavailable".into())
    }
    impl ShareTokenStore for Unavailable {
        fn get(&self, _: &str) -> Result<Option<String>, SecretError> {
            Err(unavailable())
        }
        fn set(&self, _: &str, _: &str) -> Result<(), SecretError> {
            Err(unavailable())
        }
        fn delete(&self, _: &str) -> Result<(), SecretError> {
            Err(unavailable())
        }
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
        let secrets = MemoryStore::default();
        write_project_share_config_with(project.path(), &ShareConfig::default(), &secrets, &store)
            .unwrap();
        write_project_share_config_with(project.path(), &ShareConfig::default(), &secrets, &store)
            .unwrap();
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

    fn files_under(dir: &Path) -> String {
        let mut all = String::new();
        for entry in walkdir::WalkDir::new(dir).into_iter().flatten() {
            if entry.file_type().is_file() {
                all.push_str(&fs::read_to_string(entry.path()).unwrap_or_default());
            }
        }
        all
    }

    #[test]
    fn the_git_token_goes_to_the_credential_store_and_never_to_a_file_or_the_api() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let secrets = MemoryStore::default();
        write_project_share_config_with(
            project.path(),
            &github_with_token("glpat-secret-1"),
            &secrets,
            &store,
        )
        .unwrap();
        assert!(!files_under(project.path()).contains("glpat-secret-1"));
        assert!(!files_under(home.path()).contains("glpat-secret-1"));
        let key =
            crate::secret::token_key(project.path(), "https://gitlab.example.com/team/policy.git");
        assert_eq!(
            secrets.get(&key).unwrap().as_deref(),
            Some("glpat-secret-1")
        );

        let loaded = load_share_config_with(project.path(), &secrets, &store);
        // Sync still gets the token.
        assert_eq!(loaded.github.token, "glpat-secret-1");
        let api = serde_json::to_value(&loaded).unwrap();
        assert!(!api.to_string().contains("glpat-secret-1"), "{api}");
        assert_eq!(api["github"]["tokenSet"], true, "{api}");
        assert!(api["github"].get("hasToken").is_none(), "{api}");
        assert!(!fs::read_to_string(project.path().join(CONFIG_FILENAME))
            .unwrap()
            .contains("tokenSet"));
    }

    #[test]
    fn an_empty_token_keeps_the_stored_one_and_clear_token_removes_it() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let secrets = MemoryStore::default();
        write_project_share_config_with(
            project.path(),
            &github_with_token("glpat-2"),
            &secrets,
            &store,
        )
        .unwrap();
        // The UI saves other settings without a token.
        write_project_share_config_with(project.path(), &github_with_token(""), &secrets, &store)
            .unwrap();
        assert_eq!(
            load_share_config_with(project.path(), &secrets, &store)
                .github
                .token,
            "glpat-2"
        );

        let mut clear = github_with_token("");
        clear.github.clear_token = true;
        write_project_share_config_with(project.path(), &clear, &secrets, &store).unwrap();
        let cfg = load_share_config_with(project.path(), &secrets, &store);
        assert_eq!(cfg.github.token, "");
        assert!(!cfg.github.token_set);
    }

    /// A credential store that reads and stores but cannot delete (a locked
    /// keychain item).
    #[derive(Default)]
    struct DeleteFails(MemoryStore);
    impl ShareTokenStore for DeleteFails {
        fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
            self.0.get(key)
        }
        fn set(&self, key: &str, token: &str) -> Result<(), SecretError> {
            self.0.set(key, token)
        }
        fn delete(&self, _: &str) -> Result<(), SecretError> {
            Err(SecretError::Refused(
                "Cannot remove the token from the OS credential store".into(),
            ))
        }
    }

    #[test]
    fn loading_a_token_from_ax_json_writes_nothing_and_a_save_moves_it() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let secrets = MemoryStore::default();
        let repo = "https://gitlab.example.com/team/policy.git";
        let config_path = project.path().join(CONFIG_FILENAME);
        write_share_json(
            &config_path,
            serde_json::json!({
                "provider": "github",
                "github": { "repoUrl": repo, "token": "glpat-old" }
            }),
        );
        let before = fs::read(&config_path).unwrap();
        // A read (GET /api/policy/share/config, also in read-only mode) must
        // not touch ax.json or the credential store.
        let cfg = load_share_config_with(project.path(), &secrets, &store);
        assert_eq!(cfg.github.token, "glpat-old");
        assert!(cfg.github.token_set);
        assert_eq!(fs::read(&config_path).unwrap(), before);
        let key = crate::secret::token_key(project.path(), repo);
        assert_eq!(secrets.get(&key).unwrap(), None);
        assert!(!store.exists());

        // The next save (without a new token) moves it to the credential store.
        let mut save = cfg.clone();
        save.github.token = String::new();
        write_project_share_config_with(project.path(), &save, &secrets, &store).unwrap();
        let on_disk = fs::read_to_string(&config_path).unwrap();
        assert!(!on_disk.contains("glpat-old"), "{on_disk}");
        assert!(on_disk.contains("gitlab.example.com"), "{on_disk}");
        assert_eq!(secrets.get(&key).unwrap().as_deref(), Some("glpat-old"));
        assert_eq!(
            load_share_config_with(project.path(), &secrets, &store)
                .github
                .token,
            "glpat-old"
        );
    }

    /// The credential store already holds a token, and `ax.json` holds a
    /// different one (edited by hand). Loading uses the `ax.json` token, so
    /// the next save must move that one, not drop it for the older stored one.
    #[test]
    fn a_save_moves_an_ax_json_token_that_differs_from_the_stored_one() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let secrets = MemoryStore::default();
        let repo = "https://gitlab.example.com/team/policy.git";
        let key = crate::secret::token_key(project.path(), repo);
        secrets.set(&key, "glpat-stored").unwrap();
        let config_path = project.path().join(CONFIG_FILENAME);
        write_share_json(
            &config_path,
            serde_json::json!({
                "provider": "github",
                "github": { "repoUrl": repo, "token": "glpat-edited" }
            }),
        );
        let cfg = load_share_config_with(project.path(), &secrets, &store);
        assert_eq!(cfg.github.token, "glpat-edited");

        let mut save = cfg.clone();
        save.github.token = String::new();
        write_project_share_config_with(project.path(), &save, &secrets, &store).unwrap();
        let on_disk = fs::read_to_string(&config_path).unwrap();
        assert!(!on_disk.contains("glpat-edited"), "{on_disk}");
        assert_eq!(secrets.get(&key).unwrap().as_deref(), Some("glpat-edited"));
        assert_eq!(
            load_share_config_with(project.path(), &secrets, &store)
                .github
                .token,
            "glpat-edited"
        );
    }

    #[test]
    fn a_save_refuses_to_drop_a_token_in_ax_json_it_cannot_move() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let config_path = project.path().join(CONFIG_FILENAME);
        write_share_json(
            &config_path,
            serde_json::json!({
                "provider": "github",
                "github": { "repoUrl": "https://gitlab.example.com/team/policy.git", "token": "glpat-old" }
            }),
        );
        let before = fs::read(&config_path).unwrap();
        let err = write_project_share_config_with(
            project.path(),
            &github_with_token(""),
            &Unavailable,
            &store,
        )
        .unwrap_err();
        assert!(err.contains("credential store"), "{err}");
        assert_eq!(fs::read(&config_path).unwrap(), before);
    }

    #[test]
    fn a_token_in_the_old_token_file_is_read_without_writing() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let secrets = MemoryStore::default();
        let repo = "https://gitlab.example.com/team/policy.git";
        write_share_json(
            &project.path().join(CONFIG_FILENAME),
            serde_json::json!({ "provider": "github", "github": { "repoUrl": repo } }),
        );
        write_git_token(&store, repo, Some("glpat-file")).unwrap();
        let before = fs::read(&store).unwrap();
        let cfg = load_share_config_with(project.path(), &secrets, &store);
        assert_eq!(cfg.github.token, "glpat-file");
        assert_eq!(fs::read(&store).unwrap(), before);
        let key = crate::secret::token_key(project.path(), repo);
        assert_eq!(secrets.get(&key).unwrap(), None);

        // A save copies it to this project's credential-store entry.
        write_project_share_config_with(project.path(), &github_with_token(""), &secrets, &store)
            .unwrap();
        assert_eq!(secrets.get(&key).unwrap().as_deref(), Some("glpat-file"));
    }

    #[test]
    fn two_projects_on_one_repo_url_both_keep_the_old_file_token() {
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let secrets = MemoryStore::default();
        let repo = "https://gitlab.example.com/team/policy.git";
        write_git_token(&store, repo, Some("glpat-shared")).unwrap();
        let first = TempDir::new().unwrap();
        let second = TempDir::new().unwrap();
        for project in [&first, &second] {
            write_share_json(
                &project.path().join(CONFIG_FILENAME),
                serde_json::json!({ "provider": "github", "github": { "repoUrl": repo } }),
            );
        }
        // The first project loads and saves (which copies the token into its
        // own credential-store entry); the second must still find it.
        let cfg = load_share_config_with(first.path(), &secrets, &store);
        assert_eq!(cfg.github.token, "glpat-shared");
        write_project_share_config_with(first.path(), &github_with_token(""), &secrets, &store)
            .unwrap();
        let cfg = load_share_config_with(second.path(), &secrets, &store);
        assert_eq!(cfg.github.token, "glpat-shared");
        assert!(cfg.github.token_set);
    }

    #[test]
    fn clear_token_fails_when_the_credential_store_cannot_delete_it() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let secrets = DeleteFails::default();
        write_project_share_config_with(
            project.path(),
            &github_with_token("glpat-locked"),
            &secrets,
            &store,
        )
        .unwrap();
        let mut clear = github_with_token("");
        clear.github.clear_token = true;
        let err =
            write_project_share_config_with(project.path(), &clear, &secrets, &store).unwrap_err();
        assert!(err.contains("credential store"), "{err}");
    }

    /// With no credential store at all (headless Linux, no Secret Service)
    /// there is nothing to delete there, so clearing the token succeeds once
    /// ax.json and the old token file no longer hold it.
    #[test]
    fn clear_token_succeeds_without_a_credential_store() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let repo = "https://gitlab.example.com/team/policy.git";
        write_share_json(
            &project.path().join(CONFIG_FILENAME),
            serde_json::json!({
                "provider": "github",
                "github": { "repoUrl": repo, "token": "glpat-old" }
            }),
        );
        write_git_token(&store, repo, Some("glpat-file")).unwrap();

        let mut clear = github_with_token("");
        clear.github.repo_url = repo.into();
        clear.github.clear_token = true;
        write_project_share_config_with(project.path(), &clear, &Unavailable, &store)
            .expect("clearing must not need a credential store that is not there");

        let cfg = load_share_config_with(project.path(), &Unavailable, &store);
        assert_eq!(cfg.github.token, "");
        assert!(!cfg.github.token_set);
        let raw = fs::read_to_string(project.path().join(CONFIG_FILENAME)).unwrap();
        assert!(!raw.contains("glpat-old"), "{raw}");
    }

    /// A credential store that holds tokens but can be locked (a locked macOS
    /// keychain, a locked or busy Secret Service): while locked every call
    /// fails, and the tokens are all there again once it is unlocked.
    #[derive(Default)]
    struct Lockable {
        tokens: MemoryStore,
        locked: std::sync::atomic::AtomicBool,
    }
    impl Lockable {
        fn set_locked(&self, locked: bool) {
            self.locked
                .store(locked, std::sync::atomic::Ordering::SeqCst);
        }
        fn check(&self) -> Result<(), SecretError> {
            if self.locked.load(std::sync::atomic::Ordering::SeqCst) {
                Err(SecretError::Refused(
                    "Cannot read the OS credential store; unlock it and try again".into(),
                ))
            } else {
                Ok(())
            }
        }
    }
    impl ShareTokenStore for Lockable {
        fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
            self.check()?;
            self.tokens.get(key)
        }
        fn set(&self, key: &str, token: &str) -> Result<(), SecretError> {
            self.check()?;
            self.tokens.set(key, token)
        }
        fn delete(&self, key: &str) -> Result<(), SecretError> {
            self.check()?;
            self.tokens.delete(key)
        }
    }

    /// Clearing while the store is locked must not report success: the token
    /// is still in the store and would be loaded again once it is unlocked.
    #[test]
    fn clear_token_fails_while_the_credential_store_is_locked() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let secrets = Lockable::default();
        write_project_share_config_with(
            project.path(),
            &github_with_token("glpat-locked"),
            &secrets,
            &store,
        )
        .unwrap();

        secrets.set_locked(true);
        let mut clear = github_with_token("");
        clear.github.clear_token = true;
        let err = write_project_share_config_with(project.path(), &clear, &secrets, &store)
            .expect_err("a clear the locked store did not carry out must fail");
        assert!(err.contains("credential store"), "{err}");

        // Once unlocked, clearing again removes it for good.
        secrets.set_locked(false);
        write_project_share_config_with(project.path(), &clear, &secrets, &store).unwrap();
        let cfg = load_share_config_with(project.path(), &secrets, &store);
        assert_eq!(cfg.github.token, "");
        assert!(!cfg.github.token_set);
    }

    #[test]
    fn changing_the_repo_url_removes_the_token_stored_for_the_old_url() {
        let project = TempDir::new().unwrap();
        let home = TempDir::new().unwrap();
        let store = home.path().join("auth/git-share.json");
        let secrets = MemoryStore::default();
        let old_url = "https://gitlab.example.com/team/policy.git";
        let new_url = "https://gitlab.example.com/team/other.git";
        let old_key = crate::secret::token_key(project.path(), old_url);
        write_project_share_config_with(
            project.path(),
            &github_with_token("glpat-a"),
            &secrets,
            &store,
        )
        .unwrap();
        assert_eq!(secrets.get(&old_key).unwrap().as_deref(), Some("glpat-a"));

        // A save on another URL, with or without a new token, leaves no token
        // for the old URL behind; the old token is never moved to the new URL.
        let mut moved = github_with_token("");
        moved.github.repo_url = new_url.into();
        write_project_share_config_with(project.path(), &moved, &secrets, &store).unwrap();
        assert_eq!(secrets.get(&old_key).unwrap(), None);
        assert!(
            !load_share_config_with(project.path(), &secrets, &store)
                .github
                .token_set
        );

        // The same holds for a clear that also changes the URL.
        let new_key = crate::secret::token_key(project.path(), new_url);
        moved.github.token = "glpat-b".into();
        write_project_share_config_with(project.path(), &moved, &secrets, &store).unwrap();
        let mut clear = github_with_token("");
        clear.github.clear_token = true;
        write_project_share_config_with(project.path(), &clear, &secrets, &store).unwrap();
        assert_eq!(secrets.get(&new_key).unwrap(), None);
        assert_eq!(secrets.get(&old_key).unwrap(), None);
    }

    #[test]
    fn without_a_credential_store_the_old_token_is_read_and_no_new_token_is_written() {
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
        let cfg = load_share_config_with(project.path(), &Unavailable, &store);
        assert_eq!(cfg.github.token, "glpat-old");
        assert!(cfg.github.token_set);

        let err = write_project_share_config_with(
            project.path(),
            &github_with_token("glpat-new"),
            &Unavailable,
            &store,
        )
        .unwrap_err();
        assert!(err.contains("credential store"), "{err}");
        assert!(!files_under(project.path()).contains("glpat-new"));
        assert!(!files_under(home.path()).contains("glpat-new"));
    }
}
