//! SonarQube first-time setup: project creation, token generation, local persistence.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;

const DEFAULT_ADMIN_USER: &str = "admin";
const DEFAULT_ADMIN_PASSWORD: &str = "admin";
const DEFAULT_TOKEN_NAME: &str = "ax-ship";

const PLACEHOLDER_PROJECT_KEYS: &[&str] = &[
    "your-project",
    "your_project",
    "ax-project",
    "<your-project>",
];

/// Result of a SonarQube project lookup (`found` | `missing` | `auth_failed` | `unreachable`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectLookup {
    Found,
    Missing,
    AuthFailed,
    Unreachable,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RepoProjectStatus {
    pub key: String,
    pub name: String,
    pub exists: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SonarSetupStatus {
    pub login_user: String,
    #[serde(skip_serializing)]
    pub login_password: String,
    pub login_password_hint: String,
    pub project_exists: bool,
    /// Structured lookup result for UI (distinguishes missing vs auth failure).
    pub project_lookup: ProjectLookup,
    /// One SonarQube project per git repo under a multi-repo workspace.
    #[serde(default)]
    pub repo_projects: Vec<RepoProjectStatus>,
    pub token_configured: bool,
    /// `Some(false)` when a token is present but SonarQube rejects it.
    pub token_valid: Option<bool>,
    pub scanner_available: bool,
    pub token_path: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SonarBootstrapResult {
    pub project_created: bool,
    pub project_key: String,
    pub project_name: String,
    #[serde(default)]
    pub projects_created: usize,
    #[serde(default)]
    pub repo_projects: Vec<RepoProjectStatus>,
    pub token_saved: bool,
    pub token_env_set: bool,
    pub ui_url: String,
    pub login_user: String,
    pub login_password_hint: String,
    pub token_path: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct SonarBootstrapConfig {
    pub host: String,
    pub project_key: String,
    pub project_name: String,
    pub admin_user: String,
    pub admin_password: String,
    pub token_name: String,
    pub token_env: String,
    /// `[sonar] migrate_legacy_keys` in `.ax/ship.toml`: adopt existing
    /// `<folder>-<repo>` projects as ax-created so they can be renamed.
    pub migrate_legacy_keys: bool,
}

impl SonarBootstrapConfig {
    pub fn from_sonar(host: &str, project_key: &str, token_env: &str) -> Self {
        Self::from_project(host, project_key, project_key, token_env)
    }

    pub fn from_project(
        host: &str,
        project_key: &str,
        project_name: &str,
        token_env: &str,
    ) -> Self {
        Self {
            host: host.trim_end_matches('/').to_string(),
            project_key: project_key.to_string(),
            project_name: project_name.to_string(),
            admin_user: DEFAULT_ADMIN_USER.into(),
            admin_password: DEFAULT_ADMIN_PASSWORD.into(),
            token_name: DEFAULT_TOKEN_NAME.into(),
            token_env: token_env.to_string(),
            migrate_legacy_keys: false,
        }
    }

    /// Resolve Sonar project key/name from ship.toml + project root directory.
    pub fn resolve_for_project(sonar: &crate::sonar::SonarConfig, project_root: &Path) -> Self {
        let (project_key, project_name) = resolve_sonar_project(&sonar.project_key, project_root);
        Self {
            host: sonar.host.trim_end_matches('/').to_string(),
            project_key,
            project_name,
            admin_user: sonar.admin_user.clone(),
            admin_password: sonar.admin_password.clone(),
            token_name: DEFAULT_TOKEN_NAME.into(),
            token_env: sonar.token_env.clone(),
            migrate_legacy_keys: sonar.migrate_legacy_keys,
        }
    }
}

/// Sanitize a directory name into a SonarQube-safe project key.
pub fn sonar_key_from_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = s.trim_matches('-').trim_matches('.').to_string();
    if trimmed.is_empty() {
        "ax-project".into()
    } else {
        trimmed
    }
}

/// Workspace-level Sonar key from ship.toml (or folder name when placeholder).
pub fn workspace_sonar_key(configured_key: &str, project_root: &Path) -> String {
    resolve_sonar_project(configured_key, project_root).0
}

/// Sonar project key + display name for each git repo in a multi-repo workspace.
pub fn resolve_sonar_repo_projects(
    workspace_key: &str,
    repo_names: &[String],
) -> Vec<RepoProjectStatus> {
    let multi_repo = repo_names.len() > 1;
    repo_names
        .iter()
        .map(|repo| {
            let key = canonical_repo_project_key(workspace_key, repo, multi_repo);
            RepoProjectStatus {
                key,
                name: repo.clone(),
                exists: false,
            }
        })
        .collect()
}

/// Canonical SonarQube project key for a git repo folder under a workspace.
pub fn canonical_repo_project_key(
    workspace_key: &str,
    repo_name: &str,
    multi_repo: bool,
) -> String {
    if multi_repo {
        format!("{workspace_key}-{}", sonar_key_from_name(repo_name))
    } else {
        workspace_key.to_string()
    }
}

/// Legacy workspace prefixes that may have been used before `project_key` changed (e.g. Contoso → ax).
pub fn legacy_workspace_prefixes(workspace_key: &str, project_root: &Path) -> Vec<String> {
    let folder_slug = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .map(sonar_key_from_name)
        .unwrap_or_default();
    if folder_slug.is_empty() || folder_slug == workspace_key {
        return Vec::new();
    }
    vec![folder_slug]
}

/// Use the configured key unless it is a template placeholder — then use the project folder name.
pub fn resolve_sonar_project(configured_key: &str, project_root: &Path) -> (String, String) {
    let configured = configured_key.trim();
    let folder_name = project_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("ax-project");

    let use_folder = configured.is_empty()
        || PLACEHOLDER_PROJECT_KEYS
            .iter()
            .any(|p| configured.eq_ignore_ascii_case(p));

    if use_folder {
        let key = sonar_key_from_name(folder_name);
        let name = folder_name.to_string();
        (key, name)
    } else {
        (configured.to_string(), configured.to_string())
    }
}

#[derive(Deserialize)]
struct ProjectSearchResponse {
    components: Vec<ProjectComponent>,
}

#[derive(Deserialize)]
struct ProjectComponent {
    key: String,
}

#[derive(Deserialize)]
struct TokenGenerateResponse {
    token: String,
}

pub fn sonar_token_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("sonar.token")
}

pub fn read_sonar_token(project_root: &Path, token_env: &str) -> Option<String> {
    let from_file = std::fs::read_to_string(sonar_token_path(project_root))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if from_file.is_some() {
        return from_file;
    }
    std::env::var(token_env)
        .ok()
        .filter(|s| !s.trim().is_empty())
}

pub fn token_configured(project_root: &Path, token_env: &str) -> bool {
    read_sonar_token(project_root, token_env).is_some()
}

pub fn scanner_available(scanner_path: &str) -> bool {
    std::process::Command::new(scanner_path)
        .arg("-h")
        .output()
        .map(|o| o.status.success() || !o.stdout.is_empty() || !o.stderr.is_empty())
        .unwrap_or(false)
}

pub async fn inspect_setup(
    config: &SonarBootstrapConfig,
    project_root: &Path,
    scanner_path: &str,
    repo_names: &[String],
) -> Result<SonarSetupStatus, String> {
    let token = read_sonar_token(project_root, &config.token_env);
    let workspace_key = workspace_sonar_key(&config.project_key, project_root);
    let mut repo_projects = resolve_sonar_repo_projects(&workspace_key, repo_names);

    if !sonar_reachable(&config.host).await {
        return Ok(SonarSetupStatus {
            login_user: config.admin_user.clone(),
            login_password: config.admin_password.clone(),
            login_password_hint:
                "Default local admin credentials in .ax/ship.toml — applied automatically by ax."
                    .into(),
            project_exists: false,
            project_lookup: ProjectLookup::Unreachable,
            repo_projects,
            token_configured: token.is_some(),
            token_valid: None,
            scanner_available: scanner_available(scanner_path),
            token_path: sonar_token_path(project_root).display().to_string(),
        });
    }

    let keys: Vec<String> = if repo_projects.is_empty() {
        vec![workspace_key.clone()]
    } else {
        repo_projects.iter().map(|p| p.key.clone()).collect()
    };
    let (lookup_summary, exists_by_key) = lookup_projects_reachable(
        &config.host,
        &keys,
        token.as_deref(),
        &config.admin_user,
        &config.admin_password,
    )
    .await;

    for project in &mut repo_projects {
        project.exists = exists_by_key.get(&project.key).copied().unwrap_or(false);
    }

    let project_lookup = if lookup_summary == ProjectLookup::Unreachable {
        ProjectLookup::Unreachable
    } else if repo_projects.is_empty() {
        lookup_summary
    } else if repo_projects.iter().all(|p| p.exists) {
        ProjectLookup::Found
    } else if repo_projects.iter().any(|p| p.exists) {
        ProjectLookup::Missing
    } else {
        lookup_summary
    };

    let project_exists = if repo_projects.is_empty() {
        project_lookup == ProjectLookup::Found
    } else {
        repo_projects.iter().all(|p| p.exists)
    };

    let token_valid = if token.is_some() {
        Some(validate_sonar_token(&config.host, token.as_deref().unwrap()).await)
    } else {
        None
    };

    Ok(SonarSetupStatus {
        login_user: config.admin_user.clone(),
        login_password: config.admin_password.clone(),
        login_password_hint:
            "Default local admin credentials in .ax/ship.toml — applied automatically by ax.".into(),
        project_exists,
        project_lookup,
        repo_projects,
        token_configured: token.is_some(),
        token_valid,
        scanner_available: scanner_available(scanner_path),
        token_path: sonar_token_path(project_root).display().to_string(),
    })
}

pub async fn bootstrap_sonar(
    config: &SonarBootstrapConfig,
    project_root: &Path,
    repo_names: &[String],
    stack: Option<&crate::sonar::SonarConfig>,
) -> Result<SonarBootstrapResult, String> {
    if !sonar_reachable(&config.host).await {
        let Some(sonar) = stack else {
            return Err(format!(
                "SonarQube is not reachable at {}. Install & start the container first.",
                config.host
            ));
        };
        let log = crate::container::InstallLog::new();
        crate::container::ensure_sonar_stack_online(sonar, &log).await?;
    }

    if !check_login(&config.host, &config.admin_user, &config.admin_password).await? {
        return Err(format!(
            "SonarQube login failed for user '{}'. Ensure the local container is running and reset the stack if credentials were changed outside ax.",
            config.admin_user
        ));
    }

    ensure_sonar_dark_theme(&config.host, &config.admin_user, &config.admin_password).await;

    let (projects_created, repo_projects, _) =
        ensure_projects_reachable(config, project_root, repo_names).await?;

    let token = generate_token(
        &config.host,
        &config.token_name,
        &config.admin_user,
        &config.admin_password,
    )
    .await?;

    let (token_path, token_env_set) = persist_token(project_root, &config.token_env, &token)?;

    let message = if projects_created > 0 {
        format!("Created {projects_created} SonarQube project(s) and saved scanner token.")
    } else {
        "SonarQube project(s) already existed; generated and saved a new scanner token.".into()
    };

    Ok(SonarBootstrapResult {
        project_created: projects_created > 0,
        project_key: config.project_key.clone(),
        project_name: config.project_name.clone(),
        projects_created,
        repo_projects,
        token_saved: true,
        token_env_set,
        ui_url: config.host.clone(),
        login_user: config.admin_user.clone(),
        login_password_hint:
            "Default local admin credentials in .ax/ship.toml — applied automatically by ax.".into(),
        token_path: token_path.display().to_string(),
        message,
    })
}

/// Create any missing SonarQube projects for discovered git repos (idempotent).
pub async fn ensure_sonar_projects(
    config: &SonarBootstrapConfig,
    project_root: &Path,
    repo_names: &[String],
) -> Result<(usize, Vec<RepoProjectStatus>, Vec<String>), String> {
    if !sonar_reachable(&config.host).await {
        return Err(format!(
            "SonarQube is not reachable at {}. Install & start the container first.",
            config.host
        ));
    }
    ensure_projects_reachable(config, project_root, repo_names).await
}

/// [`ensure_sonar_projects`] for a caller that has just seen SonarQube up.
async fn ensure_projects_reachable(
    config: &SonarBootstrapConfig,
    project_root: &Path,
    repo_names: &[String],
) -> Result<(usize, Vec<RepoProjectStatus>, Vec<String>), String> {
    let token = read_sonar_token(project_root, &config.token_env);
    let token_ok = match token.as_deref() {
        Some(t) => validate_sonar_token(&config.host, t).await,
        None => false,
    };
    let admin_ok = check_login(&config.host, &config.admin_user, &config.admin_password).await?;

    if !admin_ok && !token_ok {
        return Err(format!(
            "SonarQube auth failed: admin user '{}' rejected and no valid scanner token. \
             Reinstall the local stack or run Setup project & token if credentials were changed outside ax.",
            config.admin_user
        ));
    }

    let workspace_key = workspace_sonar_key(&config.project_key, project_root);
    let mut repo_projects = resolve_sonar_repo_projects(&workspace_key, repo_names);
    let mut projects_created = 0usize;
    let mut newly_created_repos = Vec::new();

    if !repo_projects.is_empty() {
        let migration_log = migrate_legacy_sonar_projects(
            config,
            project_root,
            repo_names,
            token.as_deref(),
            admin_ok,
        )
        .await?;
        for line in migration_log {
            tracing::info!(message = %line, "SonarQube project migration");
        }
    }

    if repo_projects.is_empty() {
        if lookup_projects_reachable(
            &config.host,
            std::slice::from_ref(&workspace_key),
            token.as_deref(),
            &config.admin_user,
            &config.admin_password,
        )
        .await
        .0 != ProjectLookup::Found
        {
            if !admin_ok {
                return Err(
            "SonarQube admin login required to create projects. Reinstall the local stack or reset admin credentials in .ax/ship.toml."
                        .into(),
                );
            }
            create_project(
                &config.host,
                &workspace_key,
                &config.project_name,
                &config.admin_user,
                &config.admin_password,
                token.as_deref(),
            )
            .await?;
            projects_created = 1;
            note_created_project(project_root, &workspace_key);
        }
        return Ok((projects_created, vec![], newly_created_repos));
    }

    let keys: Vec<String> = repo_projects.iter().map(|p| p.key.clone()).collect();
    let (lookup_summary, exists_by_key) = lookup_projects_reachable(
        &config.host,
        &keys,
        token.as_deref(),
        &config.admin_user,
        &config.admin_password,
    )
    .await;
    if lookup_summary == ProjectLookup::Unreachable {
        return Err(format!(
            "SonarQube is not reachable at {}. Install & start the container first.",
            config.host
        ));
    }

    for project in &mut repo_projects {
        if exists_by_key.get(&project.key).copied().unwrap_or(false) {
            project.exists = true;
            continue;
        }
        if !admin_ok {
            return Err(format!(
                "SonarQube project '{}' is missing and admin login is required to create it.",
                project.key
            ));
        }
        create_project(
            &config.host,
            &project.key,
            &project.name,
            &config.admin_user,
            &config.admin_password,
            token.as_deref(),
        )
        .await?;
        project.exists = true;
        note_created_project(project_root, &project.key);
        projects_created += 1;
        newly_created_repos.push(project.name.clone());
    }

    Ok((projects_created, repo_projects, newly_created_repos))
}

/// Ensure scanner token exists and is valid; generate one when missing or rejected.
pub async fn ensure_sonar_token(
    config: &SonarBootstrapConfig,
    project_root: &Path,
) -> Result<(), String> {
    if !sonar_reachable(&config.host).await {
        return Err("SonarQube is not reachable".into());
    }
    ensure_token_reachable(config, project_root).await
}

/// [`ensure_sonar_token`] for a caller that has just seen SonarQube up.
async fn ensure_token_reachable(
    config: &SonarBootstrapConfig,
    project_root: &Path,
) -> Result<(), String> {
    if let Some(token) = read_sonar_token(project_root, &config.token_env) {
        if validate_sonar_token(&config.host, &token).await {
            return Ok(());
        }
    }

    if !check_login(&config.host, &config.admin_user, &config.admin_password).await? {
        return Err(format!(
            "SonarQube login failed for user '{}'. Reset the local stack if credentials were changed outside ax.",
            config.admin_user
        ));
    }

    let token = generate_token(
        &config.host,
        &config.token_name,
        &config.admin_user,
        &config.admin_password,
    )
    .await?;

    persist_token(project_root, &config.token_env, &token)?;
    Ok(())
}

/// Idempotent Sonar provisioning driven by autodiscovered git repositories.
pub async fn auto_provision_sonar_from_discovery(
    sonar: &crate::sonar::SonarConfig,
    project_root: &Path,
    repo_names: &[String],
) -> Result<(), String> {
    if !sonar.enabled || repo_names.is_empty() {
        return Ok(());
    }
    let config = SonarBootstrapConfig::resolve_for_project(sonar, project_root);
    if !sonar_reachable(&config.host).await {
        return Ok(());
    }
    tracing::info!(
        repos = repo_names.len(),
        "Auto-provisioning SonarQube projects from discovered git repos"
    );
    ready_for_scan_reachable(&config, project_root, repo_names).await
}

/// Prepare SonarQube before a quality-gate scan: projects per git repo + scanner token.
pub async fn ensure_sonar_ready_for_scan(
    sonar: &crate::sonar::SonarConfig,
    project_root: &Path,
    repo_names: &[String],
) -> Result<(), String> {
    if !sonar.enabled {
        return Ok(());
    }
    let config = SonarBootstrapConfig::resolve_for_project(sonar, project_root);
    if !sonar_reachable(&config.host).await {
        return Err(format!(
            "SonarQube is not reachable at {}. Install and start it from the SonarQube page (Setup tab).",
            config.host
        ));
    }
    ready_for_scan_reachable(&config, project_root, repo_names).await
}

/// One reachability check per entry point: the steps below assume SonarQube is up.
async fn ready_for_scan_reachable(
    config: &SonarBootstrapConfig,
    project_root: &Path,
    repo_names: &[String],
) -> Result<(), String> {
    ensure_sonar_dark_theme(&config.host, &config.admin_user, &config.admin_password).await;
    ensure_projects_reachable(config, project_root, repo_names).await?;
    ensure_token_reachable(config, project_root).await?;
    Ok(())
}

/// Generate a fresh scanner token and persist it (`.ax/sonar.token` + env).
pub async fn regenerate_sonar_token(
    config: &SonarBootstrapConfig,
    project_root: &Path,
) -> Result<SonarBootstrapResult, String> {
    if !sonar_reachable(&config.host).await {
        return Err(format!(
            "SonarQube is not reachable at {}. Start the container first.",
            config.host
        ));
    }

    if !check_login(&config.host, &config.admin_user, &config.admin_password).await? {
        return Err(format!(
            "SonarQube login failed for user '{}'. Ensure the local container is running.",
            config.admin_user
        ));
    }

    let token = generate_token(
        &config.host,
        &config.token_name,
        &config.admin_user,
        &config.admin_password,
    )
    .await?;

    let (token_path, token_env_set) = persist_token(project_root, &config.token_env, &token)?;

    Ok(SonarBootstrapResult {
        project_created: false,
        project_key: config.project_key.clone(),
        project_name: config.project_name.clone(),
        projects_created: 0,
        repo_projects: vec![],
        token_saved: true,
        token_env_set,
        ui_url: config.host.clone(),
        login_user: config.admin_user.clone(),
        login_password_hint:
            "Default local admin credentials in .ax/ship.toml — applied automatically by ax.".into(),
        token_path: token_path.display().to_string(),
        message: "Regenerated and saved scanner token.".into(),
    })
}

/// Set SonarQube UI to dark theme for the admin user (best-effort, idempotent).
pub async fn ensure_sonar_dark_theme(host: &str, user: &str, password: &str) {
    let host = host.trim_end_matches('/');
    let prefs = [
        ("appearance.theme", "dark"),
        ("sonar.ui.theme", "dark"),
        ("theme", "dark"),
        ("user.theme", "dark"),
    ];
    for (key, value) in prefs {
        let url = format!(
            "{host}/api/user_preferences/set?login={}&key={}&value={}",
            encode_query(user),
            encode_query(key),
            encode_query(value),
        );
        if let Ok(resp) = authed_post(&url, user, password).await {
            if resp.status().is_success() {
                tracing::debug!(preference = key, "SonarQube dark theme preference set");
            }
        }
    }
    tracing::debug!(
        "SonarQube dark theme preferences applied (proxy also injects dark theme in HTML)"
    );
}

/// Whether SonarQube at `host` is `UP` and can serve API calls (a server
/// that is still `STARTING` does not count).
pub async fn sonar_reachable(host: &str) -> bool {
    crate::container::sonar_status_up(http_client(), host).await
}

/// One client for every SonarQube call, so requests share its connection pool.
fn http_client() -> &'static reqwest::Client {
    static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}

