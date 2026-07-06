//! Local SonarQube integration (Podman-compatible).

mod sonar;

pub use sonar::{QualityGateResult, SonarClient, SonarCondition, SonarConfig};
