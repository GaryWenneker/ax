# Evidence: Automatic LCM remediation PRs

Date: 2026-10-13
SPEC: [lcm-automatic-remediation.md](../specs/lcm-automatic-remediation.md)
Specification approval: **not obtained (autonomous implementation)**.
Confidence is limited to the exercised controls and CI evidence below.

## Acceptance criteria and checks

| Requirement | Evidence / test |
| --- | --- |
| Regressions in any severity are rejected | `test_mixed_severity_increase_is_not_safe`; `test_higher_risk_regression_cannot_pass` |
| Strict reduction required | `test_safe_decrease_is_eligible`, `test_no_actual_improvement_is_skipped` |
| Invalid/missing npm audit reports fail closed | `test_missing_metadata_fails_closed`, `test_registry_error_fails_closed`, `test_invalid_counts_fail_closed` |
| Manifest or source changes not accepted | `test_unexpected_manifest_change_rejected` |
| No modification, no PR | `test_no_change_does_not_propose_pr` |
| git failure not ignored | `test_diff_failure_is_not_ignored` |
| Newly introduced releases at least 168 hours old | Existing `check_dependency_age.py --base HEAD` gate before bot PR step |
| Lockfile installability | `npm ci --dry-run --ignore-scripts --no-audit` before bot PR step |
| Auto-merge/deployment forbidden | Workflow creates branches/PRs only, no merge or deploy commands |
| GitHub token event suppression | Explicitly documented; `AX_LCM_PR_TOKEN` opt-in needed for follow-on PR security runs |
| Cargo/GitHub Actions | Existing Dependabot, not an npm autofix candidate |

## Observed checks

- PR #102, [Dependency Policy run](https://github.com/GaryWenneker/ax/actions/workflows/dependency-policy.yml): both `minimum-release-age-7-days` and `dependency-vulnerability-review` succeeded on commit `d4ef58425833b7cac34980e3e182178ef7ab3712`.
- The minimum release age job also ran `test_lcm_autofix.py`; the OSV job executed the existing advisory tests.
- No new package manager dependency introduced.

## Limitations and pending real-world proof

- `Dependency Auto-Remediation` runs only from the default branch after merge; its first scheduled/push run must be inspected separately.
- The linked account cannot change repository Actions PR-creation settings or configure `AX_LCM_PR_TOKEN`. Without that setting and/or a token with PR permissions, automatic creation may fail.
- No Rust/Cargo auto-repair beyond Dependabot, no major upgrades, no automatic merges.
- Full Rust workspace builds, Netlify builds, application e2e tests, static coverage calculation and mutation gauntlets were not run for this GitHub-workflow-only change.
- `npm audit` may continue to report vulnerabilities not fixable without a breaking update; that is expected and does not justify bypassing the age gate.