/// Check SonarQube web UI credentials against `/api/authentication/validate`.
pub async fn validate_sonar_login(host: &str, user: &str, password: &str) -> Result<bool, String> {
    let host = host.trim_end_matches('/');
    if !sonar_reachable(host).await {
        return Err("SonarQube is not reachable".into());
    }
    check_login(host, user, password).await
}

/// [`validate_sonar_login`] for a caller that has just seen SonarQube up.
async fn check_login(host: &str, user: &str, password: &str) -> Result<bool, String> {
    let host = host.trim_end_matches('/');
    let url = format!("{host}/api/authentication/validate");
    let resp = authed_get(&url, user, password).await?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(false);
    }
    if !resp.status().is_success() {
        return Err(format!(
            "SonarQube login check failed: HTTP {}",
            resp.status()
        ));
    }
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body.get("valid").and_then(|v| v.as_bool()).unwrap_or(false))
}

/// Look up one or more projects — scanner token first, then admin basic auth.
pub async fn lookup_projects(
    host: &str,
    project_keys: &[String],
    token: Option<&str>,
    user: &str,
    password: &str,
) -> (ProjectLookup, HashMap<String, bool>) {
    if project_keys.is_empty() {
        return (ProjectLookup::Missing, HashMap::new());
    }
    if !sonar_reachable(host).await {
        return (ProjectLookup::Unreachable, HashMap::new());
    }
    lookup_projects_reachable(host, project_keys, token, user, password).await
}

