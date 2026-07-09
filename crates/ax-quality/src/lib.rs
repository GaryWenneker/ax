//! Quality tooling — SonarQube integration and container runtimes.

mod container;
mod sonar;

pub use container::{
    discover_runtimes, discover_sonar, ensure_sonar_live, find_container, resolve_runtime,
    ContainerInfo, ContainerRuntime, RuntimeInfo, SonarDiscovery,
};
pub use sonar::{QualityGateResult, SonarClient, SonarCondition, SonarConfig};
