//! Quality gate pipeline orchestration.

use std::path::PathBuf;
use std::process::Command;

use ax_core::Ax;
use ax_extraction::IndexOptions;
use ax_git::{map_files_to_nodes, map_hunks_to_nodes};
use ax_quality::SonarClient;
use ax_remote::{build_provider, DraftPrRequest, ShipConfig};
use ax_tia::{affected_files_from_changes, TiaOptions};

use crate::advanced::{check_business_rules, detect_breaking_changes, find_affected_routes};
use crate::events::{ShipEvent, ShipEventBus};
use crate::git_root::{diff_all_repos, resolve_git_root, resolve_git_roots};
use crate::run_log::RunLogger;
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
        let mut logger = RunLogger::start(&self.project_root, Some(self.bus.clone()));
        logger.line("Evaluate started");
        let mut steps = Vec::new();
        let base = self.config.ship.target_branch.clone();
        let full_index = self.config.quality_gate.index_mode == "full";
        let full_sonar = self.config.sonar.scan_mode == "full";

        self.step_start("index");
        logger.step_start("index");
        let mut ax = Ax::open(&self.project_root).await.map_err(|e| e.to_string())?;
        let index_result = if full_index {
            logger.line("Full index — scanning entire project");
            ax.index_all(IndexOptions::default(), None)
                .await
                .map_err(|e| e.to_string())
        } else {
            logger.line("Incremental index — syncing changed files");
            ax.sync(IndexOptions::default(), None)
                .await
                .map_err(|e| e.to_string())
        };
        if let Err(ref e) = index_result {
            self.step_fail("index", e);
            logger.step_fail("index", e);
            logger.finish(false);
            return Err(e.clone());
        }
        let index_detail = index_result.ok().map(|r| {
            if r.files_indexed == 0 {
                if full_index {
                    "Full index — already up to date".to_string()
                } else {
                    "Already up to date".to_string()
                }
            } else if full_index {
                format!("{} files indexed", r.files_indexed)
            } else {
                format!("{} files synced", r.files_indexed)
            }
        });
        self.step_ok("index", index_detail.clone());
        logger.step_ok("index", index_detail.as_deref());

        let git_roots = resolve_git_roots(&self.project_root, &self.config)?;

        self.step_start("diff");
        logger.step_start("diff");
        if git_roots.len() > 1 {
            logger.line(format!("Diff across {} git repositories", git_roots.len()));
        }
        let multi = match diff_all_repos(&git_roots, &base, &self.config.ship.target_branches) {
            Ok(d) => d,
            Err(e) if e.contains("base ref") => {
                let detail = "base ref not found — skipping".to_string();
                self.step_ok("diff", Some(detail.clone()));
                logger.step_ok("diff", Some(&detail));
                logger.finish(true);
                return Ok(empty_report());
            }
            Err(e) => {
                self.step_fail("diff", &e);
                logger.step_fail("diff", &e);
                logger.finish(false);
                return Err(e);
            }
        };
        let changed_files: Vec<String> = multi
            .files
            .iter()
            .filter(|f| f.change != "deleted")
            .map(|f| f.path.clone())
            .collect();
        let diff_detail = if git_roots.len() > 1 {
            format!(
                "{} files across {}/{} repos",
                changed_files.len(),
                multi.repos_with_changes,
                multi.repos_scanned
            )
        } else {
            format!("{} files", changed_files.len())
        };
        self.step_ok("diff", Some(diff_detail.clone()));
        logger.step_ok("diff", Some(&diff_detail));

        let pool = ax.db_pool().clone();
        let dirty_nodes = if multi.hunks.is_empty() {
            map_files_to_nodes(&pool, &changed_files)
                .await
                .map_err(|e| e.to_string())?
        } else {
            map_hunks_to_nodes(&pool, &multi.hunks)
                .await
                .map_err(|e| e.to_string())?
        };

        self.step_start("tia");
        logger.step_start("tia");
        let tia_opts = TiaOptions::default().with_depth(5);
        let tia = affected_files_from_changes(&pool, &changed_files, &tia_opts)
            .await
            .map_err(|e| e.to_string())?;
        let tia_detail = if tia.tests.is_empty() {
            if changed_files.is_empty() {
                "No impacted tests — no changed files".to_string()
            } else {
                format!("No impacted tests — {} changed file(s)", changed_files.len())
            }
        } else {
            format!("{} impacted test(s)", tia.tests.len())
        };
        self.step_ok("tia", Some(tia_detail.clone()));
        logger.step_ok("tia", Some(&tia_detail));

        self.step_start("tests");
        logger.step_start("tests");
        let tests_ok = run_impacted_tests(&self.config, &tia);
        let tests_detail = if tia.tests.is_empty() {
            Some("Skipped — no impacted tests".to_string())
        } else if tests_ok {
            Some(format!("{} test(s) passed", tia.tests.len()))
        } else {
            Some(format!("{} test(s) failed", tia.tests.len()))
        };
        if tests_ok {
            self.step_ok("tests", tests_detail.clone());
            logger.step_ok("tests", tests_detail.as_deref());
        } else {
            self.step_fail("tests", tests_detail.as_deref().unwrap_or("tests failed"));
            logger.step_fail("tests", tests_detail.as_deref().unwrap_or("tests failed"));
        }

        let repo_names: Vec<String> = git_roots
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str().map(str::to_string)))
            .collect();

        self.step_start("sonar");
        logger.step_start("sonar");
        let sonar = SonarClient::new(self.config.sonar.clone());
        let (sonar_status, sonar_detail, sonar_result) = if self.config.sonar.enabled {
            logger.line(format!(
                "Preparing SonarQube — {} git repositor{} (projects + token)",
                repo_names.len().max(1),
                if repo_names.len() == 1 { "y" } else { "ies" }
            ));
            if !ax_quality::sonar_reachable(&self.config.sonar.host).await {
                let detail = format!(
                    "SonarQube offline at {} — install and start from the SonarQube page",
                    self.config.sonar.host
                );
                self.step_fail("sonar", &detail);
                logger.step_fail("sonar", &detail);
                (
                    "failed".to_string(),
                    Some(detail),
                    None,
                )
            } else if let Err(e) = ax_quality::ensure_sonar_ready_for_scan(
                &self.config.sonar,
                &self.project_root,
                &repo_names,
            )
            .await
            {
                self.step_fail("sonar", &e);
                logger.step_fail("sonar", &e);
                ("failed".to_string(), Some(e), None)
            } else {
            let bus = self.bus.clone();
            if full_sonar {
                logger.line(format!(
                    "Full SonarQube scan — all {} git repositor{}",
                    repo_names.len().max(1),
                    if repo_names.len() == 1 { "y" } else { "ies" }
                ));
            } else {
                logger.line(format!(
                    "SonarQube scan — all {} git repositor{} ({} changed file(s) in diff)",
                    repo_names.len().max(1),
                    if repo_names.len() == 1 { "y" } else { "ies" },
                    changed_files.len()
                ));
            }
            let sonar_scan = run_sonar_platform_scan_async(
                self.config.sonar.clone(),
                self.project_root.clone(),
                repo_names.clone(),
                if full_sonar {
                    Vec::new()
                } else {
                    changed_files.clone()
                },
                full_sonar,
                bus,
                &mut logger,
            )
            .await;
            let scan_ok = sonar_scan.is_ok();
            if let Err(e) = sonar_scan {
                self.step_fail("sonar", &e);
                logger.step_fail("sonar", &e);
            } else {
                self.step_ok("sonar", Some("Scan completed".to_string()));
                logger.step_ok("sonar", Some("Scan completed"));
            }
            let sonar_result = sonar
                .fetch_quality_gate(&self.project_root, &repo_names)
                .await
                .ok();
            let (status, detail) = if !scan_ok {
                (
                    "failed",
                    Some("Scanner failed — see log".to_string()),
                )
            } else if let Some(ref gate) = sonar_result {
                (
                    if gate.passed { "passed" } else { "failed" },
                    Some(format!("Quality gate {}", gate.status)),
                )
            } else {
                ("failed", Some("Could not fetch quality gate".to_string()))
            };
            (status.to_string(), detail, sonar_result)
            }
        } else {
            self.step_ok("sonar", Some("Disabled in settings".to_string()));
            logger.step_ok("sonar", Some("Disabled in settings"));
            (
                "skipped".to_string(),
                Some("Enable SonarQube in settings".to_string()),
                None,
            )
        };

        self.step_start("policy");
        logger.step_start("policy");
        let business_rule_warnings = check_business_rules(&pool, &dirty_nodes)
            .await
            .unwrap_or_default();
        let policy_detail = if business_rule_warnings.is_empty() {
            "No policy violations".to_string()
        } else {
            format!("{} warning(s)", business_rule_warnings.len())
        };
        self.step_ok("policy", Some(policy_detail.clone()));
        logger.step_ok("policy", Some(&policy_detail));

        let breaking_warnings = detect_breaking_changes(&pool, &dirty_nodes)
            .await
            .unwrap_or_default();
        let affected_routes = find_affected_routes(&pool, &dirty_nodes)
            .await
            .unwrap_or_default();

        let sonar_passed = match sonar_status.as_str() {
            "passed" => sonar_result.as_ref().map(|r| r.passed).unwrap_or(true),
            "skipped" => true,
            _ => false,
        };
        steps.push(GateStepStatus {
            step: "index".into(),
            status: "passed".into(),
            detail: index_detail,
        });
        steps.push(GateStepStatus {
            step: "tia".into(),
            status: "passed".into(),
            detail: Some(tia_detail.clone()),
        });
        steps.push(GateStepStatus {
            step: "tests".into(),
            status: if tests_ok { "passed" } else { "failed" }.into(),
            detail: tests_detail,
        });
        steps.push(GateStepStatus {
            step: "sonar".into(),
            status: sonar_status,
            detail: sonar_detail,
        });
        steps.push(GateStepStatus {
            step: "policy".into(),
            status: "passed".into(),
            detail: Some(policy_detail),
        });

        let passed = tests_ok && sonar_passed;

        let result = PipelineResult {
            git: multi.context,
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

        logger.line(if passed {
            "Quality gate passed"
        } else {
            "Quality gate failed"
        });
        logger.finish(passed);
        Ok(ShipReport::from_pipeline(result))
    }

    pub async fn create_draft_pr(&self, title: &str, body: &str) -> Result<ax_remote::PrRef, String> {
        let report = self.run_evaluate().await?;
        if !report.quality_gate.passed {
            return Err("quality gate failed — cannot create PR".into());
        }
        let provider = build_provider(&self.config.remote)?;
        let git_root = resolve_git_root(&self.project_root, &self.config)?;
        let branch = ax_git::current_branch(&git_root)
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

fn dispatch_sonar_progress(
    ev: &ax_quality::SonarScanProgressEvent,
    bus: &ShipEventBus,
    logger: &mut RunLogger,
) {
    use ax_quality::SonarScanProgressEvent;
    match ev {
        SonarScanProgressEvent::Started {
            index,
            total,
            project_key,
            repo_name,
        } => {
            bus.publish(ShipEvent::SonarProjectStarted {
                project_key: project_key.clone(),
                repo_name: repo_name.clone(),
                index: *index,
                total: *total,
            });
            logger.sonar_project_start(*index, *total, project_key, repo_name);
        }
        SonarScanProgressEvent::Finished {
            project_key,
            repo_name,
            ok,
            error,
        } => {
            bus.publish(ShipEvent::SonarProjectFinished {
                project_key: project_key.clone(),
                repo_name: repo_name.clone(),
                ok: *ok,
                detail: error.clone(),
            });
            if *ok {
                logger.sonar_project_ok(project_key, repo_name);
            } else {
                logger.sonar_project_fail(
                    project_key,
                    repo_name,
                    error.as_deref().unwrap_or("scan failed"),
                );
            }
        }
        SonarScanProgressEvent::Skipped {
            project_key,
            repo_name,
            reason,
        } => {
            bus.publish(ShipEvent::SonarProjectSkipped {
                project_key: project_key.clone(),
                repo_name: repo_name.clone(),
                reason: reason.clone(),
            });
            logger.sonar_project_skip(project_key, repo_name, reason);
        }
    }
}

async fn run_sonar_platform_scan_async(
    sonar_config: ax_quality::SonarConfig,
    project_root: PathBuf,
    repo_names: Vec<String>,
    changed_files: Vec<String>,
    full_repo: bool,
    bus: ShipEventBus,
    logger: &mut RunLogger,
) -> Result<(), String> {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let scan_task = tokio::task::spawn_blocking(move || {
        let sonar = SonarClient::new(sonar_config);
        let mut progress = move |ev: ax_quality::SonarScanProgressEvent| {
            let _ = tx.send(ev);
        };
        let mut progress_cb: Option<&mut dyn FnMut(ax_quality::SonarScanProgressEvent)> =
            Some(&mut progress);
        sonar.run_platform_scan_with_progress(
            &project_root,
            &repo_names,
            &changed_files,
            full_repo,
            &mut progress_cb,
        )
    });

    while let Some(ev) = rx.recv().await {
        dispatch_sonar_progress(&ev, &bus, logger);
    }

    scan_task
        .await
        .map_err(|e| format!("Sonar scan task failed: {e}"))?
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
