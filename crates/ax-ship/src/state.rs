//! Shared ship state for API and SSE.

use ax_git::GitContext;
use ax_quality::QualityGateResult;
use ax_tia::TiaResult;

use crate::pipeline::PipelineResult;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ShipState {
    pub report: Option<ShipReport>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShipReport {
    pub git: Option<GitContext>,
    pub changed_files: Vec<String>,
    pub dirty_nodes: Vec<ax_git::DirtyNode>,
    pub tia: Option<TiaResult>,
    pub quality_gate: QualityGateSummary,
    pub breaking_warnings: Vec<BreakingWarning>,
    pub business_rule_warnings: Vec<BusinessRuleWarning>,
    pub affected_routes: Vec<String>,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct QualityGateSummary {
    pub steps: Vec<GateStepStatus>,
    pub sonar: Option<QualityGateResult>,
    pub passed: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GateStepStatus {
    pub step: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BreakingWarning {
    pub node_id: String,
    pub node_name: String,
    pub reason: String,
    pub locations: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BusinessRuleWarning {
    pub rule_id: String,
    pub rule_text: String,
    pub node_name: String,
    pub severity: String,
}

impl ShipState {
    pub fn from_report(report: &ShipReport) -> Self {
        Self {
            report: Some(report.clone()),
        }
    }
}

impl ShipReport {
    pub fn from_pipeline(result: PipelineResult) -> Self {
        Self {
            git: result.git,
            changed_files: result.changed_files,
            dirty_nodes: result.dirty_nodes,
            tia: result.tia,
            quality_gate: result.quality_gate,
            breaking_warnings: result.breaking_warnings,
            business_rule_warnings: result.business_rule_warnings,
            affected_routes: result.affected_routes,
        }
    }
}
