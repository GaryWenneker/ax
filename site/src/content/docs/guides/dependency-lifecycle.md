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
   vulnerability counts at any severity, or fails the 168-hour release-age gate.
4. Verifies lockfile consistency with `npm ci --dry-run`.
5. Opens or updates one project-specific PR only when a valid improvement
   exists. Human review is always required.

Dependabot independently proposes Rust, GitHub Actions and ordinary npm
version upgrades. Major or risky changes are not automatically attempted by
the fix workflow.

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
