//! Quality tooling — SonarQube integration and container runtimes.

mod bootstrap;
mod container;
mod sonar;

pub use bootstrap::{
    bootstrap_sonar, inspect_setup, read_sonar_token, scanner_available, scanner_host_url,
    sonar_token_path, token_configured, SonarBootstrapConfig, SonarBootstrapResult,
    SonarSetupStatus,
};
pub use container::{
    discover_runtimes, discover_sonar, ensure_sonar_live, ensure_sonar_live_with_log,
    find_container, find_container_any, resolve_runtime, start_sonar_container_with_log,
    stop_sonar_container_with_log, ContainerInfo, ContainerRuntime, InstallLog, RuntimeInfo,
    SonarDiscovery,
};
pub use sonar::{QualityGateResult, SonarClient, SonarCondition, SonarConfig};
