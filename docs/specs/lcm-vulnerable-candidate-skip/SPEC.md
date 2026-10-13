# Handle vulnerable auto-remediation candidates without weakening LCM gates

## Incident
GitHub Actions run 38392219706, Dependency Auto-Remediation, failed for
`telemetry-worker`: `sharp@0.35.4` was proposed by `npm audit fix`
but OSV identified `GHSA-wq5f-xc86-pv6w`. No auto-PR must contain it.

## Executable requirements

1. **Known advisory in candidate mode:** `check_dependency_advisories.py --candidate`
   exits 2, emits a visible GitHub warning naming the vulnerable version and advisory,
   and does not permit downstream `npm ci` or automatic PR creation.
2. **Known advisory in strict PR mode:** without `--candidate`, the same
   vulnerability exits 1; `dependency-policy.yml` continues rejecting the PR.
3. **OSV failure:** transport error, malformed response, missing advisory ID,
   incomplete batch, or pagination exits 1 in both modes. Never treat unknown risk
   as a safe skip.
4. **Safe proposal:** exit 0 in both modes, still subject to the strict seven-day
   publication-age and `npm ci --dry-run` checks, and may receive a review-only PR.
5. **Operational reporting:** the remediation job finishes successfully with a
   warning for a rejected *candidate*, while independent daily vulnerability
   monitoring and PR security checks remain in place.
6. **No source/deployment changes:** no application, package manifest, lockfile,
   version, deployment, or release changes in this fix.

## Implementation plan

- Edit `scripts/check_dependency_advisories.py` to separate verified advisory
  findings from structural/query failures and provide the candidate-only exit code.
- Edit `.github/workflows/dependency-autofix.yml` to handle exit code 2,
  explicitly gate `npm ci` and PR creation on confirmed OSV safety, and
  run the preexisting advisory regression tests alongside remediation tests.
- Document candidate-skip semantics in `docs/DEPENDENCY_LIFECYCLE.md`.
- Record validation in `docs/specs/lcm-vulnerable-candidate-skip/EVIDENCE.md`.

No new dependencies or external service permissions.
Spec approval not obtained separately (autonomous response to reported failure).
