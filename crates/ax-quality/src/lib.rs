//! Quality tooling — SonarQube integration and container runtimes.

mod bootstrap;
mod container;
mod sonar;

pub use bootstrap::{
    auto_provision_sonar_from_discovery, bootstrap_sonar, canonical_repo_project_key,
    ensure_sonar_dark_theme, ensure_sonar_projects, ensure_sonar_ready_for_scan,
    ensure_sonar_token, inspect_setup, legacy_workspace_prefixes, lookup_project, lookup_projects,
    read_sonar_token, regenerate_sonar_token, resolve_sonar_project, resolve_sonar_repo_projects,
    scanner_available, scanner_host_url, sonar_key_from_name, sonar_reachable, sonar_token_path,
    token_configured, validate_sonar_login, validate_sonar_token, workspace_sonar_key,
    ProjectLookup, RepoProjectStatus, SonarBootstrapConfig, SonarBootstrapResult, SonarSetupStatus,
};
pub use container::{
    db_container_name, discover_runtimes, discover_sonar, ensure_sonar_live,
    ensure_sonar_live_with_log, ensure_sonar_stack_online, find_container, find_container_any,
    resolve_runtime, sonar_host_port, sonar_localhost_candidates, sonar_ping_fast,
    start_sonar_container_with_log, start_sonar_stack, stop_sonar_container_with_log,
    ContainerInfo, ContainerRuntime, InstallLog, RuntimeInfo, SonarDiscovery,
};
pub use sonar::{
    QualityGateResult, SonarClient, SonarCondition, SonarConfig, SonarScanProgressEvent,
};
