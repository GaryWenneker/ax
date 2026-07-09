//! SonarQube first-time setup: project creation, token generation, local persistence.

use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::Deserialize;

const DEFAULT_ADMIN_USER: &str = "admin";
const DEFAULT_ADMIN_PASSWORD: &str = "admin";
const DEFAULT_TOKEN_NAME: &str = "ax-ship";

#[derive(Debug, Clone, serde::Serialize)]
pub struct SonarSetupStatus {
    pub login_user: String,
    pub login_password_hint: String,
    pub project_exists: bool,
    pub token_configured: bool,
    pub scanner_available: bool,
    pub token_path: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SonarBootstrapResult {
    pub project_created: bool,
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
        Self {
            host: host.trim_end_matches('/').to_string(),
            project_key: project_key.to_string(),
            project_name: project_key.to_string(),
            admin_user: DEFAULT_ADMIN_USER.into(),
            admin_password: DEFAULT_ADMIN_PASSWORD.into(),
            token_name: DEFAULT_TOKEN_NAME.into(),
            token_env: token_env.to_string(),
        }
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
    std::env::var(token_env)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            std::fs::read_to_string(sonar_token_path(project_root))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        })
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
) -> Result<SonarSetupStatus, String> {
    let project_exists = project_exists(&config.host, &config.project_key, &config.admin_user, &config.admin_password)
        .await?;
    Ok(SonarSetupStatus {
        login_user: config.admin_user.clone(),
        login_password_hint: "Default: admin / admin — change after first login in the SonarQube UI.".into(),
        project_exists,
        token_configured: token_configured(project_root, &config.token_env),
        scanner_available: scanner_available(scanner_path),
        token_path: sonar_token_path(project_root).display().to_string(),
    })
}

pub async fn bootstrap_sonar(
    config: &SonarBootstrapConfig,
    project_root: &Path,
) -> Result<SonarBootstrapResult, String> {
    if !sonar_reachable(&config.host).await {
        return Err(format!(
            "SonarQube is not reachable at {}. Install & start the container first.",
            config.host
        ));
    }

    if !validate_login(&config.host, &config.admin_user, &config.admin_password).await? {
        return Err(
            "SonarQube login failed (admin/admin). Open the UI and reset the admin password, then retry."
                .into(),
        );
    }

    let mut project_created = false;
    if !project_exists(&config.host, &config.project_key, &config.admin_user, &config.admin_password).await? {
        create_project(
            &config.host,
            &config.project_key,
            &config.project_name,
            &config.admin_user,
            &config.admin_password,
        )
        .await?;
        project_created = true;
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
        project_created,
        token_saved: true,
        token_env_set,
        ui_url: config.host.clone(),
        login_user: config.admin_user.clone(),
        login_password_hint: "admin / admin (change in UI → My Account)".into(),
        token_path: token_path.display().to_string(),
        message: if project_created {
            "Created SonarQube project and saved scanner token.".into()
        } else {
            "SonarQube project already existed; generated and saved a new scanner token.".into()
        },
    })
}

async fn sonar_reachable(host: &str) -> bool {
    let url = format!("{host}/api/system/status");
    reqwest::Client::new()
        .get(&url)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

async fn validate_login(host: &str, user: &str, password: &str) -> Result<bool, String> {
    let url = format!("{host}/api/authentication/validate");
    let resp = authed_get(&url, user, password).await?;
    Ok(resp.status().is_success())
}

async fn project_exists(host: &str, project_key: &str, user: &str, password: &str) -> Result<bool, String> {
    let url = format!("{host}/api/projects/search?projects={project_key}");
    let resp = authed_get(&url, user, password).await?;
    if !resp.status().is_success() {
        return Err(format!("SonarQube project search failed: HTTP {}", resp.status()));
    }
    let body: ProjectSearchResponse = resp.json().await.map_err(|e| e.to_string())?;
    Ok(body.components.iter().any(|c| c.key == project_key))
}

async fn create_project(
    host: &str,
    project_key: &str,
    project_name: &str,
    user: &str,
    password: &str,
) -> Result<(), String> {
    let url = format!("{host}/api/projects/create?project={project_key}&name={project_name}");
    let resp = authed_post(&url, user, password).await?;
    if resp.status().is_success() {
        return Ok(());
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(format!("Failed to create SonarQube project: HTTP {status} {body}"))
}

async fn generate_token(host: &str, name: &str, user: &str, password: &str) -> Result<String, String> {
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

async fn authed_get(url: &str, user: &str, password: &str) -> Result<reqwest::Response, String> {
    reqwest::Client::new()
        .get(url)
        .header("Authorization", basic_auth(user, password))
        .send()
        .await
        .map_err(|e| e.to_string())
}

async fn authed_post(url: &str, user: &str, password: &str) -> Result<reqwest::Response, String> {
    reqwest::Client::new()
        .post(url)
        .header("Authorization", basic_auth(user, password))
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
