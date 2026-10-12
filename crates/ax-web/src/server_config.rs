//! Configuration model for standalone Ax nodes.
//!
//! Discovery never establishes trust: replicas contact only the explicitly
//! configured primary and credentials are loaded from environment variables.

use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Primary,
    Replica,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRegistration {
    pub id: String,
    pub path: PathBuf,
    /// Name of an environment variable. Never persist an authentication token.
    pub token_env: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub schema_version: u32,
    pub cluster_id: String,
    pub node_id: String,
    pub role: NodeRole,
    pub primary_url: Option<String>,
    pub projects: Vec<ProjectRegistration>,
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn validate_primary_url(value: &str) -> Result<(), String> {
    let url = reqwest::Url::parse(value).map_err(|_| "invalid primary URL")?;
    if url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(
            "primary URL must be an origin without credentials, query, path or fragment".into(),
        );
    }
    let host = url.host_str().ok_or("primary URL must have a host")?;
    let local = host == "localhost"
        || host
            .trim_matches(['[',']']).parse::<std::net::IpAddr>()
            .is_ok_and(|ip| match ip { std::net::IpAddr::V4(ip)=>ip.is_loopback() || ip.is_private(), std::net::IpAddr::V6(ip)=>ip.is_loopback() || ip.is_unique_local() });
    if url.scheme() != "https" && !(url.scheme() == "http" && local) {
        return Err("primary URL must use HTTPS; HTTP is allowed only for explicit private or loopback IP addresses".into());
    }
    Ok(())
}

impl ServerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported server configuration version".into());
        }
        if !valid_id(&self.cluster_id) || !valid_id(&self.node_id) {
            return Err(
                "cluster_id and node_id must contain only letters, digits, '-' or '_'".into(),
            );
        }
        match (self.role, self.primary_url.as_deref()) {
            (NodeRole::Replica, Some(url)) => validate_primary_url(url)?,
            (NodeRole::Replica, None) => {
                return Err("replica requires an explicitly configured primary_url".into());
            }
            (NodeRole::Primary, Some(_)) => {
                return Err("primary must not configure primary_url".into());
            }
            (NodeRole::Primary, None) => {}
        }
        if self.projects.is_empty() {
            return Err("at least one project must be registered".into());
        }
        let mut ids = HashSet::new();
        let mut token_names = HashSet::new();
        for project in &self.projects {
            if !valid_id(&project.id) {
                return Err("invalid project id".into());
            }
            if !ids.insert(&project.id) {
                return Err("duplicate project id".into());
            }
            if !project.path.is_dir() {
                return Err(format!(
                    "project directory does not exist: {}",
                    project.path.display()
                ));
            }
            if !project.path.join(".ax").is_dir() {
                return Err(format!(
                    "project must be initialized with ax init: {}",
                    project.path.display()
                ));
            }
            if project.token_env.len() < 3
                || !project
                    .token_env
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err("token_env must name an uppercase environment variable".into());
            }
            if !token_names.insert(&project.token_env) {
                return Err("each project requires a separate token environment variable".into());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(role: NodeRole, primary_url: Option<&str>) -> ServerConfig {
        let dir = std::env::temp_dir();
        ServerConfig {
            schema_version: 1,
            cluster_id: "cluster".into(),
            node_id: "node".into(),
            role,
            primary_url: primary_url.map(str::to_owned),
            projects: vec![ProjectRegistration {
                id: "project".into(),
                path: dir,
                token_env: "AX_PROJECT_TOKEN".into(),
            }],
        }
    }

    #[test]
    fn replica_requires_explicit_primary() {
        assert!(config(NodeRole::Replica, None).validate().is_err());
    }

    #[test]
    fn rejects_untrusted_primary_origins() {
        for url in [
            "http://example.com",
            "https://user:secret@example.com",
            "https://example.com/path",
            "https://example.com/?token=secret",
        ] {
            assert!(validate_primary_url(url).is_err(), "{url}");
        }
        assert!(validate_primary_url("http://127.0.0.1:9070").is_ok());
        assert!(validate_primary_url("https://example.com").is_ok());
    }

    #[test]
    fn primary_cannot_point_to_another_primary() {
        assert!(config(NodeRole::Primary, Some("https://example.com"))
            .validate()
            .is_err());
    }
}
