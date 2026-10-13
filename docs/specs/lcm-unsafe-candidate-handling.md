# LCM unsafe-candidate handling: executable acceptance specification

Status: Implemented autonomously from the user's request to check the failed LCM workflow on 2026-10-13. Spec approval: not obtained (autonomous run).

## Incident

The most recent `Dependency Auto-Remediation` run 38392219706 completed with a red telemetry matrix job because the proposed `sharp@0.35.4` was affected by OSV advisory GHSA-wq5f-xc86-pv6w. This is a **proper security rejection**, not an infrastructure failure. The independent daily vulnerability monitor must continue to alert about actual security debt.

## Acceptance criteria

1. **Unsafe candidate**: Given a known OSV advisory for an introduced package, the remediation runner outputs a visible GitHub warning, skips `npm ci` and skips branch/PR updates; the matrix job succeeds (expected, safe rejection).
2. **Incomplete/failed OSV lookup**: Malformed results, pagination, registry outage, rate limiting or missing advisory identifiers remain hard errors. The job fails, and no PR is pushed.
3. **Safe candidate**: Clean OSV results permit the existing dry-run installation and review-only PR creation.
4. **PR strictness**: Without `--candidate`, `check_dependency_advisories.py` keeps its existing semantics: **any** advisory or incomplete OSV response causes a failing security check.
5. **Seven-day enforcement**: No changes to registry release-age behavior; prior age check remains required before OSV review, and cannot be skipped on an eligible candidate.
6. **No broad actions**: Never reactivate general CI, release or deployment automation, never auto-merge, never mutate dependencies while evaluating OSV results.
7. **Tests**: Unit tests cover clean, advisory, and malformed/OSV-failure candidates under both CLI modes. Existing test suite and GitHub PR status checks must pass.

## Implementation

Files: `scripts/check_dependency_advisories.py`,
`scripts/test_dependency_advisories.py`,
`.github/workflows/dependency-autofix.yml`,
`docs/DEPENDENCY_LIFECYCLE.md`,
`docs/specs/lcm-unsafe-candidate-handling.md` (this specification),
`docs/audits/lcm-unsafe-candidate-evidence.md`.
No new packages, credentials, or build/deploy permissions.

## Limitations

Passing an auto-remediation job does not mean all installed dependencies are vulnerability-free. Actual security debt remains visible in the independent scheduled dependency monitor. The workflow can only propose safe lockfile-only npm updates; unfixed advisories remain subject to manual review.
