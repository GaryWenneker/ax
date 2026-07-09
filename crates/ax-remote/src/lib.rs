//! PR platform integrations (GitHub + Azure DevOps).

mod config;
mod github;
mod azure_devops;
mod provider;
mod seed;
mod ship_config;

pub use config::{AzureDevOpsSection, GithubSection, RemoteConfig};
pub use provider::{DraftPrRequest, PrProvider, PrRef, ReviewComment};
pub use seed::{seed_ship_config, ShipSeedResult};
pub use ship_config::{
    load_ship_config, save_ship_config, QualityGateSection, ShipConfig, ShipSection,
    TestRunnerSection,
};

use std::path::Path;
use std::sync::Arc;

pub fn build_provider(config: &RemoteConfig) -> Result<Arc<dyn PrProvider>, String> {
    match config.provider.as_str() {
        "github" => {
            let gh = config.github.as_ref().ok_or("missing [remote.github] config")?;
            let token = std::env::var(&gh.token_env).map_err(|_| format!("set {}", gh.token_env))?;
            Ok(Arc::new(github::GithubProvider::new(token, gh.owner.clone(), gh.repo.clone()).map_err(|e| e.to_string())?))
        }
        "azure_devops" => {
            let az = config
                .azure_devops
                .as_ref()
                .ok_or("missing [remote.azure_devops] config")?;
            let pat = std::env::var(&az.token_env).map_err(|_| format!("set {}", az.token_env))?;
            Ok(Arc::new(azure_devops::AzureDevOpsProvider::new(
                pat,
                az.org.clone(),
                az.project.clone(),
                az.repo_id.clone(),
            )))
        }
        other => Err(format!("unknown remote provider: {other}")),
    }
}

pub fn ship_config_path(project_root: &Path) -> std::path::PathBuf {
    config_path(project_root)
}

use config::config_path;
