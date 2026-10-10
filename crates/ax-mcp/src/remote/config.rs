use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use url::Url;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteConfig {
    pub public_url: String,
    pub oauth: OAuthConfig,
    pub projects: Vec<Project>,
    #[serde(default)]
    pub api_keys: Vec<ApiKey>,
    #[serde(default)]
    pub allowed_origins: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OAuthConfig {
    pub issuer: String,
    pub jwks_uri: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: String,
    pub root: PathBuf,
    pub subjects: Vec<String>,
    #[serde(default)]
    pub write_subjects: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiKey {
    pub id: String,
    /// SHA-256 hex, never the original key. API keys are for IDE clients only.
    pub sha256: String,
    pub projects: Vec<String>,
    #[serde(default)]
    pub write: bool,
}

pub fn https_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "Invalid HTTPS URL".to_string())?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Expected an HTTPS URL without credentials, query or fragment".into());
    }
    Ok(url)
}

impl RemoteConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut config: Self = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&mut self) -> Result<(), String> {
        let public = https_url(&self.public_url)?;
        if public.path() != "/" {
            return Err("public_url must be an HTTPS origin".into());
        }
        self.public_url = self.public_url.trim_end_matches('/').to_owned();
        https_url(&self.oauth.issuer)?;
        https_url(&self.oauth.jwks_uri)?;
        if self.projects.is_empty() {
            return Err("Register at least one project".into());
        }
        let mut ids = HashSet::new();
        for p in &mut self.projects {
            if p.id.is_empty()
                || p.id.len() > 64
                || !p
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
                || !ids.insert(p.id.clone())
            {
                return Err("Project IDs must be unique URL-safe identifiers".into());
            }
            p.root = p
                .root
                .canonicalize()
                .map_err(|_| format!("Project {} root does not exist", p.id))?;
            if !p.root.join(".ax/ax.db").is_file() {
                return Err(format!("Project {} needs ax init", p.id));
            }
            if p.subjects.is_empty() || p.subjects.iter().any(|s| s.is_empty()) {
                return Err(format!("Project {} needs explicit OAuth subjects", p.id));
            }
            if p.write_subjects.iter().any(|s| !p.subjects.contains(s)) {
                return Err("Write subjects must also have context access".into());
            }
        }
        let mut keys = HashSet::new();
        for key in &self.api_keys {
            if key.id.is_empty()
                || !keys.insert(&key.id)
                || key.sha256.len() != 64
                || hex::decode(&key.sha256).is_err()
                || key.projects.is_empty()
                || key.projects.iter().any(|p| !ids.contains(p))
            {
                return Err("Invalid API key hash, ID or project grant".into());
            }
        }
        for origin in &self.allowed_origins {
            let u = https_url(origin)?;
            if u.path() != "/" {
                return Err("Allowed origins must be HTTPS origins".into());
            }
        }
        Ok(())
    }

    pub fn metadata_url(&self) -> String {
        format!("{}/.well-known/oauth-protected-resource", self.public_url)
    }
}