/// [`lookup_projects`] for a caller that has just seen SonarQube up.
async fn lookup_projects_reachable(
    host: &str,
    project_keys: &[String],
    token: Option<&str>,
    user: &str,
    password: &str,
) -> (ProjectLookup, HashMap<String, bool>) {
    let mut exists = HashMap::new();
    if project_keys.is_empty() {
        return (ProjectLookup::Missing, exists);
    }
    if let Some(t) = token.filter(|s| !s.trim().is_empty()) {
        match search_projects_batch(host, project_keys, Auth::Bearer(t)).await {
            Ok(found) => {
                for key in project_keys {
                    exists.insert(key.clone(), found.contains(key));
                }
                let summary = summarize_project_lookup(project_keys, &exists);
                return (summary, exists);
            }
            Err(LookupError::AuthFailed) => { /* fall through to admin */ }
            Err(LookupError::Request(e)) => {
                tracing::debug!(error = %e, "SonarQube project lookup (token) failed");
                if is_connection_error(&e) {
                    return (ProjectLookup::Unreachable, exists);
                }
            }
        }
    }

    match search_projects_batch(host, project_keys, Auth::Basic(user, password)).await {
        Ok(found) => {
            for key in project_keys {
                exists.insert(key.clone(), found.contains(key));
            }
            (summarize_project_lookup(project_keys, &exists), exists)
        }
        Err(LookupError::AuthFailed) => (ProjectLookup::AuthFailed, exists),
        Err(LookupError::Request(e)) => {
            tracing::debug!(error = %e, "SonarQube project lookup (admin) failed");
            if is_connection_error(&e) {
                (ProjectLookup::Unreachable, exists)
            } else {
                (ProjectLookup::AuthFailed, exists)
            }
        }
    }
}

