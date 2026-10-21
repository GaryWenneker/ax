---
name: security-audit
description: "Use when conducting security assessments — OWASP Top 10 / API / LLM, CWE Top 25, CVSS scoring — auditing PHP/TYPO3, APIs, frontend, Terraform/K8s/Docker IaC, AWS cloud, AI agent configs, scanning dependencies, or processing open dependency lifecycle (LCM) pull requests, open Dependabot alerts, and open GitHub issues."
alwaysApply: false
priority: 50
enabled: true
status: approved
scope: project
group: security
allowed-tools: Bash(grep:*) Bash(jq:*) Bash(gh:*) Read Glob Grep
compatibility: Requires grep, jq, gh CLI.
license: (MIT AND CC-BY-SA-4.0). See LICENSE-MIT and LICENSE-CC-BY-SA-4.0
metadata:
  author: Netresearch DTT GmbH
  repository: https://github.com/netresearch/security-audit-skill
  version: 2.11.8
---

# Security Audit Skill

Security audit patterns (OWASP Top 10, LLM Top 10 2025, CWE Top 25 2025, CVSS v4.0), cloud/IaC, GitHub security. 80+ PHP/TYPO3 checkpoints (v14.3 LTS in `typo3-security.md`).

## Expertise Areas

- **Vulnerabilities**: XXE, SQLi, XSS, CSRF, command injection, path traversal, file upload, deserialization, SSRF, SSTI, JWT, type juggling
- **Standards**: OWASP Top 10 / API / LLM (2025), CWE Top 25, CVSS v3.1/v4.0, OWASP ASVS
- **Cloud & IaC**: AWS; Terraform, Kubernetes, Docker, Helm
- **API & Frontend**: REST/GraphQL authZ, rate limits, mass assignment, CSP, DOM-XSS
- **AI Agents**: SKILL.md/AGENTS.md/CLAUDE.md/mcp.json/hooks.json audit; prompt injection; excessive agency

## Reference Files (in `references/`, `.md` implied)

- **Core**: owasp-top10, cwe-top25, xxe-prevention, cvss-scoring, api-key-encryption
- **Prevention**: deserialization-prevention, path-traversal-prevention, file-upload-security, input-validation, error-message-sanitization, ssh-forced-command-hardening
- **Architecture**: authentication-patterns, security-headers, security-logging, cryptography-guide, security-invariants, indistinguishability-defences
- **Language features** (`*-security-features`): php, python, javascript-typescript, nodejs, go
- **Frameworks** (`*-security`): typo3, typo3-fluid, typo3-typoscript, symfony, react, vue
- **Cloud & IaC**: aws-security, iac-security
- **API & Frontend**: api-security, frontend-security
- **AI Agent**: llm-security (OWASP LLM Top 10 2025)
- **Threats**: modern-attacks, cve-patterns
- **DevSecOps**: ci-security-pipeline, supply-chain-security, automated-scanning, gha-security, git-history-secrets
- **Incident**: supply-chain-incident-response

## Security Checklist

- [ ] `semgrep`/`opengrep`, `trivy fs --severity HIGH,CRITICAL`, `gitleaks` clean
- [ ] bcrypt/Argon2 passwords, CSRF on state changes, TLS 1.2+
- [ ] Server-side input validation; parameterized SQL; XML entities off
- [ ] Output encoding + CSP; no unserialize() on user input
- [ ] API keys encrypted; exception messages sanitized
- [ ] Secrets out of VCS; audit logging on
- [ ] Uploads validated, renamed, outside web root
- [ ] Headers HSTS + X-Content-Type-Options; dependencies scanned
- [ ] Open LCM pull requests (Dependabot version bumps and `automation/lcm-*`) merged, closed, or named with a blocker
- [ ] Open Dependabot alerts on the security tab fixed, or named with a blocker
- [ ] Open GitHub issues read; in-scope security issues closed with the fix, or named with a blocker

## GitHub Actions Security

- **NEVER** interpolate `${{ inputs.* }}` / `${{ github.event.* }}` in `run:` — use `env:`
- Dependency triage: upgrade > override > dismiss. Full patterns: `references/gha-security.md`.

