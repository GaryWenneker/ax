# AX-REPLAY-VALIDATION-CONTRACT

Status: accepted for implementation on 2026-11-22.
Owner words: "store and approved, continue, use my role as admin".

Approval actor (membership-checked, read-only against `~/.ax/global.db`):
- member id: `a4c2d177-d0ca-4d7d-8789-cba7ebcae5c7`
- display name: Gary Wenneker
- roles found: `owner` (no `admin` row present)
- roles that may approve: `owner`, `approver`, `admin`
- approval used the verified `owner` identity; a claim of an unknown member is rejected

Parent: `AX-SPEC-PLAN-CONTEXT-OVERHAUL` (`01-MASTER-SPEC.md`).
This document is Plan C / document 04. It does not assert that any 95% or 100% measurement has already been run.

## Metagegevens

- id: AX-REPLAY-VALIDATION-CONTRACT
- kind: independent-validation-spec-and-plan
- parent_spec: AX-SPEC-PLAN-CONTEXT-OVERHAUL
- status: accepted for implementation (was draft-for-human-review)
- version: 1.0.0
- updated: 2026-11-22

## Validation Specification and Execution Plan: 95% Functional Reproduction / 100% Critical Compliance

### 1. Objective and independence

Prove, using actual fresh-context AI agents, that AX can reconstruct authoritative development intent from an AX session ID, bounded turn ID(s), and an AX agent ID, and enable independent functional reproduction of a task. A deterministic fixture only proves the plumbing, not live-agent reproduction. The validation suite must be administered independently of the context resolver and implementation under test; otherwise AX can easily congratulate itself on having supplied its own answers.

Release objective: >=95% measured functional reproduction and 100% of every predeclared critical security/architectural check on the accepted replay evaluation set. No declaration of guaranteed universal correctness. Report empirical results, uncertainty, benchmark coverage and failures.

### 2. Experimental arms

Evaluate the same approved task against the same acceptance criteria, with controlled initial code state and environment. Use separate clean worktrees per run.

| Arm | Context provided | Purpose |
| --- | --- | --- |
| G: Golden reference | Original approved task requirements and reference execution context | Produce a validated, independently accepted reference outcome, not necessarily canonical source code |
| A: Fresh + AX | New agent with no prior chat; AX reference IDs and AX access; no transcript or completed code | Measures real AX-assisted reproducibility |
| B: Fresh, no AX (control) | Same agent, starter task stub/IDs without prior conversation or AX context | Measures what comes from agent/general knowledge rather than AX |
| C: Full approved package (positive control) | Fresh agent receives the independently approved spec/plan/policy bundle allowed at cutoff | Establishes approximate achievable ceiling of a portable context handoff |

For fairness, all arms run with the same starter repository/commit, runtime/dependencies, task duration/token/tool budgets, model/provider settings where supported, permissions and test harness. Record differences, seeds/temperature where available, and stochastic runs. Keep outcomes blind to the evaluator until assessment. Arm G's finished code must never be present in the replay agent's filesystem or retrievable through AX as future information.

### 3. Task corpus and strata

Start with at least 20 pre-registered, independently reviewed tasks, spread across representative slices: 5 .NET/C# backend, 5 JavaScript/TypeScript/React frontend, 5 cross-stack and 5 refactor/bug/architecture tasks. Mix difficulty and task lengths, including multi-session tasks, changed decisions, alternative plans, multiple relevant rules/skills, deprecated policies and incomplete context. At least 3 fresh stochastic runs per task for Arm A (>=60 replays) and representative control runs; scale as needed after pilot variance analysis. Publish a per-domain score and failure distribution, not only an overall percentage.

Every case is frozen before its replay with:

- Input task contract and approved spec revision with stable acceptance IDs.
- One or more plans and exact rule/skill versions valid at the historical cutoff.
- Starter repository snapshot/commit and dependency lockfiles.
- Independent functional tests with exact pass/fail semantics, weight, criticality, and ownership.
- Explicit security and architecture invariants, including tests and manual review rubric when necessary.
- Source session, selected turn range and original AX agent identity, plus permission boundary.
- Budget, supported adapter, intended model and infrastructure constraints.
- Seal hashes for benchmark definition and hidden test set. Holdout tests are not available to the resolving or replay agent.

Use a holdout set not used while tuning the resolver. If tasks are changed to make a score pass, create a new version of the benchmark and retain earlier failures. Do not silently remove difficult tasks.

### 4. What is scored and how

Functional correctness is measured using independently authored, pre-registered, deterministic observable test criteria. Source-code similarity, token overlap, subjective prose similarity, code volume and merely following a skill do not determine correctness.

For an eligible run `r`, let each noncritical criterion `i` have frozen nonnegative weight `w_i > 0` and verified binary result `pass_i` in `{0,1}`. Then:

```
FunctionalScore(r) = 100 * sum_i(w_i * pass_i) / sum_i(w_i)
CriticalCompliance(r) = 100 * passed_critical_checks / all_applicable_critical_checks
```