/// Look up a project by key — scanner token first, then admin basic auth.
pub async fn lookup_project(
    host: &str,
    project_key: &str,
    token: Option<&str>,
    user: &str,
    password: &str,
) -> ProjectLookup {
    lookup_projects(host, &[project_key.to_string()], token, user, password)
        .await
        .0
}

fn summarize_project_lookup(
    project_keys: &[String],
    exists: &HashMap<String, bool>,
) -> ProjectLookup {
    if project_keys
        .iter()
        .all(|k| exists.get(k).copied().unwrap_or(false))
    {
        ProjectLookup::Found
    } else {
        ProjectLookup::Missing
    }
}

fn is_connection_error(err: &str) -> bool {
    err.contains("error sending request")
        || err.contains("connection refused")
        || err.contains("timed out")
        || err.contains("timeout")
        || err.contains("dns error")
}

/// Returns whether SonarQube accepts the scanner token.
/// SonarQube 9.x: user token as HTTP Basic username (empty password). 10+: Bearer also works.
pub async fn validate_sonar_token(host: &str, token: &str) -> bool {
    let host = host.trim_end_matches('/');
    let token = token.trim();
    if token.is_empty() {
        return false;
    }

    let validate_url = format!("{host}/api/authentication/validate");
    if let Ok(resp) = token_authed_get(&validate_url, token).await {
        if resp.status().is_success() {
            if let Ok(body) = resp.json::<serde_json::Value>().await {
                if body.get("valid").and_then(|v| v.as_bool()) == Some(true) {
                    return true;
                }
            }
        }
    }

    let current_url = format!("{host}/api/users/current");
    token_authed_get(&current_url, token)
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

enum Auth<'a> {
    Bearer(&'a str),
    Basic(&'a str, &'a str),
}

enum LookupError {
    AuthFailed,
    Request(String),
}

async fn search_projects_batch(
    host: &str,
    project_keys: &[String],
    auth: Auth<'_>,
) -> Result<HashSet<String>, LookupError> {
    let host = host.trim_end_matches('/');
    let keys_param = project_keys
        .iter()
        .map(|k| encode_query(k))
        .collect::<Vec<_>>()
        .join(",");
    let url = format!("{host}/api/projects/search?projects={keys_param}");
    let resp = match auth {
        Auth::Bearer(token) => token_authed_get(&url, token).await,
        Auth::Basic(user, password) => authed_get(&url, user, password).await,
    }
    .map_err(LookupError::Request)?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(LookupError::AuthFailed);
    }
    if !resp.status().is_success() {
        return Err(LookupError::Request(format!(
            "SonarQube project search failed: HTTP {}",
            resp.status()
        )));
    }
    let body: ProjectSearchResponse = resp
        .json()
        .await
        .map_err(|e| LookupError::Request(e.to_string()))?;
    Ok(body.components.into_iter().map(|c| c.key).collect())
}

/// Where ax records the SonarQube project keys it created itself.
fn created_projects_path(project_root: &Path) -> PathBuf {
    project_root.join(".ax").join("sonar-projects.json")
}

#[derive(Default, serde::Serialize, Deserialize)]
struct CreatedProjects {
    #[serde(default)]
    created: Vec<String>,
}

/// The SonarQube project keys ax created for this workspace.
fn created_projects(project_root: &Path) -> HashSet<String> {
    ax_utils::read_bounded(&created_projects_path(project_root), 1024 * 1024)
        .ok()
        .and_then(|raw| serde_json::from_slice::<CreatedProjects>(&raw).ok())
        .map(|r| r.created.into_iter().collect())
        .unwrap_or_default()
}

