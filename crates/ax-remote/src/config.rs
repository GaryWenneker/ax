//! Remote PR provider configuration.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct RemoteConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    pub github: Option<GithubSection>,
    pub azure_devops: Option<AzureDevOpsSection>,
}

fn default_provider() -> String {
    "azure_devops".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GithubSection {
    pub owner: String,
    pub repo: String,
    #[serde(default = "default_gh_token_env")]
    pub token_env: String,
}

fn default_gh_token_env() -> String {
    "GITHUB_TOKEN".into()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AzureDevOpsSection {
    pub org: String,
    pub project: String,
    pub repo_id: String,
    #[serde(default = "default_azdo_token_env")]
    pub token_env: String,
}

fn default_azdo_token_env() -> String {
    "AZDO_PAT".into()
}

pub fn config_path(project_root: &std::path::Path) -> std::path::PathBuf {
    project_root.join(".ax").join("ship.toml")
}