## Dependency Lifecycle Monitor (ax)

The green gate for this repo is [`.github/workflows/dependency-monitor.yml`](../../../.github/workflows/dependency-monitor.yml). It is red when RustSec reports a vulnerability, or when `npm audit --package-lock-only --audit-level=moderate` fails for `site`, `telemetry-worker`, or `crates/ax-web/web-ui`.

For every finding that fails that workflow:

1. Record the advisory id, package, version, and the CWE or RustSec category the advisory names.
2. Take the patched release when one exists. A missing patch means remove the dependency. Do not add a cargo-audit ignore, and do not dismiss the advisory, to turn the job green.
3. Unmaintained and unsound warnings stay visible. They do not fail the monitor. A warning that is also a vulnerability does.

When the monitor is green on `main`, bump the patch version, let CI pass, publish the six release assets, then point `site/public/releases/latest.txt` and `site/netlify/latest-version.json` at that tag. The site pointer waits until all six assets exist. The same monitor run must still be green after the bump.

## Lifecycle pull requests (ax)

A security audit of this repository is unfinished while an open LCM pull request can still be merged or closed. LCM pull requests are dependency version bumps and automatic remediation proposals. A pull request is in scope when any of these is true:

- the author is `app/dependabot` or `app/github-actions`
- the head branch starts with `dependabot/` or `automation/lcm-`
- the title is a dependency version bump (`chore(deps)`, `bump `, or `security update`)

List them before changing anything:

```bash
gh pr list --repo GaryWenneker/ax --state open --limit 100 \
  --json number,title,author,headRefName,statusCheckRollup,mergeable,url
```

Keep the rows that match the scope above. Handle each one in number order:

1. Read the diff and the check rollup. Record the package, the old version, the new version, and whether the change is a patch, minor, or major.
2. Merge when every one of these holds:
   - required checks are green, including `minimum-release-age-7-days` and `dependency-vulnerability-review` when those checks apply to the diff
   - the bump is a patch or minor, or a security fix whose new versions are at least 168 hours old
   - the diff is limited to lockfiles, manifests, or the GitHub Actions pin the bump names
   - tests that cover the affected package pass on the pull request head
   Merge with `gh pr merge <number> --repo GaryWenneker/ax --merge`. Do not use an admin bypass.
3. Close when the pull request cannot land, and write the close comment in the language from ax settings `reviews.commentLanguage`:
   - a major bump that breaks the build, or a grammar or API change without a migration
   - the release-age check failed because a new version is still inside the 168-hour window
   - the dependency is unused, so the bump is unnecessary
   - the branch conflicts with `main` and rebasing it would not be the same bump
   Close with `gh pr close <number> --repo GaryWenneker/ax --comment "<reason>"`.
4. Leave it open only when this run cannot finish a required check. The usual case is a GitHub Actions pin whose token is not allowed to update workflow files. Name that number and the blocker in the audit report.

List the LCM pull requests again at the end. Every one is merged, closed, or named with a blocker. Do not dismiss an advisory, do not add a cargo-audit ignore, and do not force-merge a red required check to clear the queue. The workflows that open these pull requests still do not merge them. This skill is the review that does.

## Dependabot alerts (ax)

A security audit of this repository is unfinished while an open alert remains on <https://github.com/GaryWenneker/ax/security/dependabot>. An alert is a vulnerability record. It is a different object from a version-bump pull request. An alert is in scope when `state` is `open`.

List them before changing anything:

```bash
gh api --paginate "repos/GaryWenneker/ax/dependabot/alerts?state=open&per_page=100" \
  --jq '.[] | {number, state, ghsa: .security_advisory.ghsa_id, cve: .security_advisory.cve_id, severity: .security_advisory.severity, package: .dependency.package.name, ecosystem: .dependency.package.ecosystem, manifest: .dependency.manifest_path, vulnerable: .security_vulnerability.vulnerable_version_range, patched: .security_vulnerability.first_patched_version.identifier, url: .html_url}'
```