/// Adds `add` to and removes `remove` from the record of projects ax created.
fn update_created_projects(
    project_root: &Path,
    add: &[String],
    remove: &[String],
) -> std::io::Result<()> {
    ax_utils::update_locked(&created_projects_path(project_root), |current| {
        let mut record: CreatedProjects = current
            .and_then(|raw| serde_json::from_slice(&raw).ok())
            .unwrap_or_default();
        record.created.retain(|k| !remove.contains(k));
        for key in add {
            if !record.created.contains(key) {
                record.created.push(key.clone());
            }
        }
        record.created.sort();
        let mut out = serde_json::to_vec_pretty(&record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        out.push(b'\n');
        Ok(Some(out))
    })
    .map(|_| ())
}

fn record_created_projects(project_root: &Path, keys: &[String]) -> std::io::Result<()> {
    update_created_projects(project_root, keys, &[])
}

/// Notes a project ax just created; a failure only costs the later
/// migration of that key, so it is logged and not returned.
fn note_created_project(project_root: &Path, key: &str) {
    if let Err(e) = record_created_projects(project_root, &[key.to_string()]) {
        tracing::warn!(project = key, error = %e, "could not record the SonarQube project ax created");
    }
}

/// Moves projects from an older workspace key (`<folder>-<repo>`) to the
/// current one (`<project_key>-<repo>`). Only projects that ax recorded as
/// created in `.ax/sonar-projects.json` are touched, and nothing is ever
/// deleted: a recorded legacy project whose canonical key already exists is
/// left in place and reported, for the user to remove in SonarQube.
///
/// Older ax versions did not keep that record. With `migrate_legacy_keys`
/// set, every existing legacy key for this workspace's repos is recorded as
/// ax-created first, and then renamed the same way.
async fn migrate_legacy_sonar_projects(
    config: &SonarBootstrapConfig,
    project_root: &Path,
    repo_names: &[String],
    token: Option<&str>,
    admin_ok: bool,
) -> Result<Vec<String>, String> {
    if repo_names.is_empty() || !admin_ok {
        return Ok(Vec::new());
    }

    let workspace_key = workspace_sonar_key(&config.project_key, project_root);
    let legacy_prefixes = legacy_workspace_prefixes(&workspace_key, project_root);
    if legacy_prefixes.is_empty() {
        return Ok(Vec::new());
    }
    let mut ours = created_projects(project_root);
    if ours.is_empty() && !config.migrate_legacy_keys {
        return Ok(Vec::new());
    }

    let multi_repo = repo_names.len() > 1;
    let mut keys_to_check: Vec<String> = Vec::new();
    for repo in repo_names {
        let canonical = canonical_repo_project_key(&workspace_key, repo, multi_repo);
        keys_to_check.push(canonical);
        for legacy_prefix in &legacy_prefixes {
            keys_to_check.push(format!("{legacy_prefix}-{}", sonar_key_from_name(repo)));
        }
    }
    keys_to_check.sort();
    keys_to_check.dedup();

    let (_, exists) = lookup_projects(
        &config.host,
        &keys_to_check,
        token,
        &config.admin_user,
        &config.admin_password,
    )
    .await;

    let mut actions = Vec::new();
    if config.migrate_legacy_keys {
        let mut adopted: Vec<String> = Vec::new();
        for repo in repo_names {
            for legacy_prefix in &legacy_prefixes {
                let legacy = format!("{legacy_prefix}-{}", sonar_key_from_name(repo));
                if exists.get(&legacy).copied().unwrap_or(false) && !ours.contains(&legacy) {
                    adopted.push(legacy);
                }
            }
        }
        adopted.sort();
        adopted.dedup();
        if !adopted.is_empty() {
            record_created_projects(project_root, &adopted).map_err(|e| {
                format!("could not record the legacy SonarQube projects to migrate: {e}")
            })?;
            for key in &adopted {
                actions.push(format!(
                    "Adopted legacy SonarQube project '{key}' for migration (migrate_legacy_keys)"
                ));
            }
            ours.extend(adopted);
        }
    }
    for repo in repo_names {
        let canonical = canonical_repo_project_key(&workspace_key, repo, multi_repo);
        for legacy_prefix in &legacy_prefixes {
            let legacy = format!("{legacy_prefix}-{}", sonar_key_from_name(repo));
            if !ours.contains(&legacy) || !exists.get(&legacy).copied().unwrap_or(false) {
                continue;
            }
            if exists.get(&canonical).copied().unwrap_or(false) {
                actions.push(format!(
                    "SonarQube project '{legacy}' duplicates '{canonical}' and was left in place; delete it in SonarQube if it is no longer needed"
                ));
            } else {
                update_project_key(
                    &config.host,
                    &legacy,
                    &canonical,
                    token,
                    &config.admin_user,
                    &config.admin_password,
                )
                .await?;
                if let Err(e) = update_created_projects(
                    project_root,
                    std::slice::from_ref(&canonical),
                    std::slice::from_ref(&legacy),
                ) {
                    tracing::warn!(error = %e, "could not update the record of SonarQube projects ax created");
                }
                actions.push(format!(
                    "Renamed SonarQube project '{legacy}' → '{canonical}'"
                ));
            }
        }
    }

    Ok(actions)
}

async fn update_project_key(
    host: &str,
    from: &str,
    to: &str,
    token: Option<&str>,
    user: &str,
    password: &str,
) -> Result<(), String> {
    let url = format!(
        "{}/api/projects/update_key?from={}&to={}",
        host.trim_end_matches('/'),
        encode_query(from),
        encode_query(to)
    );
    let resp = authed_post_with_token(&url, token, user, password).await?;
    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(format!(
        "Failed to rename SonarQube project '{from}' → '{to}': HTTP {status} {body}"
    ))
}

async fn create_project(
    host: &str,
    project_key: &str,
    project_name: &str,
    user: &str,
    password: &str,
    token: Option<&str>,
) -> Result<(), String> {
    let url = format!(
        "{host}/api/projects/create?project={}&name={}",
        encode_query(project_key),
        encode_query(project_name)
    );
    let resp = authed_post_with_token(&url, token, user, password).await?;
    if resp.status().is_success() {
        provision_sonar_project(host, project_key, token, user, password).await?;
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(format!(
        "Failed to create SonarQube project: HTTP {status} {body}"
    ))
}

/// Apply default quality profiles, new-code period, and quality gate (idempotent).
async fn provision_sonar_project(
    host: &str,
    project_key: &str,
    token: Option<&str>,
    user: &str,
    password: &str,
) -> Result<(), String> {
    for (language, profile) in [
        ("cs", "Sonar way"),
        ("js", "Sonar way"),
        ("ts", "Sonar way"),
        ("php", "Sonar way"),
        ("web", "Sonar way"),
        ("xml", "Sonar way"),
        ("yaml", "Sonar way"),
        ("json", "Sonar way"),
    ] {
        let url = format!(
            "{host}/api/qualityprofiles/add_project?project={}&language={}&qualityProfile={}",
            encode_query(project_key),
            encode_query(language),
            encode_query(profile)
        );
        let _ = authed_post_with_token(&url, token, user, password).await;
    }

    let new_code_url = format!(
        "{host}/api/new_code_periods/set?project={}&type=PREVIOUS_VERSION",
        encode_query(project_key)
    );
    let _ = authed_post_with_token(&new_code_url, token, user, password).await;

    let gate_url = format!(
        "{host}/api/qualitygates/select?projectKey={}&gateName={}",
        encode_query(project_key),
        encode_query("Sonar way")
    );
    let _ = authed_post_with_token(&gate_url, token, user, password).await;

    Ok(())
}

/// Percent-encodes a query value per UTF-8 byte, leaving only RFC 3986
/// unreserved characters as they are.
fn encode_query(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn token_url(host: &str, action: &str, name: &str) -> String {
    format!(
        "{host}/api/user_tokens/{action}?name={}",
        encode_query(name)
    )
}

async fn generate_token(
    host: &str,
    name: &str,
    user: &str,
    password: &str,
) -> Result<String, String> {
    match try_generate_token(host, name, user, password).await {
        Ok(token) => Ok(token),
        Err(e) if token_already_exists(&e) => {
            revoke_token(host, name, user, password).await?;
            try_generate_token(host, name, user, password).await
        }
        Err(e) => Err(e),
    }
}

fn token_already_exists(err: &str) -> bool {
    err.contains("already exists")
}

async fn try_generate_token(
    host: &str,
    name: &str,
    user: &str,
    password: &str,
) -> Result<String, String> {
    let url = token_url(host, "generate", name);
    let resp = authed_post(&url, user, password).await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Failed to generate SonarQube token: HTTP {status} {body}"
        ));
    }
    let body: TokenGenerateResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body.token)
}

