# Evidence: LCM unsafe remediation candidate handling

Date: 2026-10-13
Specification: [lcm-unsafe-candidate-handling.md](../specs/lcm-unsafe-candidate-handling.md)
Specification approval: not obtained (autonomous incident fix).

## Grounded incident

- [Failed auto-remediation run #38392219706](https://github.com/GaryWenneker/ax/actions/runs/38392219706): website and Command Center matrix jobs succeeded.
- Telemetry candidate proposed `sharp@0.35.4`; OSV rejected it because of `GHSA-wq5f-xc86-pv6w`.
- The rejection was correct, but the remediation workflow had reported it as a failed job on every run.

## Acceptance-to-test mapping

| Requirement | Executable control |
| --- | --- |
| Vulnerable remediation candidate is skipped | `test_candidate_mode_skips_known_vulnerable_release` (candidate returns 2), Actions condition excludes install/PR steps |
| Standard PR security review stays strict | `test_strict_pr_mode_still_fails_known_advisory` (exits 1) |
| Incomplete/paginated OSV result fails closed | `test_candidate_mode_fails_closed_on_incomplete_result` (exits 1) |
| OSV outage remains a hard failure | `test_candidate_mode_fails_closed_on_osv_outage` (exits 1) |
| A safe candidate is not rejected | `test_clean_candidate_is_eligible` (exits 0) |
| Missing advisory identity is not mistaken for a normal CVE | `test_malformed_advisory_is_error_not_skip` |
| Original release-age gate preserved | No code changes in `check_dependency_age.py`; its workflow step continues before OSV candidate processing |
| No automatic PR for blocked candidate | Guarded workflow steps require `steps.osv.outputs.safe == 'true'` |
| No deployment or automatic merging | Workflow edits create branch/PR only |

## Observed CI

- [Dependency Policy run 38392870910](https://github.com/GaryWenneker/ax/actions/runs/38392870910):
  both `minimum-release-age-7-days` and `dependency-vulnerability-review`
  succeeded on the implementation commit before this evidence-only update.
- No new third-party dependency or external permission introduced.

## Limitations and planned post-merge verification

- This PR's own security checks cannot validate the scheduled-run matrix semantics end-to-end; a new main-branch run must prove the telemetry candidate skips cleanly with a warning.
- No Rust workspace build, Netlify deploy, full-application integration suite, measured coverage or mutation tool was executed because this change is restricted to GitHub policy script and workflow semantics.
- A passing auto-remediation status never means the existing telemetry package vulnerabilities are resolved; the independent monitor continues to report them.