A 403, or a body that says Dependabot alerts are disabled, is a blocker. It is not an empty queue. Name that failure. Do not report zero alerts unless this command printed no rows.

Handle each open alert in number order:

1. Record the alert number, GHSA id, CVE id when present, severity, package, ecosystem, manifest, vulnerable range, and first patched version.
2. When an open lifecycle pull request already bumps that package to the patched version, the lifecycle section disposes of that pull request. The alert is handled only after that pull request merges and a fresh list shows the alert is no longer `open`.
3. When no such pull request exists and `patched` is set, apply that version under the dependency-monitor rules. Take the patched release. A patched release still inside the 168-hour window stays a named blocker. The PHP runtime does not wait.
4. When `patched` is null, remove the dependency. Do not dismiss the alert, and do not add a cargo-audit ignore, to clear the page.
5. Leave the alert open only when this run cannot land the fix. Name the alert number and the blocker.

List open alerts again at the end. Every alert is gone from the open list, or named with a blocker.

## GitHub issues (ax)

A security audit of this repository is unfinished while an open issue on <https://github.com/GaryWenneker/ax/issues> has not been read. List them before changing anything:

```bash
gh issue list --repo GaryWenneker/ax --state open --limit 100 \
  --json number,title,labels,author,url
```

Read the body before classifying an issue as out of scope:

```bash
gh issue view <number> --repo GaryWenneker/ax --json number,title,labels,body
```

An issue is in scope when any of these is true:

- a label name is `security`, `vulnerability`, `dependencies`, or `dependabot`
- the title or body contains `CVE-`, `GHSA-`, `RUSTSEC-`, or an npm advisory id
- the title or body reports a vulnerability, a secret, a Dependabot finding, or a GitHub security control that is still off (dependency graph, Dependabot alerts, Dependabot security updates, or a required security check)

Handle each in-scope issue in number order:

1. Record the number, title, and the advisory or control it names.
2. Fix it on the default branch when the fix is a patched dependency, a removed dependency, or a repository setting the audit can verify. A checklist issue closes only when each item is true in the live repository. Write the comment in the language from ax settings `reviews.commentLanguage`, then close:

```bash
gh issue close <number> --repo GaryWenneker/ax --reason completed --comment "<reason>"
```

3. Leave it open only when this run cannot land the fix. Name the number and the blocker. Do not close it to clear the list. Do not run `npm audit fix --force`.

An issue that matches none of the scope rules stays open. The report names it out of scope with the title and labels that were read. This audit does not close enhancement or roadmap issues.

List open issues again at the end. Every in-scope issue is closed because the fix is on the default branch or the named setting is on, or it is named with a blocker. Every other open issue is named as out of scope. An empty open list is a completed pass: say the queue was empty.

## PHP runtime

Every PHP script that runs must use the latest stable version published at <https://www.php.net/releases/?json>. That version is the highest non-museum `x.y.z` whose tags are not alpha, beta, or RC. The seven-day / 168-hour release-age gate does not apply to this runtime. A release published today is required as soon as it is on php.net.

Check the interpreter that runs Composer and PHP, every `shivammathur/setup-php` `php-version`, and every `FROM php:` tag. A minor channel is current only when it is the major.minor of that latest stable version. An older patch, an older minor, and the floating tag `latest` fail. Do not close a pull request that moves PHP onto that latest stable version because the release is still inside the 168-hour window. Cargo, npm, and GitHub Actions still wait 168 hours.

In this repository:

```bash
python3 scripts/check_dependency_age.py --print-latest-php
python3 scripts/check_dependency_age.py --php-runtime "$(php -r 'echo PHP_VERSION;')" --php-scan .
```

## Verification

```bash
./scripts/security-audit-dispatcher.sh /path/to/project  # auto-detect stack
./scripts/security-audit.sh /path/to/project             # PHP-only
./scripts/github-security-audit.sh owner/repo            # GH repo
```

Dispatcher detects the stack from indicator files and runs matching `scripts/scanners/*.sh` (13 ecosystems; see `references/` index).

---

> Contributing: https://github.com/netresearch/security-audit-skill