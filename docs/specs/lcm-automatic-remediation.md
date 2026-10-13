# Automatic dependency remediation pull requests: executable specification

Status: Implemented autonomously from the owner's request on 2026-10-13; specification approval: not obtained (autonomous implementation).

## Intended behavior

1. **Periodic**: Scheduled GitHub Actions, and a first push to `main`, evaluate the three locked npm projects separately and safely propose lockfile-only fixes.
2. **Non-breaking**: `npm audit fix` runs without `--force`, with `--package-lock-only` and `--ignore-scripts`, and a registry cutoff at least seven days in the past. No manifest or application code changes are proposed by automation.
3. **Improvement required**: Comparing two valid npm audit JSON reports, accept a candidate only when at least one finding is removed and counts at each severity never increase. Missing/invalid reports fail closed.
4. **Age control**: Before creating a PR, verify every newly locked npm version in the entire repo was published at least 168 hours ago, via the existing release-age checker against `HEAD`. Failure blocks the PR.
5. **OSV security**: Reject any newly introduced package version with a known OSV advisory before creating a PR, even when npm audit severity totals improved. Registry or OSV lookup failures block creation.
6. **Consistency**: Validate npm lockfile and manifest consistency with the existing age checker, and `npm ci --dry-run --ignore-scripts`.
7. **No noise**: Skip a project with no lockfile change, unchanged risk count, or only breaking fixes. A stable bot branch per project avoids parallel duplicate PRs.
8. **Review required**: PRs are created only, never merged. The existing PR security checks remain applicable when the action uses a dedicated `AX_LCM_PR_TOKEN`; the default `GITHUB_TOKEN` suppresses triggered workflow runs.
9. **No unrelated automation**: No existing release, deployment, Docker, backend CI or unrelated workflow is activated.
10. **Broader ecosystem**: Cargo and GitHub Actions version PRs remain Dependabot's responsibility. Dependabot security PRs require the repository-side security switches; the new workflow focuses on npm audit-fixable problems.
11. **Failure modes**: Registry/network failures, invalid audit JSON, missing metadata, malformed workflow inputs, unexpected file changes and missing GitHub PR permissions must be visible failures, not silently accepted candidates.

## Tests

- `test_lcm_autofix.py`: improvements accepted, no changes rejected, severity increases rejected, invalid/missing reports rejected, changes outside the selected lockfile rejected.
- Workflow execution on a real GitHub PR checks unit tests and bash workflow shape.
- A launched scheduled/on-push run and created PR are **not** considered proven until observed in GitHub Actions.

## Files and setup

- New: `.github/workflows/dependency-autofix.yml`, `scripts/lcm_autofix.py`, `scripts/test_lcm_autofix.py`, this SPEC and `docs/audits/lcm-autofix-evidence.md`.
- Updated: `docs/DEPENDENCY_LIFECYCLE.md`, `README.md`, `site/src/content/docs/guides/dependency-lifecycle.md`, `site/astro.config.mjs`, `.github/PIPELINES.md`.
- No new library dependencies. GitHub-hosted runners provide Bash, Python 3.12, npm and GitHub CLI.
- **One-time settings**: GitHub Actions must be allowed to create PRs. A scoped `AX_LCM_PR_TOKEN` (Contents read/write, Pull requests read/write) is strongly recommended so bot PRs trigger the PR security checks. Never automatically merge bot PRs.

## Explicitly out of scope

- Security exceptions to the seven-day rule, major manifest upgrades, unmaintained-package replacement, auto-merge, deployment, and automatic Rust lockfile repairs not handled by Dependabot.