async fn revoke_token(host: &str, name: &str, user: &str, password: &str) -> Result<(), String> {
    let url = token_url(host, "revoke", name);
    let resp = authed_post(&url, user, password).await?;
    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(format!(
        "Failed to revoke existing SonarQube token: HTTP {status} {body}"
    ))
}

async fn authed_get(url: &str, user: &str, password: &str) -> Result<reqwest::Response, String> {
    http_client()
        .get(url)
        .header("Authorization", basic_auth(user, password))
        .send()
        .await
        .map_err(|e| e.to_string())
}

/// GET with a SonarQube user/scanner token (9.x basic, 10+ bearer fallback).
async fn token_authed_get(url: &str, token: &str) -> Result<reqwest::Response, String> {
    let trimmed = token.trim();
    let client = http_client();

    // SonarQube 9.x: token as login, empty password.
    let basic = client
        .get(url)
        .header("Authorization", basic_auth(trimmed, ""))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if basic.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(basic);
    }

    // SonarQube 10+: Bearer scheme.
    client
        .get(url)
        .header("Authorization", format!("Bearer {trimmed}"))
        .send()
        .await
        .map_err(|e| e.to_string())
}

async fn authed_post(url: &str, user: &str, password: &str) -> Result<reqwest::Response, String> {
    http_client()
        .post(url)
        .header("Authorization", basic_auth(user, password))
        .send()
        .await
        .map_err(|e| e.to_string())
}

async fn authed_post_with_token(
    url: &str,
    token: Option<&str>,
    user: &str,
    password: &str,
) -> Result<reqwest::Response, String> {
    if let Some(t) = token.filter(|s| !s.trim().is_empty()) {
        let resp = token_authed_post(url, t).await?;
        if resp.status() != reqwest::StatusCode::UNAUTHORIZED {
            return Ok(resp);
        }
    }
    authed_post(url, user, password).await
}

async fn token_authed_post(url: &str, token: &str) -> Result<reqwest::Response, String> {
    let trimmed = token.trim();
    let client = http_client();

    let basic = client
        .post(url)
        .header("Authorization", basic_auth(trimmed, ""))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if basic.status() != reqwest::StatusCode::UNAUTHORIZED {
        return Ok(basic);
    }

    client
        .post(url)
        .header("Authorization", format!("Bearer {trimmed}"))
        .send()
        .await
        .map_err(|e| e.to_string())
}

fn basic_auth(user: &str, password: &str) -> String {
    format!("Basic {}", STANDARD.encode(format!("{user}:{password}")))
}

/// Saves the scanner token to `.ax/sonar.token` (owner read/write only on
/// Unix, replaced atomically) and, on Windows, to the user environment.
/// The running process environment is never changed: callers read the
/// token back with [`read_sonar_token`], which prefers the file. Returns the
/// token path and whether the user environment variable was set.
fn persist_token(
    project_root: &Path,
    token_env: &str,
    token: &str,
) -> Result<(PathBuf, bool), String> {
    let token_path = sonar_token_path(project_root);
    ax_utils::atomic_write_private(&token_path, format!("{token}\n").as_bytes())
        .map_err(|e| format!("could not save {}: {e}", token_path.display()))?;
    Ok((token_path, set_user_env_var(token_env, token)))
}

const USER_ENV_NAME_VAR: &str = "AX_USER_ENV_NAME";
const USER_ENV_VALUE_VAR: &str = "AX_USER_ENV_VALUE";

/// Whether `name` is a plain environment variable name
/// (`[A-Za-z_][A-Za-z0-9_]*`).
fn is_env_var_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The PowerShell command that stores `name=value` in the Windows user
/// environment. The name and value reach the script through the child's
/// environment, never through the command line, so neither can change the
/// script and the token does not show up in process listings. Returns
/// `None` when `name` is not a plain variable name.
fn user_env_command(name: &str, value: &str) -> Option<std::process::Command> {
    if !is_env_var_name(name) {
        return None;
    }
    let script = format!(
        "[Environment]::SetEnvironmentVariable($env:{USER_ENV_NAME_VAR}, $env:{USER_ENV_VALUE_VAR}, 'User')"
    );
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .env(USER_ENV_NAME_VAR, name)
        .env(USER_ENV_VALUE_VAR, value);
    Some(cmd)
}

fn set_user_env_var(name: &str, value: &str) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let Some(mut cmd) = user_env_command(name, value) else {
        tracing::warn!(
            token_env = name,
            "sonar.token_env is not a plain variable name; not storing the token in the user environment"
        );
        return false;
    };
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

