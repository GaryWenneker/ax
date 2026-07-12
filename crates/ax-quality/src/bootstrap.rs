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
pub fn resolve_sonar_repo_projects(workspace_key: &str, repo_names: &[String]) -> Vec<RepoProjectStatus> {
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
pub fn canonical_repo_project_key(workspace_key: &str, repo_name: &str, multi_repo: bool) -> String {
    if multi_repo {
        format!("{workspace_key}-{}", sonar_key_from_name(repo_name))
    } else {
        workspace_key.to_string()
    }
}

/// Legacy workspace prefixes that may have been used before `project_key` changed (e.g. VfPf → ax).
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
            login_password_hint: "Default local admin credentials in .ax/ship.toml — applied automatically by ax.".into(),
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
    let (lookup_summary, exists_by_key) = lookup_projects(
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
        login_password_hint: "Default local admin credentials in .ax/ship.toml — applied automatically by ax.".into(),
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

    if !validate_sonar_login(&config.host, &config.admin_user, &config.admin_password).await? {
        return Err(format!(
            "SonarQube login failed for user '{}'. Ensure the local container is running and reset the stack if credentials were changed outside ax.",
            config.admin_user
        ));
    }

    ensure_sonar_dark_theme(&config.host, &config.admin_user, &config.admin_password).await;

    let (projects_created, repo_projects, _) =
        ensure_sonar_projects(config, project_root, repo_names).await?;

    let token = generate_token(
        &config.host,
        &config.token_name,
        &config.admin_user,
        &config.admin_password,
    )
    .await?;

    let token_path = sonar_token_path(project_root);
    if let Some(parent) = token_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&token_path, format!("{token}\n")).map_err(|e| e.to_string())?;

    std::env::set_var(&config.token_env, &token);
    let token_env_set = set_user_env_var(&config.token_env, &token);

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
        login_password_hint: "Default local admin credentials in .ax/ship.toml — applied automatically by ax.".into(),
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

    let token = read_sonar_token(project_root, &config.token_env);
    let token_ok = match token.as_deref() {
        Some(t) => validate_sonar_token(&config.host, t).await,
        None => false,
    };
    let admin_ok = validate_sonar_login(&config.host, &config.admin_user, &config.admin_password).await?;

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
        if lookup_project(
            &config.host,
            &workspace_key,
            token.as_deref(),
            &config.admin_user,
            &config.admin_password,
        )
        .await
        != ProjectLookup::Found
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
        }
        return Ok((projects_created, vec![], newly_created_repos));
    }

    let keys: Vec<String> = repo_projects.iter().map(|p| p.key.clone()).collect();
    let (lookup_summary, exists_by_key) = lookup_projects(
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
    if let Some(token) = read_sonar_token(project_root, &config.token_env) {
        if validate_sonar_token(&config.host, &token).await {
            return Ok(());
        }
    }

    if !validate_sonar_login(&config.host, &config.admin_user, &config.admin_password).await? {
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

    let token_path = sonar_token_path(project_root);
    if let Some(parent) = token_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&token_path, format!("{token}\n")).map_err(|e| e.to_string())?;
    std::env::set_var(&config.token_env, &token);
    let _ = set_user_env_var(&config.token_env, &token);
    Ok(())
}

/// True when the SonarQube project exists but has never received an analysis.
pub async fn project_needs_baseline_scan(host: &str, project_key: &str, token: &str) -> bool {
    let url = format!(
        "{}/api/measures/component?component={project_key}&metricKeys=ncloc",
        host.trim_end_matches('/')
    );
    let Ok(resp) = token_authed_get(&url, token).await else {
        return true;
    };
    if !resp.status().is_success() {
        return true;
    }
    let Ok(body) = resp.json::<serde_json::Value>().await else {
        return true;
    };
    body.get("component")
        .and_then(|c| c.get("measures"))
        .and_then(|m| m.as_array())
        .map(|m| m.is_empty())
        .unwrap_or(true)
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
    if !sonar_reachable(&sonar.host).await {
        return Ok(());
    }
    tracing::info!(
        repos = repo_names.len(),
        "Auto-provisioning SonarQube projects from discovered git repos"
    );
    ensure_sonar_ready_for_scan(sonar, project_root, repo_names).await
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
    ensure_sonar_dark_theme(&config.host, &config.admin_user, &config.admin_password).await;
    ensure_sonar_projects(&config, project_root, repo_names).await?;
    ensure_sonar_token(&config, project_root).await?;
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

    if !validate_sonar_login(&config.host, &config.admin_user, &config.admin_password).await? {
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

    let token_path = sonar_token_path(project_root);
    if let Some(parent) = token_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&token_path, format!("{token}\n")).map_err(|e| e.to_string())?;

    std::env::set_var(&config.token_env, &token);
    let token_env_set = set_user_env_var(&config.token_env, &token);

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
        login_password_hint: "Default local admin credentials in .ax/ship.toml — applied automatically by ax.".into(),
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
    tracing::debug!("SonarQube dark theme preferences applied (proxy also injects dark theme in HTML)");
}

