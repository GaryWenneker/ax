//! Quality gate pipeline orchestration.

use std::path::PathBuf;
use std::process::Command;

use ax_core::Ax;
use ax_git::{diff_vs_base, map_files_to_nodes, map_hunks_to_nodes, GitError};
use ax_quality::SonarClient;
use ax_remote::{build_provider, load_ship_config, DraftPrRequest, ShipConfig};
use ax_tia::{affected_files_from_changes, TiaOptions};

use crate::advanced::{check_business_rules, detect_breaking_changes, find_affected_routes};
use crate::events::{ShipEvent, ShipEventBus};
use crate::state::{GateStepStatus, QualityGateSummary, ShipReport};

#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub git: Option<ax_git::GitContext>,
    pub changed_files: Vec<String>,
    pub dirty_nodes: Vec<ax_git::DirtyNode>,
    pub tia: Option<ax_tia::TiaResult>,
    pub quality_gate: QualityGateSummary,
    pub breaking_warnings: Vec<crate::state::BreakingWarning>,
    pub business_rule_warnings: Vec<crate::state::BusinessRuleWarning>,
    pub affected_routes: Vec<String>,
}

pub struct ShipPipeline {
    project_root: PathBuf,
    config: ShipConfig,
    bus: ShipEventBus,
}

impl ShipPipeline {
    pub fn new(project_root: PathBuf, config: ShipConfig, bus: ShipEventBus) -> Self {
        Self {
            project_root,
            config,
            bus,
        }
    }

    pub async fn run_evaluate(&self) -> Result<ShipReport, String> {
        let mut steps = Vec::new();
        let base = self.config.ship.target_branch.clone();

        self.step_start("index");
        let mut ax = Ax::open(&self.project_root).await.map_err(|e| e.to_string())?;
        if let Err(e) = ax
            .sync(ax_extraction::IndexOptions::default(), None)
            .await
        {
            self.step_fail("index", &e.to_string());
            return Err(e.to_string());
        }
        self.step_ok("index", None);

        self.step_start("diff");
        let diff = match diff_vs_base(&self.project_root, &base) {
            Ok(d) => d,
            Err(GitError::RefNotFound(_)) => {
                self.step_ok("diff", Some("base ref not found — skipping".into()));
                return Ok(empty_report());
            }
            Err(e) => {
                self.step_fail("diff", &e.to_string());
                return Err(e.to_string());
            }
        };
        let changed_files: Vec<String> = diff
            .files
            .iter()
            .filter(|f| f.change != "deleted")
            .map(|f| f.path.clone())
            .collect();
        self.step_ok("diff", Some(format!("{} files", changed_files.len())));

        let pool = ax.db_pool().clone();
        let dirty_nodes = if diff.hunks.is_empty() {
            map_files_to_nodes(&pool, &changed_files)
                .await
                .map_err(|e| e.to_string())?
        } else {
            map_hunks_to_nodes(&pool, &diff.hunks)
                .await
                .map_err(|e| e.to_string())?
        };

        self.step_start("tia");
        let tia_opts = TiaOptions::default().with_depth(5);
        let tia = affected_files_from_changes(&pool, &changed_files, &tia_opts)
            .await
            .map_err(|e| e.to_string())?;
        self.step_ok("tia", Some(format!("{} tests", tia.tests.len())));

        self.step_start("tests");
        let tests_ok = run_impacted_tests(&self.config, &tia);
        if tests_ok {
            self.step_ok("tests", None);
        } else {
            self.step_fail("tests", "one or more tests failed");
        }

        self.step_start("sonar");
        let sonar = SonarClient::new(self.config.sonar.clone());
        let _ = sonar.ensure_running().await;
        if let Err(e) = sonar.run_incremental_scan(&self.project_root, &changed_files) {
            self.step_fail("sonar", &e);
        } else {
            self.step_ok("sonar", None);
        }
        let sonar_result = sonar.fetch_quality_gate().await.ok();

        self.step_start("policy");
        let business_rule_warnings = check_business_rules(&pool, &dirty_nodes)
            .await
            .unwrap_or_default();
        self.step_ok("policy", None);

        let breaking_warnings = detect_breaking_changes(&pool, &dirty_nodes)
            .await
            .unwrap_or_default();
        let affected_routes = find_affected_routes(&pool, &dirty_nodes)
            .await
            .unwrap_or_default();

        steps.push(GateStepStatus {
            step: "index".into(),
            status: "passed".into(),
            detail: None,
        });
        steps.push(GateStepStatus {
            step: "tia".into(),
            status: "passed".into(),
            detail: Some(format!("{} tests", tia.tests.len())),
        });
        steps.push(GateStepStatus {
            step: "tests".into(),
            status: if tests_ok { "passed" } else { "failed" }.into(),
            detail: None,
        });
        steps.push(GateStepStatus {
            step: "sonar".into(),
            status: sonar_result
                .as_ref()
                .map(|r| if r.passed { "passed" } else { "failed" })
                .unwrap_or("skipped")
                .into(),
            detail: None,
        });

        let passed = tests_ok && sonar_result.as_ref().map(|r| r.passed).unwrap_or(true);

        let result = PipelineResult {
            git: Some(diff.context),
            changed_files,
            dirty_nodes,
            tia: Some(tia),
            quality_gate: QualityGateSummary {
                steps,
                sonar: sonar_result,
                passed,
            },
            breaking_warnings,
            business_rule_warnings,
            affected_routes,
        };

        Ok(ShipReport::from_pipeline(result))
    }

    pub async fn create_draft_pr(&self, title: &str, body: &str) -> Result<ax_remote::PrRef, String> {
        let report = self.run_evaluate().await?;
        if !report.quality_gate.passed {
            return Err("quality gate failed — cannot create PR".into());
        }
        let provider = build_provider(&self.config.remote)?;
        let branch = ax_git::current_branch(&self.project_root)
            .map_err(|e| e.to_string())?
            .ok_or("detached HEAD")?;
        provider
            .create_draft_pr(DraftPrRequest {
                title: title.to_string(),
                body: body.to_string(),
                head_branch: branch,
                base_branch: self.config.ship.target_branch.clone(),
                draft: true,
            })
            .await
    }

    fn step_start(&self, step: &str) {
        self.bus
            .publish(ShipEvent::StepStarted { step: step.into() });
    }

    fn step_ok(&self, step: &str, detail: Option<String>) {
        self.bus.publish(ShipEvent::StepFinished {
            step: step.into(),
            ok: true,
            detail,
        });
    }

    fn step_fail(&self, step: &str, detail: &str) {
        self.bus.publish(ShipEvent::StepFinished {
            step: step.into(),
            ok: false,
            detail: Some(detail.into()),
        });
    }
}

fn run_impacted_tests(config: &ShipConfig, tia: &ax_tia::TiaResult) -> bool {
    if tia.tests.is_empty() {
        return true;
    }
    let names: Vec<&str> = tia.tests.iter().map(|t| t.name.as_str()).collect();
    let filter = names.join("|");
    let status = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "{} -- {} -- --exact",
            config.quality_gate.tests.runner, filter
        ))
        .status();
    matches!(status, Ok(s) if s.success())
}

fn empty_report() -> ShipReport {
    ShipReport::from_pipeline(PipelineResult {
        git: None,
        changed_files: vec![],
        dirty_nodes: vec![],
        tia: None,
        quality_gate: QualityGateSummary::default(),
        breaking_warnings: vec![],
        business_rule_warnings: vec![],
        affected_routes: vec![],
    })
}
