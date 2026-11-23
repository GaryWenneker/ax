//! Independent replay validation (Plan C).
//!
//! Scores frozen criteria and seals evidence. Has no write path into the
//! context resolver or authority selection logic.

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq)]
pub struct CriterionResult {
    pub id: String,
    pub weight: f64,
    pub critical: bool,
    pub passed: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scorecard {
    pub functional_passed_weight: f64,
    pub functional_total_weight: f64,
    pub functional_percent: f64,
    pub functional_unverified: u32,
    pub critical_passed: u32,
    pub critical_total: u32,
    pub critical_percent: Option<f64>,
    pub critical_omission_audit_required: bool,
    pub verdict: Verdict,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CriterionDef {
    pub id: String,
    pub weight: f64,
    pub critical: bool,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FrozenTask {
    pub task_id: String,
    pub stratum: String,
    pub benchmark_version: String,
    pub criteria: Vec<CriterionDef>,
    pub definition_seal: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceRecord {
    pub benchmark_version: String,
    pub task_id: String,
    pub run_id: String,
    pub arm: String,
    pub definition_seal: String,
    pub scorecard: Scorecard,
    pub evidence_seal: String,
}

pub fn score_run(criteria: &[CriterionResult]) -> Result<Scorecard, String> {
    if criteria.is_empty() {
        return Err("no criteria".into());
    }
    let mut functional_passed = 0.0;
    let mut functional_total = 0.0;
    let mut functional_unverified = 0u32;
    let mut critical_passed = 0u32;
    let mut critical_total = 0u32;
    let mut critical_failed_or_unverified = false;

    for item in criteria {
        if item.critical {
            critical_total += 1;
            match item.passed {
                Some(true) => critical_passed += 1,
                Some(false) | None => critical_failed_or_unverified = true,
            }
            continue;
        }
        if item.weight <= 0.0 {
            return Err(format!("noncritical weight must be > 0 for {}", item.id));
        }
        functional_total += item.weight;
        match item.passed {
            Some(true) => functional_passed += item.weight,
            Some(false) => {}
            None => {
                functional_unverified += 1;
                critical_failed_or_unverified = true;
            }
        }
    }

    if functional_total <= 0.0 {
        return Err("no noncritical weight".into());
    }

    let functional_percent = 100.0 * functional_passed / functional_total;
    let critical_omission_audit_required = critical_total == 0;
    let critical_percent = if critical_total == 0 {
        None
    } else {
        Some(100.0 * f64::from(critical_passed) / f64::from(critical_total))
    };

    let verdict = if functional_unverified > 0 || critical_failed_or_unverified {
        if criteria
            .iter()
            .any(|item| item.critical && item.passed.is_none())
        {
            Verdict::Blocked
        } else {
            Verdict::Fail
        }
    } else if critical_omission_audit_required {
        Verdict::Blocked
    } else if functional_percent + f64::EPSILON < 95.0 {
        Verdict::Fail
    } else {
        Verdict::Pass
    };

    Ok(Scorecard {
        functional_passed_weight: functional_passed,
        functional_total_weight: functional_total,
        functional_percent,
        functional_unverified,
        critical_passed,
        critical_total,
        critical_percent,
        critical_omission_audit_required,
        verdict,
    })
}

/// Operator-facing scorecard lines. Does not claim a release gate without a Pass verdict.
pub fn format_scorecard_lines(card: &Scorecard) -> Vec<String> {
    let mut lines = vec![format!(
        "functional: {:.1}% ({:.0}/{:.0} weight)",
        card.functional_percent, card.functional_passed_weight, card.functional_total_weight
    )];
    if card.functional_unverified > 0 {
        lines.push(format!(
            "functional unverified: {}",
            card.functional_unverified
        ));
    }
    match card.critical_percent {
        Some(percent) => lines.push(format!(
            "critical: {:.1}% ({}/{})",
            percent, card.critical_passed, card.critical_total
        )),
        None => lines.push("critical: omission audit required".into()),
    }
    let verdict = match card.verdict {
        Verdict::Pass => "verdict: pass (measured)",
        Verdict::Fail => "verdict: fail",
        Verdict::Blocked => "verdict: blocked",
    };
    lines.push(verdict.into());
    if card.verdict != Verdict::Pass {
        lines.push("release gate: not claimed".into());
    }
    lines
}

/// Plan C V2 pilot layout: separate arm workspaces under a parent, never overlapping golden.
pub fn prepare_v2_pilot_arms(
    parent: &std::path::Path,
    golden_root: Option<&std::path::Path>,
) -> Result<Vec<ArmWorkspace>, String> {
    create_arm_workspaces(parent, &["G", "A", "B", "C"], golden_root)
}

/// Supported replay adapters for Plan C pilots. Expand only with measured support.
pub const PILOT_ADAPTERS: &[&str] = &["fixture", "cursor"];

/// First registered pilot task (TypeScript/React stratum). Not a release gate.
pub fn freeze_pilot_task_v1() -> Result<FrozenTask, String> {
    freeze_task(
        "pilot-ts-list-filter",
        "typescript-react",
        "pilot-v1",
        vec![
            CriterionDef {
                id: "filter-matches-query".into(),
                weight: 2.0,
                critical: false,
            },
            CriterionDef {
                id: "empty-query-shows-all".into(),
                weight: 1.0,
                critical: false,
            },
            CriterionDef {
                id: "no-secret-in-ui".into(),
                weight: 0.0,
                critical: true,
            },
        ],
    )
}

#[derive(Debug, Clone, PartialEq)]
pub struct PilotBundle {
    pub task: FrozenTask,
    pub arms: Vec<ArmWorkspace>,
    pub task_path: std::path::PathBuf,
    pub adapters: Vec<String>,
}

/// Prepare G/A/B/C arm dirs and write the frozen pilot task JSON (no golden code).
pub fn prepare_pilot_bundle(parent: &std::path::Path) -> Result<PilotBundle, String> {
    let arms = prepare_v2_pilot_arms(parent, None)?;
    let task = freeze_pilot_task_v1()?;
    let task_path = parent.join("frozen-task.json");
    let body = serde_json::to_string_pretty(&task).map_err(|error| error.to_string())?;
    std::fs::write(&task_path, body).map_err(|error| error.to_string())?;
    Ok(PilotBundle {
        task,
        arms,
        task_path,
        adapters: PILOT_ADAPTERS
            .iter()
            .map(|item| (*item).to_string())
            .collect(),
    })
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FixturePilotArmScore {
    pub arm: String,
    pub run_id: String,
    pub evidence_path: std::path::PathBuf,
    pub evidence_seal: String,
    pub verdict: String,
    pub functional_percent: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FixturePilotReport {
    pub task_id: String,
    pub adapter: String,
    pub live_agent_runs: u32,
    pub release_gate: String,
    pub arms: Vec<FixturePilotArmScore>,
}

fn fixture_pilot_results(arm: &str) -> Result<Vec<(String, Option<bool>)>, String> {
    match arm {
        "G" => Ok(vec![
            ("filter-matches-query".into(), Some(true)),
            ("empty-query-shows-all".into(), Some(true)),
            ("no-secret-in-ui".into(), Some(true)),
        ]),
        "A" => Ok(vec![
            ("filter-matches-query".into(), Some(true)),
            ("empty-query-shows-all".into(), Some(true)),
            ("no-secret-in-ui".into(), Some(false)),
        ]),
        "B" => Ok(vec![
            ("filter-matches-query".into(), Some(false)),
            ("empty-query-shows-all".into(), Some(true)),
            ("no-secret-in-ui".into(), Some(true)),
        ]),
        "C" => Ok(vec![
            ("filter-matches-query".into(), Some(true)),
            ("empty-query-shows-all".into(), Some(true)),
            ("no-secret-in-ui".into(), None),
        ]),
        other => Err(format!("unknown pilot arm {other}")),
    }
}

fn load_or_prepare_pilot_bundle(parent: &std::path::Path) -> Result<PilotBundle, String> {
    let task_path = parent.join("frozen-task.json");
    let arm_names = ["G", "A", "B", "C"];
    let prepared = task_path.is_file() && arm_names.iter().all(|arm| parent.join(arm).is_dir());
    if prepared {
        let body = std::fs::read_to_string(&task_path).map_err(|error| error.to_string())?;
        let task: FrozenTask = serde_json::from_str(&body).map_err(|error| error.to_string())?;
        let arms = arm_names
            .iter()
            .map(|arm| ArmWorkspace {
                arm: (*arm).to_string(),
                root: parent.join(arm),
            })
            .collect();
        return Ok(PilotBundle {
            task,
            arms,
            task_path,
            adapters: PILOT_ADAPTERS
                .iter()
                .map(|item| (*item).to_string())
                .collect(),
        });
    }
    prepare_pilot_bundle(parent)
}

/// Score a prepared (or freshly prepared) pilot bundle with the fixture adapter only.
/// Always reports `release_gate: "not claimed"` — fixture plumbing is not live Arm A evidence.
pub fn score_fixture_pilot(parent: &std::path::Path) -> Result<FixturePilotReport, String> {
    let bundle = load_or_prepare_pilot_bundle(parent)?;
    let mut arms = Vec::with_capacity(bundle.arms.len());
    for workspace in &bundle.arms {
        let results = fixture_pilot_results(&workspace.arm)?;
        let run_id = format!("fixture-{}-{}", bundle.task.task_id, workspace.arm);
        let evidence = run_fixture(&bundle.task, &run_id, &workspace.arm, &results)?;
        if !verify_evidence(&evidence) {
            return Err(format!("evidence seal failed for arm {}", workspace.arm));
        }
        let evidence_path = workspace
            .root
            .join(format!("evidence-{}.json", workspace.arm));
        let body = serde_json::json!({
            "benchmarkVersion": evidence.benchmark_version,
            "taskId": evidence.task_id,
            "runId": evidence.run_id,
            "arm": evidence.arm,
            "definitionSeal": evidence.definition_seal,
            "evidenceSeal": evidence.evidence_seal,
            "verdict": verdict_name(evidence.scorecard.verdict),
            "functionalPercent": evidence.scorecard.functional_percent,
            "adapter": "fixture",
            "releaseGate": "not claimed",
        });
        std::fs::write(
            &evidence_path,
            serde_json::to_string_pretty(&body).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        arms.push(FixturePilotArmScore {
            arm: workspace.arm.clone(),
            run_id: evidence.run_id,
            evidence_path,
            evidence_seal: evidence.evidence_seal,
            verdict: verdict_name(evidence.scorecard.verdict).to_string(),
            functional_percent: evidence.scorecard.functional_percent,
        });
    }
    Ok(FixturePilotReport {
        task_id: bundle.task.task_id,
        adapter: "fixture".into(),
        live_agent_runs: 0,
        release_gate: "not claimed".into(),
        arms,
    })
}

pub fn freeze_task(
    task_id: &str,
    stratum: &str,
    benchmark_version: &str,
    criteria: Vec<CriterionDef>,
) -> Result<FrozenTask, String> {
    require_text(task_id, "task")?;
    require_text(stratum, "stratum")?;
    require_text(benchmark_version, "benchmark")?;
    if criteria.is_empty() {
        return Err("no criteria".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut has_noncritical = false;
    for item in &criteria {
        require_text(&item.id, "criterion")?;
        if !seen.insert(item.id.clone()) {
            return Err(format!("duplicate criterion {}", item.id));
        }
        if item.critical {
            continue;
        }
        if item.weight <= 0.0 {
            return Err(format!("noncritical weight must be > 0 for {}", item.id));
        }
        has_noncritical = true;
    }
    if !has_noncritical {
        return Err("no noncritical weight".into());
    }
    let definition_seal = task_definition_seal(task_id, stratum, benchmark_version, &criteria);
    Ok(FrozenTask {
        task_id: task_id.trim().to_string(),
        stratum: stratum.trim().to_string(),
        benchmark_version: benchmark_version.trim().to_string(),
        criteria,
        definition_seal,
    })
}

pub fn run_fixture(
    task: &FrozenTask,
    run_id: &str,
    arm: &str,
    results: &[(String, Option<bool>)],
) -> Result<EvidenceRecord, String> {
    require_text(run_id, "run")?;
    require_text(arm, "arm")?;
    if task_definition_seal(
        &task.task_id,
        &task.stratum,
        &task.benchmark_version,
        &task.criteria,
    ) != task.definition_seal
    {
        return Err("definition seal mismatch".into());
    }
    let mut by_id = std::collections::BTreeMap::new();
    for (id, passed) in results {
        if by_id.insert(id.clone(), *passed).is_some() {
            return Err(format!("duplicate result {id}"));
        }
    }
    let mut criteria = Vec::with_capacity(task.criteria.len());
    for def in &task.criteria {
        let passed = by_id.remove(&def.id).flatten();
        criteria.push(CriterionResult {
            id: def.id.clone(),
            weight: def.weight,
            critical: def.critical,
            passed,
        });
    }
    if let Some((extra, _)) = by_id.iter().next() {
        return Err(format!("unknown criterion {extra}"));
    }
    let scorecard = score_run(&criteria)?;
    let evidence_seal = evidence_seal(
        &task.benchmark_version,
        &task.task_id,
        run_id,
        arm,
        &task.definition_seal,
        &scorecard,
    );
    Ok(EvidenceRecord {
        benchmark_version: task.benchmark_version.clone(),
        task_id: task.task_id.clone(),
        run_id: run_id.trim().to_string(),
        arm: arm.trim().to_string(),
        definition_seal: task.definition_seal.clone(),
        scorecard,
        evidence_seal,
    })
}

pub fn verify_evidence(record: &EvidenceRecord) -> bool {
    evidence_seal(
        &record.benchmark_version,
        &record.task_id,
        &record.run_id,
        &record.arm,
        &record.definition_seal,
        &record.scorecard,
    ) == record.evidence_seal
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmWorkspace {
    pub arm: String,
    pub root: std::path::PathBuf,
}

/// Plan C V2: create isolated workspaces for experimental arms under `parent`.
/// Each arm gets its own directory. The golden tree must not sit inside any arm root.
pub fn create_arm_workspaces(
    parent: &std::path::Path,
    arms: &[&str],
    golden_root: Option<&std::path::Path>,
) -> Result<Vec<ArmWorkspace>, String> {
    if arms.is_empty() {
        return Err("arms required".into());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut workspaces = Vec::with_capacity(arms.len());
    for arm in arms {
        let name = arm.trim();
        if name.is_empty() {
            return Err("arm is required".into());
        }
        if !seen.insert(name.to_string()) {
            return Err(format!("duplicate arm {name}"));
        }
        let root = parent.join(name);
        if root.exists() {
            return Err(format!("arm workspace already exists: {name}"));
        }
        std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        if let Some(golden) = golden_root {
            assert_no_golden_leak(&root, golden)?;
        }
        workspaces.push(ArmWorkspace {
            arm: name.to_string(),
            root,
        });
    }
    for left in 0..workspaces.len() {
        for right in (left + 1)..workspaces.len() {
            if paths_overlap(&workspaces[left].root, &workspaces[right].root) {
                return Err("arm workspaces must not overlap".into());
            }
        }
    }
    Ok(workspaces)
}

pub fn assert_no_golden_leak(
    arm_root: &std::path::Path,
    golden_root: &std::path::Path,
) -> Result<(), String> {
    let arm = canonicalize_path(arm_root)?;
    let golden = canonicalize_path(golden_root)?;
    if arm == golden || golden.starts_with(&arm) || arm.starts_with(&golden) {
        return Err("golden tree must not be readable from an arm workspace".into());
    }
    Ok(())
}

fn paths_overlap(left: &std::path::Path, right: &std::path::Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn canonicalize_path(path: &std::path::Path) -> Result<std::path::PathBuf, String> {
    if path.exists() {
        return std::fs::canonicalize(path).map_err(|error| error.to_string());
    }
    Ok(path.to_path_buf())
}

fn task_definition_seal(
    task_id: &str,
    stratum: &str,
    benchmark_version: &str,
    criteria: &[CriterionDef],
) -> String {
    let mut pairs: Vec<(String, String)> = vec![
        ("benchmark".into(), benchmark_version.trim().into()),
        ("stratum".into(), stratum.trim().into()),
        ("task".into(), task_id.trim().into()),
    ];
    let mut ordered = criteria.to_vec();
    ordered.sort_by(|left, right| left.id.cmp(&right.id));
    for item in ordered {
        let kind = if item.critical {
            "critical"
        } else {
            "functional"
        };
        pairs.push((
            format!("criterion:{}", item.id),
            format!("{kind}:{}", item.weight),
        ));
    }
    seal_pairs("ax-replay-task-v1", &pairs)
}

fn evidence_seal(
    benchmark_version: &str,
    task_id: &str,
    run_id: &str,
    arm: &str,
    definition_seal: &str,
    scorecard: &Scorecard,
) -> String {
    let critical_percent = scorecard
        .critical_percent
        .map(|value| format!("{value:.6}"))
        .unwrap_or_else(|| "none".into());
    let pairs = [
        ("arm", arm.trim().to_string()),
        ("benchmark", benchmark_version.trim().to_string()),
        (
            "critical_omission",
            scorecard.critical_omission_audit_required.to_string(),
        ),
        ("critical_passed", scorecard.critical_passed.to_string()),
        ("critical_percent", critical_percent),
        ("critical_total", scorecard.critical_total.to_string()),
        ("definition", definition_seal.to_string()),
        (
            "functional_passed",
            format!("{:.6}", scorecard.functional_passed_weight),
        ),
        (
            "functional_percent",
            format!("{:.6}", scorecard.functional_percent),
        ),
        (
            "functional_total",
            format!("{:.6}", scorecard.functional_total_weight),
        ),
        (
            "functional_unverified",
            scorecard.functional_unverified.to_string(),
        ),
        ("run", run_id.trim().to_string()),
        ("task", task_id.trim().to_string()),
        ("verdict", verdict_name(scorecard.verdict).to_string()),
    ];
    seal_pairs("ax-replay-evidence-v1", &pairs)
}

fn seal_pairs(domain: &str, pairs: &[(impl AsRef<str>, impl AsRef<str>)]) -> String {
    let mut ordered: Vec<(&str, &str)> = pairs
        .iter()
        .map(|(key, value)| (key.as_ref(), value.as_ref()))
        .collect();
    ordered.sort_by(|left, right| left.0.cmp(right.0).then(left.1.cmp(right.1)));
    let mut payload = Vec::from(domain.as_bytes());
    payload.push(0);
    for (key, value) in ordered {
        payload.extend(key.as_bytes());
        payload.push(0);
        payload.extend(value.as_bytes());
        payload.push(0);
    }
    hex::encode(Sha256::digest(&payload))
}

fn verdict_name(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Pass => "pass",
        Verdict::Fail => "fail",
        Verdict::Blocked => "blocked",
    }
}

fn require_text(value: &str, name: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{name} is required"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, weight: f64, critical: bool, passed: Option<bool>) -> CriterionResult {
        CriterionResult {
            id: id.into(),
            weight,
            critical,
            passed,
        }
    }

    fn sample_task() -> FrozenTask {
        freeze_task(
            "case-ts-01",
            "typescript",
            "v1-frozen",
            vec![
                CriterionDef {
                    id: "behavior".into(),
                    weight: 95.0,
                    critical: false,
                },
                CriterionDef {
                    id: "edge".into(),
                    weight: 5.0,
                    critical: false,
                },
                CriterionDef {
                    id: "no-leak".into(),
                    weight: 0.0,
                    critical: true,
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn val16_one_critical_failure_with_high_functional_score_is_fail() {
        let card = score_run(&[
            item("f1", 99.0, false, Some(true)),
            item("f2", 1.0, false, Some(true)),
            item("c1", 0.0, true, Some(false)),
        ])
        .unwrap();
        assert!((card.functional_percent - 100.0).abs() < f64::EPSILON);
        assert_eq!(card.critical_passed, 0);
        assert_eq!(card.critical_total, 1);
        assert_eq!(card.verdict, Verdict::Fail);
    }

    #[test]
    fn val17_skipped_critical_is_blocked_never_pass() {
        let card = score_run(&[
            item("f1", 100.0, false, Some(true)),
            item("c1", 0.0, true, None),
        ])
        .unwrap();
        assert_eq!(card.verdict, Verdict::Blocked);
        assert_eq!(card.critical_passed, 0);
        assert_eq!(card.critical_total, 1);
    }

    #[test]
    fn functional_score_uses_weights_and_unverified_counts() {
        let card = score_run(&[
            item("a", 2.0, false, Some(true)),
            item("b", 2.0, false, Some(false)),
            item("c", 1.0, false, None),
            item("sec", 0.0, true, Some(true)),
        ])
        .unwrap();
        assert!((card.functional_percent - 40.0).abs() < f64::EPSILON);
        assert_eq!(card.functional_unverified, 1);
        assert_eq!(card.verdict, Verdict::Fail);
    }

    #[test]
    fn empty_critical_list_is_not_quietly_one_hundred() {
        let card = score_run(&[item("a", 100.0, false, Some(true))]).unwrap();
        assert!(card.critical_omission_audit_required);
        assert_eq!(card.critical_percent, None);
        assert_eq!(card.verdict, Verdict::Blocked);
    }

    #[test]
    fn release_gate_pass_requires_ninety_five_and_all_critical() {
        let card = score_run(&[
            item("a", 95.0, false, Some(true)),
            item("b", 5.0, false, Some(true)),
            item("sec", 0.0, true, Some(true)),
        ])
        .unwrap();
        assert!((card.functional_percent - 100.0).abs() < f64::EPSILON);
        assert_eq!(card.critical_percent, Some(100.0));
        assert_eq!(card.verdict, Verdict::Pass);
    }

    #[test]
    fn a_frozen_task_seals_the_same_after_criterion_reorder() {
        let left = freeze_task(
            "case-ts-01",
            "typescript",
            "v1-frozen",
            vec![
                CriterionDef {
                    id: "a".into(),
                    weight: 1.0,
                    critical: false,
                },
                CriterionDef {
                    id: "b".into(),
                    weight: 0.0,
                    critical: true,
                },
            ],
        )
        .unwrap();
        let right = freeze_task(
            "case-ts-01",
            "typescript",
            "v1-frozen",
            vec![
                CriterionDef {
                    id: "b".into(),
                    weight: 0.0,
                    critical: true,
                },
                CriterionDef {
                    id: "a".into(),
                    weight: 1.0,
                    critical: false,
                },
            ],
        )
        .unwrap();
        assert_eq!(left.definition_seal, right.definition_seal);
    }

    #[test]
    fn a_changed_criterion_weight_changes_the_task_seal() {
        let left = freeze_task(
            "case-ts-01",
            "typescript",
            "v1-frozen",
            vec![CriterionDef {
                id: "a".into(),
                weight: 1.0,
                critical: false,
            }],
        )
        .unwrap();
        let right = freeze_task(
            "case-ts-01",
            "typescript",
            "v1-frozen",
            vec![CriterionDef {
                id: "a".into(),
                weight: 2.0,
                critical: false,
            }],
        )
        .unwrap();
        assert_ne!(left.definition_seal, right.definition_seal);
    }

    #[test]
    fn val14_a_broken_evaluator_assertion_turns_red() {
        let task = sample_task();
        let evidence = run_fixture(
            &task,
            "run-1",
            "fresh-ax",
            &[
                ("behavior".into(), Some(true)),
                ("edge".into(), Some(true)),
                ("no-leak".into(), Some(false)),
            ],
        )
        .unwrap();
        assert_eq!(evidence.scorecard.verdict, Verdict::Fail);
        assert!(verify_evidence(&evidence));
    }

    #[test]
    fn sealed_evidence_fails_verification_after_tamper() {
        let task = sample_task();
        let mut evidence = run_fixture(
            &task,
            "run-1",
            "fresh-ax",
            &[
                ("behavior".into(), Some(true)),
                ("edge".into(), Some(true)),
                ("no-leak".into(), Some(true)),
            ],
        )
        .unwrap();
        assert_eq!(evidence.scorecard.verdict, Verdict::Pass);
        assert!(verify_evidence(&evidence));
        evidence.scorecard.verdict = Verdict::Fail;
        assert!(!verify_evidence(&evidence));
    }

    #[test]
    fn a_missing_result_is_unverified_and_cannot_pass() {
        let task = sample_task();
        let evidence = run_fixture(
            &task,
            "run-2",
            "fresh-ax",
            &[("behavior".into(), Some(true)), ("edge".into(), Some(true))],
        )
        .unwrap();
        assert_eq!(evidence.scorecard.verdict, Verdict::Blocked);
        assert_eq!(evidence.scorecard.critical_total, 1);
        assert_eq!(evidence.scorecard.critical_passed, 0);
    }

    #[test]
    fn an_unknown_result_id_is_rejected() {
        let task = sample_task();
        let error = run_fixture(
            &task,
            "run-3",
            "fresh-ax",
            &[
                ("behavior".into(), Some(true)),
                ("edge".into(), Some(true)),
                ("no-leak".into(), Some(true)),
                ("extra".into(), Some(true)),
            ],
        )
        .unwrap_err();
        assert!(error.contains("unknown"), "{error}");
    }

    #[test]
    fn v2_arm_workspaces_are_separate_and_reject_golden_overlap() {
        let parent = tempfile::tempdir().unwrap();
        let golden = tempfile::tempdir().unwrap();
        let workspaces = create_arm_workspaces(
            parent.path(),
            &["fresh-ax", "fresh-control", "full-package"],
            Some(golden.path()),
        )
        .unwrap();
        assert_eq!(workspaces.len(), 3);
        assert_ne!(workspaces[0].root, workspaces[1].root);
        assert!(workspaces[0].root.is_dir());
        let leak = assert_no_golden_leak(golden.path(), golden.path()).unwrap_err();
        assert!(leak.contains("golden"), "{leak}");
        let nested =
            create_arm_workspaces(golden.path(), &["fresh-ax"], Some(golden.path())).unwrap_err();
        assert!(nested.contains("golden"), "{nested}");
    }

    #[test]
    fn v2_duplicate_arm_names_are_rejected() {
        let parent = tempfile::tempdir().unwrap();
        let error =
            create_arm_workspaces(parent.path(), &["fresh-ax", "fresh-ax"], None).unwrap_err();
        assert!(error.contains("duplicate"), "{error}");
    }

    #[test]
    fn format_scorecard_lines_do_not_claim_release_on_fail() {
        let card = score_run(&[
            CriterionResult {
                id: "c1".into(),
                weight: 1.0,
                critical: true,
                passed: Some(false),
            },
            CriterionResult {
                id: "f1".into(),
                weight: 1.0,
                critical: false,
                passed: Some(true),
            },
        ])
        .unwrap();
        let lines = format_scorecard_lines(&card).join("\n");
        assert!(lines.contains("verdict: fail"), "{lines}");
        assert!(lines.contains("release gate: not claimed"), "{lines}");
        assert!(!lines.contains("verdict: pass"), "{lines}");
    }

    #[test]
    fn prepare_v2_pilot_arms_makes_g_a_b_c() {
        let parent = tempfile::tempdir().unwrap();
        let arms = prepare_v2_pilot_arms(parent.path(), None).unwrap();
        assert_eq!(
            arms.iter().map(|arm| arm.arm.as_str()).collect::<Vec<_>>(),
            vec!["G", "A", "B", "C"]
        );
    }

    #[test]
    fn pilot_bundle_writes_frozen_task_without_claiming_release() {
        let parent = tempfile::tempdir().unwrap();
        let bundle = prepare_pilot_bundle(parent.path()).unwrap();
        assert_eq!(bundle.task.task_id, "pilot-ts-list-filter");
        assert_eq!(bundle.arms.len(), 4);
        assert!(bundle.task_path.is_file());
        let loaded: FrozenTask =
            serde_json::from_str(&std::fs::read_to_string(&bundle.task_path).unwrap()).unwrap();
        assert_eq!(loaded.definition_seal, bundle.task.definition_seal);
        let fail = run_fixture(
            &bundle.task,
            "run-1",
            "A",
            &[
                ("filter-matches-query".into(), Some(true)),
                ("empty-query-shows-all".into(), Some(true)),
                ("no-secret-in-ui".into(), Some(false)),
            ],
        )
        .unwrap();
        let lines = format_scorecard_lines(&fail.scorecard).join("\n");
        assert!(lines.contains("release gate: not claimed"), "{lines}");
        assert!(bundle.adapters.contains(&"fixture".into()));
    }

    #[test]
    fn fixture_pilot_scores_all_arms_without_claiming_release() {
        let parent = tempfile::tempdir().unwrap();
        let report = score_fixture_pilot(parent.path()).unwrap();
        assert_eq!(report.adapter, "fixture");
        assert_eq!(report.live_agent_runs, 0);
        assert_eq!(report.release_gate, "not claimed");
        assert_eq!(report.task_id, "pilot-ts-list-filter");
        assert_eq!(report.arms.len(), 4);
        let by_arm: std::collections::BTreeMap<_, _> = report
            .arms
            .iter()
            .map(|arm| (arm.arm.as_str(), arm))
            .collect();
        assert_eq!(by_arm["G"].verdict, "pass");
        assert_eq!(by_arm["A"].verdict, "fail");
        assert_eq!(by_arm["B"].verdict, "fail");
        assert_eq!(by_arm["C"].verdict, "blocked");
        for arm in &report.arms {
            assert!(arm.evidence_path.is_file(), "{}", arm.arm);
            let body = std::fs::read_to_string(&arm.evidence_path).unwrap();
            assert!(body.contains("\"releaseGate\": \"not claimed\""), "{body}");
            assert!(!body.contains("95%"), "{body}");
        }
        assert_eq!(report.release_gate, "not claimed");
    }

    #[test]
    fn pilot_task_v1_freezes_the_stratum_criteria_and_both_adapters() {
        let task = freeze_pilot_task_v1().unwrap();
        assert_eq!(task.task_id, "pilot-ts-list-filter");
        assert_eq!(task.stratum, "typescript-react");
        let ids = |critical: bool| {
            task.criteria
                .iter()
                .filter(|criterion| criterion.critical == critical)
                .map(|criterion| criterion.id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(false),
            vec!["filter-matches-query", "empty-query-shows-all"]
        );
        assert_eq!(ids(true), vec!["no-secret-in-ui"]);
        let parent = tempfile::tempdir().unwrap();
        let bundle = prepare_pilot_bundle(parent.path()).unwrap();
        assert_eq!(bundle.adapters, vec!["fixture", "cursor"]);
    }

    #[test]
    fn fixture_pilot_scores_a_prepared_bundle_in_place() {
        let parent = tempfile::tempdir().unwrap();
        let prepared = prepare_pilot_bundle(parent.path()).unwrap();
        let report = score_fixture_pilot(parent.path()).unwrap();
        assert_eq!(report.arms.len(), 4);
        for arm in &report.arms {
            let expected = parent
                .path()
                .join(&arm.arm)
                .join(format!("evidence-{}.json", arm.arm));
            assert_eq!(arm.evidence_path, expected);
            let body: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&expected).unwrap()).unwrap();
            assert_eq!(
                body["definitionSeal"],
                prepared.task.definition_seal.as_str()
            );
        }
    }

    #[test]
    fn fixture_pilot_refuses_a_tampered_frozen_task_on_reuse() {
        let parent = tempfile::tempdir().unwrap();
        let bundle = prepare_pilot_bundle(parent.path()).unwrap();
        let mut task: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&bundle.task_path).unwrap()).unwrap();
        let secret = task["criteria"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|criterion| criterion["id"] == "no-secret-in-ui")
            .unwrap();
        secret["critical"] = serde_json::json!(false);
        std::fs::write(
            &bundle.task_path,
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();
        let error = score_fixture_pilot(parent.path()).unwrap_err();
        assert_eq!(error, "definition seal mismatch");
        for arm in ["G", "A", "B", "C"] {
            let evidence = parent.path().join(arm).join(format!("evidence-{arm}.json"));
            assert!(!evidence.exists(), "{arm}");
        }
    }

    #[test]
    fn fixture_pilot_refuses_a_partial_bundle() {
        let parent = tempfile::tempdir().unwrap();
        let task = freeze_pilot_task_v1().unwrap();
        std::fs::write(
            parent.path().join("frozen-task.json"),
            serde_json::to_string_pretty(&task).unwrap(),
        )
        .unwrap();
        std::fs::create_dir(parent.path().join("G")).unwrap();
        let error = score_fixture_pilot(parent.path()).unwrap_err();
        assert_eq!(error, "arm workspace already exists: G");
        assert!(!parent.path().join("G").join("evidence-G.json").exists());
    }
}