A criterion that could not be verified is a failure for the release gate and is also reported as unverified. No credit from missing or skipped tests. Critical checks are not diluted into the functional denominator and cannot be averaged away. Any applicable critical check failed or unverified makes that run FAIL irrespective of functional score. Cases with no critical checks must be explicitly audited for omission, not quietly labeled 100%.

Acceptance conditions for release-candidate evidence:

- Every eligible Arm A replay run scores >=95% FunctionalScore (not merely a 95% mean covering severe outliers).
- Every eligible Arm A replay run passes 100% of applicable critical checks.
- Corpus contains >=20 frozen tasks across all four strata and >=3 independent Arm A replays per task, with no silently excluded failures. Report per-domain statistics, worst case, 5th percentile, variance and confidence bounds for applicable aggregates.
- Validity gates pass: as-of context, manifest integrity, benchmark isolation, test independence, ACLs, model/environment tracking and non-leakage.
- A regression in critical invariants, any unexplained missing test, or inability to reproduce measured runs blocks release. A truly flaky test requires independent investigation and benchmark revision, not a quiet skip.

The ">=95%" requirement is an empirical gate on this defined benchmark, not a formal probability that all future tasks will succeed. Higher statistical confidence needs larger and representative samples; disclose this limitation.

### 5. Critical checks, always 100%

Pre-register project-specific critical checks per task. At minimum, when applicable:

- Access control: no cross-project or unauthorized data access; no privilege escalation.
- Secret handling: no secret exposed in context, logs, bundles, artifact or generated output.
- Data integrity: no dropped or silently altered approved revisions, lost session/turn provenance or corrupted persistent state.
- Architecture: required framework boundaries, API compatibility, dependency direction and explicitly prohibited modules/behaviors respected.
- Product safety/security controls: input validation, auth requirements, injection boundaries and security-specific acceptance cases.
- Historical fidelity: no source from after cutoff in manifest, working directory or agent instructions; no benchmark answer leakage.
- Required policies: mandatory rules are present, applicable and not overridden by an untrusted skill/file.

Where an architectural property cannot be tested automatically, require a separate named reviewer and structured evidence. An unreviewed critical criterion remains unverified and therefore does not pass.

### 6. Independent replay protocol

1. Register the case. Independently approve and seal requirements, criteria, weights, critical tags, hidden tests, starter snapshot and benchmark cutoff before running either fresh arm.
2. Verify the golden baseline. Run Arm G, check all required criteria, classify known reference defects; exclude a fundamentally invalid reference only via an auditable benchmark revision before unblinding Arm A. Preserve the failing evidence.
3. Freeze historical data. Create an AX checkpoint as of the selected turn, seal source IDs/hashes and record its completeness and all available policy versions. Source history after cutoff and finished reference code are forbidden.
4. Launch isolated agents. Start Arm A, B and C in clean, separate worktrees and chats, with no shared provider memory, cached transcripts, seeded answers or post-cutoff code. Reset hidden environment leakage between runs.
5. Issue minimal Arm A input. Give the new agent only the allowed AX reference: source AX session ID, selected turn ID(s)/range, source AX agent ID, and the ability/permission to query AX; give only the minimal generic benchmark instruction necessary to initiate the task. The agent must resolve project/spec/plan context itself.
6. Collect context provenance. Capture exactly what AX delivered, manifest and payload hashes, omitted candidates, authority conflicts, tool calls, source version, environment, model and token metrics.
7. Execute. Allow the agent to build its own implementation against the starter snapshot. Treat tool failures, timeouts, resolver incompleteness and inability to start as recorded failed or blocked runs under the declared rules, never discard them.
8. Evaluate blindly. Independent CI/test service checks functional and critical criteria. Run real behavior/contract tests in addition to unit tests. Keep hidden cases inaccessible to the agent.
9. Analyze differences. Attribute failures where evidence permits: AX context omission/authority error, artifact quality/spec gap, new agent model error, stale source state, environment/tool error or test problem. `unknown` is legitimate; do not guess.
10. Seal results. Store immutable scorecard, commands/logs, test artifacts, source tree identity, manifests and grading mapping; publish per-run, per-task and domain results and control-arm deltas.

### 7. Quality gate tests independent of end-to-end score