/// Whether the SonarQube HTTP API responds at `host`.
pub async fn sonar_reachable(host: &str) -> bool {
    let url = format!("{}/api/system/status", host.trim_end_matches('/'));
    http_client()
        .get(&url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Check SonarQube web UI credentials against `/api/authentication/validate`.
pub async fn validate_sonar_login(host: &str, user: &str, password: &str) -> Result<bool, String> {
    let host = host.trim_end_matches('/');
    if !sonar_reachable(host).await {
        return Err("SonarQube is not reachable".into());
    }
    let url = format!("{host}/api/authentication/validate");
    let resp = authed_get(&url, user, password).await?;
    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Ok(false);
    }
    if !resp.status().is_success() {
        return Err(format!("SonarQube login check failed: HTTP {}", resp.status()));
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
    let mut exists = HashMap::new();
    if project_keys.is_empty() {
        return (ProjectLookup::Missing, exists);
    }
    if !sonar_reachable(host).await {
        return (ProjectLookup::Unreachable, exists);
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

fn summarize_project_lookup(project_keys: &[String], exists: &HashMap<String, bool>) -> ProjectLookup {
    if project_keys.iter().all(|k| exists.get(k).copied().unwrap_or(false)) {
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
    for repo in repo_names {
        let canonical = canonical_repo_project_key(&workspace_key, repo, multi_repo);
        for legacy_prefix in &legacy_prefixes {
            let legacy = format!("{legacy_prefix}-{}", sonar_key_from_name(repo));
            if !exists.get(&legacy).copied().unwrap_or(false) {
                continue;
            }
            if exists.get(&canonical).copied().unwrap_or(false) {
                delete_project(
                    &config.host,
                    &legacy,
                    token,
                    &config.admin_user,
                    &config.admin_password,
                )
                .await?;
                actions.push(format!("Removed duplicate SonarQube project '{legacy}'"));
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
                actions.push(format!("Renamed SonarQube project '{legacy}' → '{canonical}'"));
            }
        }
    }

    Ok(actions)
}

async fn delete_project(
    host: &str,
    project_key: &str,
    token: Option<&str>,
    user: &str,
    password: &str,
) -> Result<(), String> {
    let url = format!(
        "{}/api/projects/delete?project={}",
        host.trim_end_matches('/'),
        encode_query(project_key)
    );
    let resp = authed_post_with_token(&url, token, user, password).await?;
    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(format!("Failed to delete SonarQube project '{project_key}': HTTP {status} {body}"))
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
    Err(format!("Failed to create SonarQube project: HTTP {status} {body}"))
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

fn encode_query(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            ' ' => "%20".into(),
            _ => format!("%{:02X}", c as u32),
        })
        .collect()
}

async fn generate_token(host: &str, name: &str, user: &str, password: &str) -> Result<String, String> {
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

async fn try_generate_token(host: &str, name: &str, user: &str, password: &str) -> Result<String, String> {
    let url = format!("{host}/api/user_tokens/generate?name={name}");
    let resp = authed_post(&url, user, password).await?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Failed to generate SonarQube token: HTTP {status} {body}"));
    }
    let body: TokenGenerateResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body.token)
}

async fn revoke_token(host: &str, name: &str, user: &str, password: &str) -> Result<(), String> {
    let url = format!("{host}/api/user_tokens/revoke?name={name}");
    let resp = authed_post(&url, user, password).await?;
    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(format!("Failed to revoke existing SonarQube token: HTTP {status} {body}"))
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

fn set_user_env_var(name: &str, value: &str) -> bool {
    #[cfg(windows)]
    {
        let script = format!(
            "[Environment]::SetEnvironmentVariable('{name}', '{value}', 'User')"
        );
        return std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
    }
    #[cfg(not(windows))]
    {
        let _ = (name, value);
        false
    }
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

    #[test]
    fn resolves_placeholder_to_folder_name() {
        let root = Path::new(r"C:\gary\VfPf");
        let (key, name) = resolve_sonar_project("your-project", root);
        assert_eq!(key, "VfPf");
        assert_eq!(name, "VfPf");
    }

    #[test]
    fn keeps_explicit_project_key() {
        let root = Path::new("/projects/VfPf");
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

    #[test]
    fn legacy_prefix_from_workspace_folder() {
        let root = Path::new(r"C:\gary\VfPf");
        assert_eq!(legacy_workspace_prefixes("ax", root), vec!["VfPf".to_string()]);
        assert!(legacy_workspace_prefixes("VfPf", root).is_empty());
    }

    #[test]
    fn canonical_repo_key_multi_repo() {
        assert_eq!(
            canonical_repo_project_key("ax", "Mijn-Pf", true),
            "ax-Mijn-Pf"
        );
        assert_eq!(canonical_repo_project_key("ax", "Mijn-Pf", false), "ax");
    }

    #[test]
    fn token_already_exists_detects_conflict() {
        assert!(token_already_exists(
            "Failed to generate SonarQube token: HTTP 400 {\"errors\":[{\"msg\":\"already exists\"}]}"
        ));
    }
}
