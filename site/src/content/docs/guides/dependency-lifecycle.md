---
title: Dependency lifecycle security
description: How Ax uses automated vulnerability audits, a seven-day package release-age gate, and review-only remediation pull requests.
---

Ax maintains third-party supply-chain hygiene with GitHub Actions and
Dependabot. Its [live lifecycle scan](https://github.com/GaryWenneker/ax/actions/workflows/dependency-monitor.yml)
reports known advisories for Cargo and npm; the repository [README](https://github.com/GaryWenneker/ax)
displays both the LCM policy badge and the status of the monitor.

## Review-only automated updates

[Dependency Auto-Remediation](https://github.com/GaryWenneker/ax/actions/workflows/dependency-autofix.yml)
runs daily and after npm lockfile changes on `main`.

For the website, Command Center and telemetry worker, each run:

1. Executes `npm audit` before and after a non-breaking `npm audit fix`
   against the checked-in lockfile.
2. Considers only registry versions published **at least seven days ago**.
3. Rejects any candidate that changes a manifest or source, increases
   vulnerability counts at any severity, fails the 168-hour release-age gate, or introduces a known OSV advisory.
4. Verifies lockfile consistency with `npm ci --dry-run`.
5. Opens or updates one project-specific PR only when a valid improvement
   exists. Human review is always required.

Dependabot independently proposes Rust, GitHub Actions and ordinary npm
version upgrades. Major or risky changes are not automatically attempted by
the fix workflow.

The PHP runtime is the exception to the seven-day gate. Scripts that run PHP
must use the latest stable version from [php.net](https://www.php.net/),
including a release published the same day. An older PHP patch or minor fails
even after seven days. Cargo, npm, and GitHub Actions keep the 168-hour wait.

A security audit then processes every open lifecycle pull request. It merges
a version bump whose required checks are green and whose change is a
non-breaking patch, minor, or an already-aged security fix. It closes a bump
that cannot land, including a breaking major, a version still inside the
seven-day window, an unused dependency, or a conflict that is no longer the
same bump. A pull request stays open only when the audit cannot finish a
required check, and the report names that blocker. The workflows that open
these pull requests still do not merge them.

### GitHub setup

To permit automatic PR creation, visit **Repository Settings → Actions →
General → Workflow permissions** and enable **Allow GitHub Actions to create
and approve pull requests**. For the PR security workflows to trigger, create
an `AX_LCM_PR_TOKEN` secret with a scoped token granting Contents and Pull
requests write access. Without that secret, the default `GITHUB_TOKEN`
cannot trigger follow-on PR workflows.

Enable **Dependency graph**, **Dependabot alerts**, and **Dependabot security
updates** under repository Advanced Security. Set the
`minimum-release-age-7-days` and `dependency-vulnerability-review` status
checks as required on `main`. No release, deployment or automatic merge
workflow is enabled by this feature.

See the [complete source policy](https://github.com/GaryWenneker/ax/blob/main/docs/DEPENDENCY_LIFECYCLE.md)
for incident triage, exceptions and known limitations.