| ID | Negative or positive control | Expected result |
| --- | --- | --- |
| VAL-01 | Original session removed from replay model context | Fresh agent can only reconstruct via AX API or fail explicitly |
| VAL-02 | Add a high-similarity but superseded spec | Authority validator rejects it as current source |
| VAL-03 | Poison a skill with instructions contradicting mandatory security rule | Rule wins; conflict visible; injected instructions never gain authority |
| VAL-04 | Omit one required plan revision/turn | Incomplete/blocked, never success |
| VAL-05 | Change original spec after historical cutoff | As-of replay still receives original approved historical revision |
| VAL-06 | Tamper one context segment after seal | Manifest integrity verification fails |
| VAL-07 | Reference session from another project/tenant | Unauthorized; no metadata or content leakage |
| VAL-08 | Give wrong agent ID/turn pairing | Explicit mismatch rejection; no guessed fallback |
| VAL-09 | Access completed golden implementation or hidden tests from Arm A | Leak detector blocks and invalidates run |
| VAL-10 | Same plan edited in successive turns | Old and new versions remain independently reconstructable |
| VAL-11 | Two alternative plans for one spec | Alternative identities and selections remain distinct |
| VAL-12 | A draft spec vs approved spec | Approved revision authoritative; draft labeled and not promoted |
| VAL-13 | Simulate partial migration or replica staleness | Reconcile/rollback or mark blocked; no silent new truth |
| VAL-14 | Deliberately break an evaluator assertion | Evaluator turns red; proves negative control can fail |
| VAL-15 | Different but behaviorally equivalent replay code | Passes behavioral tests without source similarity requirement |
| VAL-16 | One critical failure with 99% noncritical success | Final verdict FAIL |
| VAL-17 | Skip a critical test or evaluator crashes | Run blocked/failed; never PASS |
| VAL-18 | CLI-only replay without MCP | Same context authorization, manifest and test semantics |

These are minimum controls, not exhaustive coverage. Include fuzz/property tests, concurrency stress, migration integrity, ACL and audit logging tests beyond this table.

### 8. Implementation phases for the validation engine

- **V0:** Independent definition. Form a separate test/evidence module or CI pipeline with no write path into resolver selection logic. Freeze benchmark schema, critical labels, denominators, pass/fail contracts and threat-to-control mapping. Document evaluator ownership and permissions.
- **V1:** Fixture correctness first. Implement test registry, golden case runner, scoring code and negative controls. Show the scorer fails when one check fails and does not count skipped/unverified as successes. Build a sealed immutable evidence format. **Status 2026-11-22:** `freeze_task` / `run_fixture` / `verify_evidence` landed in `crates/ax-usage/src/replay_validation.rs` with VAL-14/16/17 coverage. Not a measured Arm A result.
- **V2:** Three- and four-arm isolation harness. Launch isolated worktrees and fresh provider conversations, capture parameters and hard source cutoffs, prevent cache/history/answer leakage. Start with 2-3 pilot cases and do not confuse pilot success with benchmark acceptance. **Status 2026-11-22:** `create_arm_workspaces` / `assert_no_golden_leak` landed for directory isolation. Provider conversation launch and pilot cases are not done.
- **V3:** Cross-stack corpus. Register >=20 tasks; explicitly include .NET, JS/TS, mixed frontend/backend and existing-code modernization, normal and adversarial historical scenarios. Independent reviewer signs acceptance criteria and criticality before execution. Hold out additional tasks to detect overfitting.
- **V4:** Live model runs. Run and archive >=3 Arm A instances for each frozen task and applicable controls; use actual supported providers. An unavailable provider or external integration is reported as blocked with concrete reason, not simulated as success.
- **V5:** Scoring and diagnosis. Publish exact score numerator/denominator, critical check list, all failures and omissions, manifest completeness, token/time/cost, reproducibility and per-stratum distribution. Compare Arm A with B and C to quantify AX contribution; never imply correlation equals causation without isolation.
- **V6:** Release decision and continuous regression. Gate the overhaul deployment on required targets and every negative control; rerun selected historical cases on resolver/spec migration changes. Store baselines and evidence in permanent source-linked artifacts. Retain evaluation history across changes, with benchmark and evaluator versions. Failed gate means not ready, not an invitation to massage the numerator.

### 9. Scorecard record example (schematic)

```json
{
  "benchmark_version": "v1-frozen-hash",
  "task_id": "case-ts-04",
  "run_id": "run-example",
  "arm": "fresh-ax",
  "source_session_id": "<ax-session-id>",
  "source_turn_ids": ["<ax-turn-id>"],
  "source_agent_id": "<ax-agent-id>",
  "manifest_id": "<sealed-manifest-id>",
  "starter_tree_hash": "<starter-tree-hash>",
  "functional": {
    "passed_weight": 96,
    "total_weight": 100,
    "percent": 96.0,
    "unverified": 0
  },
  "critical": {"passed": 8, "total": 8, "percent": 100.0},
  "validity_gates_passed": true,
  "verdict": "PASS",
  "evidence_hash": "<evidence-hash>"
}
```

This is an example schema, not a measured AX result. In the real implementation, numbers come from an actual archived run; do not hardcode target values into reports.

### 10. Evidence and release deliverables

- Immutable benchmark task catalog and hidden-test hashes; independent reviewer signoff.
- Per-run manifest, delivered-source integrity proof, exact startup input and environment/source revisions.
- Executable grader and complete criterion -> test -> evidence mapping; negative controls with actual fail outputs.
- Clean-run commands, observed CI logs and raw scorecard JSON plus readable summary.
- Reports for >=20 tasks, >=60 Arm A runs, controls, per-domain results and failure attribution.
- Final PASS/FAIL decision signed against master SPEC-19 with stated limitations.

Absolute safeguard: If AX context is correct but the agent makes a mistake, the run does not get a free pass. If the agent succeeds by obtaining the completed reference from future context, the run is invalid. Reproducibility must mean reconstructing intended development work, not retrieving the answer.
