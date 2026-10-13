# Dependency Lifecycle Management (LCM)

AX monitors third-party dependencies as part of its supply-chain security policy. The
README's **LCM** badge describes the policy; the **Dependency Lifecycle Monitor**
badge reflects the result of the latest monitoring run on `main`. Neither badge
claims that all packages are vulnerability-free.

## Coverage

| Ecosystem | Package sources | Automated update proposals | Security monitoring |
| --- | --- | --- | --- |
| Rust | `Cargo.toml`, `Cargo.lock` | Dependabot daily, 7-day cooldown | Daily RustSec audit |
| npm | `site/package-lock.json` | Dependabot daily, 7-day cooldown | Daily npm audit |
| npm | `telemetry-worker/package-lock.json` | Dependabot daily, 7-day cooldown | Daily npm audit |
| npm | `crates/ax-web/web-ui/package-lock.json` | Dependabot daily, 7-day cooldown | Daily npm audit |
| GitHub Actions | `.github/workflows/*` | Dependabot daily, 7-day cooldown | Dependabot security alerts |

Dependabot checks for ordinary version updates on weekdays. GitHub security
advisories can arrive independently of this schedule.

## Automatic remediation pull requests

AX has two complementary automated proposal paths:

1. **Dependabot** proposes Cargo, npm and GitHub Actions version updates daily,
   with a seven-day cooldown. With GitHub Dependency Graph, alerts and security
   updates enabled, it also opens advisory-driven security PRs.
2. **Dependency Auto-Remediation** runs on a daily schedule and after changes to
   npm lockfiles on `main`. It attempts a **non-breaking, lockfile-only** npm
   audit fix for each of `site/`, `telemetry-worker/`, and
   `crates/ax-web/web-ui/`. It only creates or updates a PR when the affected
   project's vulnerability count strictly decreases at one or more severity
   levels, no severity count increases, the candidate passes a full 168-hour
   registry publication-age check, and `npm ci --dry-run` succeeds.
   It creates stable `automation/lcm-*` branches, rather than duplicate
   PRs every day.

The workflow never forces major upgrades, never changes package manifests or
source files, never bypasses the seven-day gate, and **never merges or
deploys** the proposed update. Unsafe or unfixable changes remain findings for
manual investigation. A passing GitHub workflow is a technical gate, not a
claim that the change has been application-tested.

[View automatically proposed LCM PRs](https://github.com/GaryWenneker/ax/pulls?q=is%3Apr+is%3Aopen+head%3Aautomation%2Flcm-) or
[run the safe-fix workflow](https://github.com/GaryWenneker/ax/actions/workflows/dependency-autofix.yml).

### One-time rights for automatic PR creation

- Under **Settings → Actions → General → Workflow permissions**, enable
  **Allow GitHub Actions to create and approve pull requests**. This is
  necessary when using the built-in `GITHUB_TOKEN` and cannot be set through
  workflow YAML alone.
- Strongly recommended: create repository secret `AX_LCM_PR_TOKEN` containing
  a dedicated fine-grained PAT or GitHub App token with **Contents: read/write**
  and **Pull requests: read/write** for this repository. The automation falls
  back to `GITHUB_TOKEN` if the secret is absent.
- **Important:** PRs opened with `GITHUB_TOKEN` do *not* automatically trigger
  `pull_request` workflows. A dedicated token ensures the existing release-age
  and OSV checks execute on bot PRs. Keep both as required checks in the main
  ruleset. A bot PR must never bypass human review.
- Actions in this new workflow use scoped write permissions solely to update
  lockfile branches and open PRs. No release or deployment rights are needed.

## Release-age gate

- Regular Dependabot **version-update** PRs are delayed until versions are at
  least **7 days (168 hours)** old.
- Every pull request runs `minimum-release-age-7-days`: it compares the checked-in
  `Cargo.lock` and the three npm lockfiles against the PR target branch.
  Each **newly introduced or changed registry package version**, including
  transitive dependencies, must have a verifiable publication date at least
  168 hours before the check. Existing unchanged versions are grandfathered.
- Official crates.io and npm registry release metadata are the sources of truth.
  Unverifiable or non-registry additions fail closed; local Rust workspace/path
  crates and npm workspace links are not external published packages.
- Known-security-fix Dependabot PRs may be opened immediately, but **the
  7-day merge check still applies**, by design. A critical vulnerability
  therefore requires deliberate incident response; this policy does not
  silently exempt urgent patches.
- The gate runs in GitHub. It does not prevent local `npm install` or
  `cargo update` commands; to **enforce it against merges**, set the
  `minimum-release-age-7-days` check as a required status check in branch
  protection/rulesets for `main` and disallow direct pushes/bypass.

The script can also run locally against a git base:

```bash
python3.12 -m unittest discover -s scripts -p 'test_dependency_age.py'
python3.12 scripts/check_dependency_age.py --base origin/main
```

## Vulnerability triage and maintenance

- [Automatic safe-fix PR workflow](https://github.com/GaryWenneker/ax/actions/workflows/dependency-autofix.yml):
  only lockfile-only, non-breaking npm advisory reductions are proposed.
- [Daily lifecycle scan](https://github.com/GaryWenneker/ax/actions/workflows/dependency-monitor.yml):
  RustSec against `Cargo.lock` and `npm audit` for the three npm lockfiles.
  A failed scan is an actionable warning, not proof that a release is compromised.
- [PR dependency review](https://github.com/GaryWenneker/ax/actions/workflows/dependency-policy.yml):
  block newly introduced versions with **any known OSV advisory**, without requiring GitHub Dependency Graph.
- [Dependabot alerts](https://github.com/GaryWenneker/ax/security/dependabot):
  severity, advisory, affected version and fix availability; prioritize critical
  and high vulnerabilities, malicious releases, and unmaintained packages.
- [Dependency update PRs](https://github.com/GaryWenneker/ax/pulls?q=is%3Apr+author%3Aapp%2Fdependabot):
  review release notes, maintenance activity, deprecations, and breaking changes.
- A package reaching end of life, becoming unmaintained or deprecated can lack a
  CVE. Dependabot does not guarantee EOL detection; maintainers still need to
  review release history, replacement projects and upstream maintenance notices.

## One-time GitHub repository settings

The initial PR run confirmed GitHub Dependency Graph was disabled. The independent OSV checker operates without it, but native Dependabot alerts require the GitHub security settings below.

Under **Settings → Security and quality → Advanced Security**, make sure
**Dependency graph**, **Dependabot alerts** and **Dependabot security updates**
are enabled. These settings are separate from `.github/dependabot.yml`.

Under **Settings → Rules → Rulesets** (or branch protection), require both
`minimum-release-age-7-days` and `dependency-vulnerability-review` before
merging PRs to `main`. Enable notifications for security alerts and failed
workflow runs.

Only `dependency-policy.yml` (pull requests),
`dependency-monitor.yml` (daily schedule and dependency pushes to `main`),
and `dependency-autofix.yml` (daily schedule and npm lockfile pushes to `main`) are automatically triggered.
Existing CI, release and deployment workflows retain their prior owners'
suspension and are not reactivated.

## Limitations

The release-age gate uses public registries, so PR runs require registry access
and can fail when a registry is unavailable. External git/tarball dependencies
are deliberately not accepted without further policy work. Dependency scanners
find **known** advisories, not every supply-chain compromise. A passing status
does not imply an independent security audit or guarantee safety.