/// Host URL reachable from a scanner container (Podman/Docker on Windows/macOS).
pub fn scanner_host_url(host: &str) -> String {
    let trimmed = host.trim_end_matches('/');
    if trimmed.contains("localhost") {
        return trimmed.replace("localhost", "host.containers.internal");
    }
    if trimmed.contains("127.0.0.1") {
        return trimmed.replace("127.0.0.1", "host.containers.internal");
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// A minimal HTTP server standing in for SonarQube. `respond` maps
    /// `(method, path_and_query)` to `(status, body)`; every request line is
    /// recorded.
    pub(crate) mod mock_sonar {
        use std::sync::{Arc, Mutex};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        pub struct MockSonar {
            pub host: String,
            pub requests: Arc<Mutex<Vec<String>>>,
        }

        impl MockSonar {
            pub fn requests(&self) -> Vec<String> {
                self.requests.lock().unwrap().clone()
            }
        }

        pub async fn start<F>(respond: F) -> MockSonar
        where
            F: Fn(&str, &str) -> (u16, String) + Send + Sync + 'static,
        {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let host = format!("http://{}", listener.local_addr().unwrap());
            let requests = Arc::new(Mutex::new(Vec::new()));
            let log = requests.clone();
            let respond = Arc::new(respond);
            tokio::spawn(async move {
                loop {
                    let Ok((mut sock, _)) = listener.accept().await else {
                        return;
                    };
                    let log = log.clone();
                    let respond = respond.clone();
                    tokio::spawn(async move {
                        let mut buf = Vec::new();
                        let mut chunk = [0u8; 4096];
                        while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                            match sock.read(&mut chunk).await {
                                Ok(0) | Err(_) => return,
                                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                            }
                        }
                        let head = String::from_utf8_lossy(&buf).into_owned();
                        let mut parts = head.lines().next().unwrap_or("").split(' ');
                        let method = parts.next().unwrap_or("").to_string();
                        let path = parts.next().unwrap_or("").to_string();
                        log.lock().unwrap().push(format!("{method} {path}"));
                        let (status, body) = respond(&method, &path);
                        let reply = format!(
                            "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                        let _ = sock.write_all(reply.as_bytes()).await;
                        let _ = sock.shutdown().await;
                    });
                }
            });
            MockSonar { host, requests }
        }
    }

    /// Workspace folder `Contoso` with ship.toml key `ax` and repos Web and
    /// Api: the legacy keys are `Contoso-Web` / `Contoso-Api`, the canonical
    /// ones `ax-Web` / `ax-Api`.
    fn contoso_workspace() -> (tempfile::TempDir, PathBuf, Vec<String>) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Contoso");
        std::fs::create_dir_all(&root).unwrap();
        (dir, root, vec!["Web".to_string(), "Api".to_string()])
    }

    async fn sonar_with_projects(existing: &'static [&'static str]) -> mock_sonar::MockSonar {
        mock_sonar::start(move |_method, path| {
            if path.starts_with("/api/system/status") {
                return (200, r#"{"status":"UP"}"#.into());
            }
            if path.starts_with("/api/projects/search") {
                let comps: Vec<String> = existing
                    .iter()
                    .map(|k| format!(r#"{{"key":"{k}"}}"#))
                    .collect();
                return (200, format!(r#"{{"components":[{}]}}"#, comps.join(",")));
            }
            (200, "{}".into())
        })
        .await
    }

    fn mutating(requests: &[String]) -> Vec<String> {
        requests
            .iter()
            .filter(|r| {
                r.contains("/api/projects/delete") || r.contains("/api/projects/update_key")
            })
            .cloned()
            .collect()
    }

    async fn sonar_with_status(status: &'static str) -> mock_sonar::MockSonar {
        mock_sonar::start(move |_m, path| {
            if path.starts_with("/api/system/status") {
                (200, format!(r#"{{"status":"{status}"}}"#))
            } else {
                (200, "{}".into())
            }
        })
        .await
    }

    #[tokio::test]
    async fn sonar_is_ready_only_when_status_is_up() {
        for status in ["STARTING", "DB_MIGRATION_NEEDED", "DOWN"] {
            let sonar = sonar_with_status(status).await;
            assert!(!sonar_reachable(&sonar.host).await, "{status}");
            assert!(
                crate::container::wait_for_sonar(&sonar.host, 1)
                    .await
                    .is_err(),
                "{status}"
            );
        }
        let sonar = sonar_with_status("UP").await;
        assert!(sonar_reachable(&sonar.host).await);
        crate::container::wait_for_sonar(&sonar.host, 5)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn wait_for_sonar_gives_up_at_the_deadline_when_a_request_hangs() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((sock, _)) = listener.accept().await {
                held.push(sock); // never answer
            }
        });
        let started = std::time::Instant::now();
        let outcome = tokio::time::timeout(
            Duration::from_secs(30),
            crate::container::wait_for_sonar(&host, 2),
        )
        .await;
        assert!(
            matches!(outcome, Ok(Err(_))),
            "wait_for_sonar must fail on its own deadline"
        );
        assert!(
            started.elapsed() < Duration::from_secs(15),
            "{:?}",
            started.elapsed()
        );
    }

    /// A SonarQube that is UP, accepts the admin login and any token, and has
    /// the canonical `ax-Web` / `ax-Api` projects.
    async fn ready_sonar() -> mock_sonar::MockSonar {
        mock_sonar::start(|_method, path| {
            if path.starts_with("/api/system/status") {
                return (200, r#"{"status":"UP"}"#.into());
            }
            if path.starts_with("/api/authentication/validate") {
                return (200, r#"{"valid":true}"#.into());
            }
            if path.starts_with("/api/projects/search") {
                return (
                    200,
                    r#"{"components":[{"key":"ax-Web"},{"key":"ax-Api"}]}"#.into(),
                );
            }
            (200, "{}".into())
        })
        .await
    }

    fn status_pings(requests: &[String]) -> usize {
        requests
            .iter()
            .filter(|r| r.contains("/api/system/status"))
            .count()
    }

    #[tokio::test]
    async fn ensure_sonar_projects_checks_reachability_once() {
        let (_dir, root, repos) = contoso_workspace();
        let sonar = ready_sonar().await;
        let config = SonarBootstrapConfig::from_sonar(&sonar.host, "ax", "SONAR_TOKEN");
        ensure_sonar_projects(&config, &root, &repos).await.unwrap();
        assert_eq!(status_pings(&sonar.requests()), 1, "{:?}", sonar.requests());
    }

    #[tokio::test]
    async fn ensure_sonar_ready_for_scan_checks_reachability_once() {
        let (_dir, root, repos) = contoso_workspace();
        std::fs::create_dir_all(root.join(".ax")).unwrap();
        std::fs::write(root.join(".ax/sonar.token"), "squ_valid\n").unwrap();
        let sonar = ready_sonar().await;
        let cfg = crate::sonar::SonarConfig {
            enabled: true,
            host: sonar.host.clone(),
            project_key: "ax".into(),
            token_env: "AX_QUALITY_TEST_UNSET_TOKEN_ENV".into(),
            ..Default::default()
        };
        ensure_sonar_ready_for_scan(&cfg, &root, &repos)
            .await
            .unwrap();
        assert_eq!(status_pings(&sonar.requests()), 1, "{:?}", sonar.requests());
    }

    #[test]
    fn the_http_client_is_shared() {
        assert!(std::ptr::eq(http_client(), http_client()));
    }

    #[tokio::test]
    async fn migration_leaves_projects_ax_did_not_create_alone() {
        let (_dir, root, repos) = contoso_workspace();
        let sonar = sonar_with_projects(&["Contoso-Web", "ax-Web", "Contoso-Api"]).await;
        let config = SonarBootstrapConfig::from_sonar(&sonar.host, "ax", "SONAR_TOKEN");
        migrate_legacy_sonar_projects(&config, &root, &repos, None, true)
            .await
            .unwrap();
        assert_eq!(mutating(&sonar.requests()), Vec::<String>::new());
    }

    #[tokio::test]
    async fn migration_renames_only_a_project_ax_recorded_and_never_deletes() {
        let (_dir, root, repos) = contoso_workspace();
        record_created_projects(&root, &["Contoso-Web".into(), "Contoso-Api".into()]).unwrap();
        // Contoso-Web has a canonical twin, Contoso-Api does not.
        let sonar = sonar_with_projects(&["Contoso-Web", "ax-Web", "Contoso-Api"]).await;
        let config = SonarBootstrapConfig::from_sonar(&sonar.host, "ax", "SONAR_TOKEN");
        let log = migrate_legacy_sonar_projects(&config, &root, &repos, None, true)
            .await
            .unwrap();
        assert_eq!(
            mutating(&sonar.requests()),
            vec!["POST /api/projects/update_key?from=Contoso-Api&to=ax-Api".to_string()]
        );
        assert!(
            log.iter()
                .any(|l| l.contains("Contoso-Web") && l.contains("left in place")),
            "{log:?}"
        );
        let created = created_projects(&root);
        assert!(created.contains("ax-Api") && !created.contains("Contoso-Api"));
        assert!(created.contains("Contoso-Web"));
    }

    #[tokio::test]
    async fn migrate_legacy_keys_adopts_and_renames_legacy_projects_without_a_record() {
        let (_dir, root, repos) = contoso_workspace();
        assert!(!created_projects_path(&root).exists());
        let sonar = sonar_with_projects(&["Contoso-Web", "ax-Web", "Contoso-Api"]).await;
        let mut config = SonarBootstrapConfig::from_sonar(&sonar.host, "ax", "SONAR_TOKEN");
        config.migrate_legacy_keys = true;
        let log = migrate_legacy_sonar_projects(&config, &root, &repos, None, true)
            .await
            .unwrap();
        assert_eq!(
            mutating(&sonar.requests()),
            vec!["POST /api/projects/update_key?from=Contoso-Api&to=ax-Api".to_string()]
        );
        assert!(
            log.iter()
                .any(|l| l.contains("Contoso-Web") && l.contains("left in place")),
            "{log:?}"
        );
        let created = created_projects(&root);
        assert!(created.contains("ax-Api") && !created.contains("Contoso-Api"));
        assert!(created.contains("Contoso-Web"), "{created:?}");
    }

    #[test]
    fn migrate_legacy_keys_is_off_unless_the_config_sets_it() {
        let base = serde_json::json!({
            "enabled": true,
            "host": "http://localhost:9000",
            "project_key": "ax",
            "token_env": "SONAR_TOKEN",
            "scanner_path": "sonar-scanner",
            "podman_container": null
        });
        let cfg: crate::sonar::SonarConfig = serde_json::from_value(base.clone()).unwrap();
        assert!(!cfg.migrate_legacy_keys);
        let root = Path::new("/tmp/x");
        assert!(!SonarBootstrapConfig::resolve_for_project(&cfg, root).migrate_legacy_keys);
        let mut on = base;
        on["migrate_legacy_keys"] = true.into();
        let cfg: crate::sonar::SonarConfig = serde_json::from_value(on).unwrap();
        assert!(SonarBootstrapConfig::resolve_for_project(&cfg, root).migrate_legacy_keys);
    }

    #[test]
    fn persist_token_writes_the_file_and_leaves_the_process_environment_alone() {
        let dir = tempfile::tempdir().unwrap();
        let name = "AX_QUALITY_TEST_PERSIST_TOKEN_ENV";
        let (path, _) = persist_token(dir.path(), name, "squ_secret").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "squ_secret\n");
        assert_eq!(
            read_sonar_token(dir.path(), name).as_deref(),
            Some("squ_secret")
        );
        assert!(
            std::env::var_os(name).is_none(),
            "the token must not be exported into this process"
        );
    }

    #[cfg(unix)]
    #[test]
    fn persist_token_file_is_readable_by_the_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = sonar_token_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        // An older token file with wide permissions is tightened too.
        std::fs::write(&path, "old\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        persist_token(dir.path(), "AX_QUALITY_TEST_PERSIST_MODE", "squ_new").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "squ_new\n");
    }

    #[test]
    fn user_env_command_refuses_names_that_are_not_plain_identifiers() {
        for bad in [
            "X','1','User'); Start-Process calc; #",
            "",
            "1ABC",
            "A B",
            "A-B",
            "A$B",
        ] {
            assert!(user_env_command(bad, "v").is_none(), "{bad:?}");
        }
        assert!(user_env_command("SONAR_TOKEN", "v").is_some());
        assert!(user_env_command("_sonar1", "v").is_some());
    }

    #[test]
    fn user_env_command_keeps_name_and_value_off_the_command_line() {
        let cmd = user_env_command("SONAR_TOKEN", "squ_it's'secret").unwrap();
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(
            args.iter()
                .all(|a| !a.contains("squ_") && !a.contains("SONAR_TOKEN")),
            "{args:?}"
        );
        let envs: HashMap<String, String> = cmd
            .get_envs()
            .filter_map(|(k, v)| {
                Some((
                    k.to_string_lossy().into_owned(),
                    v?.to_string_lossy().into_owned(),
                ))
            })
            .collect();
        assert_eq!(
            envs.get(USER_ENV_NAME_VAR).map(String::as_str),
            Some("SONAR_TOKEN")
        );
        assert_eq!(
            envs.get(USER_ENV_VALUE_VAR).map(String::as_str),
            Some("squ_it's'secret")
        );
    }

    #[cfg(windows)]
    #[test]
    fn resolves_placeholder_to_folder_name() {
        let root = Path::new(r"C:\gary\Contoso");
        let (key, name) = resolve_sonar_project("your-project", root);
        assert_eq!(key, "Contoso");
        assert_eq!(name, "Contoso");
    }

    #[cfg(unix)]
    #[test]
    fn resolves_placeholder_to_folder_name_unix() {
        let root = Path::new("/home/gary/Contoso");
        let (key, name) = resolve_sonar_project("your-project", root);
        assert_eq!(key, "Contoso");
        assert_eq!(name, "Contoso");
    }

    #[test]
    fn keeps_explicit_project_key() {
        let root = Path::new("/projects/Contoso");
        let (key, name) = resolve_sonar_project("my-sonar-app", root);
        assert_eq!(key, "my-sonar-app");
        assert_eq!(name, "my-sonar-app");
    }

    #[test]
    fn ax_is_explicit_project_key_not_placeholder() {
        let root = Path::new(r"C:\gary\ax");
        let (key, name) = resolve_sonar_project("ax", root);
        assert_eq!(key, "ax");
        assert_eq!(name, "ax");
    }

    #[cfg(windows)]
    #[test]
    fn legacy_prefix_from_workspace_folder() {
        let root = Path::new(r"C:\gary\Contoso");
        assert_eq!(
            legacy_workspace_prefixes("ax", root),
            vec!["Contoso".to_string()]
        );
        assert!(legacy_workspace_prefixes("Contoso", root).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn legacy_prefix_from_workspace_folder_unix() {
        let root = Path::new("/home/gary/Contoso");
        assert_eq!(
            legacy_workspace_prefixes("ax", root),
            vec!["Contoso".to_string()]
        );
        assert!(legacy_workspace_prefixes("Contoso", root).is_empty());
    }

    #[test]
    fn canonical_repo_key_multi_repo() {
        assert_eq!(
            canonical_repo_project_key("ax", "Contoso-Web", true),
            "ax-Contoso-Web"
        );
        assert_eq!(canonical_repo_project_key("ax", "Contoso-Web", false), "ax");
    }

    #[test]
    fn token_already_exists_detects_conflict() {
        assert!(token_already_exists(
            "Failed to generate SonarQube token: HTTP 400 {\"errors\":[{\"msg\":\"already exists\"}]}"
        ));
    }

    #[test]
    fn encode_query_percent_encodes_utf8_bytes() {
        assert_eq!(encode_query("Caf\u{e9}-Portal"), "Caf%C3%A9-Portal");
        assert_eq!(encode_query("\u{20ac} a&b"), "%E2%82%AC%20a%26b");
    }

    #[test]
    fn token_urls_encode_the_token_name() {
        assert_eq!(
            token_url("http://h", "generate", "ax scan&x"),
            "http://h/api/user_tokens/generate?name=ax%20scan%26x"
        );
        assert_eq!(
            token_url("http://h", "revoke", "\u{e9}"),
            "http://h/api/user_tokens/revoke?name=%C3%A9"
        );
    }
}
