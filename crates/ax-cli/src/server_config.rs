//! Configuration model for standalone Ax nodes.
//!
//! This module intentionally contains no network listener or implicit discovery.
//! A replica must have an explicitly configured primary URL before syncing.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Primary,
    Replica,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRegistration {
    pub id: String,
    pub path: std::path::PathBuf,
    /// Environment variable name, never a token value.
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

impl ServerConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("unsupported server configuration version".into());
        }
        for (name, value) in [
            ("cluster_id", self.cluster_id.as_str()),
            ("node_id", self.node_id.as_str()),
        ] {
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
                return Err(format!("{name} must contain only letters, digits, '-' or '_'"));
            }
        }
        if self.role == NodeRole::Replica && self.primary_url.as_deref().unwrap_or("").is_empty() {
            return Err("replica requires an explicitly configured primary_url".into());
        }
        if self.role == NodeRole::Primary && self.primary_url.is_some() {
            return Err("primary must not configure primary_url".into());
        }
        let mut ids = std::collections::HashSet::new();
        let mut token_names = std::collections::HashSet::new();
        if self.projects.is_empty() {
            return Err("at least one project must be registered".into());
        }
        for project in &self.projects {
            if project.id.is_empty() || !project.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
                return Err("invalid project id".into());
            }
            if !ids.insert(&project.id) {
                return Err("duplicate project id".into());
            }
            if !project.path.is_dir() {
                return Err(format!("project directory does not exist: {}", project.path.display()));
            }
            if project.token_env.len() < 3 || !project.token_env.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_') {
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
    #[test]
    fn replica_requires_primary() {
        let config = ServerConfig {
            schema_version: 1,
            cluster_id: "cluster".into(),
            node_id: "node".into(),
            role: NodeRole::Replica,
            primary_url: None,
            projects: vec![ProjectRegistration {
                id: "project".into(),
                path: std::env::temp_dir(),
                token_env: "AX_PROJECT_TOKEN".into(),
            }],
        };
        assert!(config.validate().is_err());
    }
}
