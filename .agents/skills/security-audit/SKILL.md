---
name: security-audit
description: "Use when conducting security assessments — OWASP Top 10 / API / LLM, CWE Top 25, CVSS scoring — auditing PHP/TYPO3, APIs, frontend, Terraform/K8s/Docker IaC, AWS cloud, AI agent configs, scanning dependencies, or processing open dependency lifecycle (LCM) pull requests, open Dependabot alerts, and open GitHub issues."
alwaysApply: false
triggers: ["dependabot","lcm","svss"]
tags: ["open","security","number","pull","github","version","svss","lcm","dependabot"]
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
- [ ] Open LCM pull requests (Dependabot version bumps and `automation/lcm-*`) merged or closed
- [ ] Open Dependabot alerts on the security tab gone from the open list
- [ ] Open code scanning alerts fixed, or dismissed only when the finding cannot happen, with the reason recorded
- [ ] No open pull requests
- [ ] No open issues
- [ ] No open items on Security or Code quality (Dependabot, code scanning, secret scanning, code quality)
- [ ] Local commits pushed to the remote
- [ ] When those queues and the dependency monitor are green, and the LCM pull requests are merged or closed: bump, build, publish the six packages, then point the site at that tag

## The run does not stop (ax)

A run of this skill is not finished while any of these is still open: a pull request, an issue, a Dependabot alert, a code-scanning alert, a secret-scanning alert, or a code-quality alert.

Do not end the run, and do not tell the user the run stopped, while one of those lists is non-empty. A stale dependency graph, a queued analysis, a release-age wait, and a red check are more work. They are not an ending, and they are not a named blocker that completes the run.

- A pull request that cannot merge is closed. The comment is in the language from ax settings `reviews.commentLanguage`.
- An issue is closed as completed or not planned, as the issues section says.
- A Dependabot alert whose vulnerable version is still in the manifest on the default branch is fixed on that branch. An open alert outranks the 168-hour wait. Apply the patched version, or remove the dependency when no patched version exists.
- A Dependabot alert whose vulnerable version is already absent from that manifest, while `state` is still `open`, means the dependency graph is behind the branch. Dismiss it. `dismissed_reason` is `not_used`. The comment, in the review language, names the package, the version now in the manifest, and the commit. Then list the alerts again.

```bash
gh api -X PATCH "repos/GaryWenneker/ax/dependabot/alerts/<number>" \
  -f state=dismissed \
  -f dismissed_reason=not_used \
  -f dismissed_comment="<package> <old> staat niet meer in <manifest> op <commit>. De lock heeft <package> <new>."
```

- A code-scanning alert on lines that already contain the fix stays open until an analysis of that commit exists. List it again after that analysis. If the analysis still reports it and the harm cannot happen, dismiss it with the code-scanning reason. Do not end before that.

The last action before the release section is the list commands. Each open count is zero. A 404 from the code-quality or secret-scanning alerts API means that product has no alert list. A 403 is retried until the command returns a list. It is not reported as zero.

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
4. A pull request that still cannot be merged or closed is not an ending. Close it with the reason. A workflow pin the token cannot update is changed on the default branch in a commit, then the pull request is closed as already applied.

List the LCM pull requests again at the end. None are open. Do not dismiss an advisory, do not add a cargo-audit ignore, and do not force-merge a red required check to clear the queue. The workflows that open these pull requests still do not merge them. This skill is the review that does.

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
3. When no such pull request exists and `patched` is set, apply that version on the default branch. An open alert is fixed even when that release is still inside the 168-hour window. The PHP runtime does not wait either.
4. When `patched` is null, remove the dependency. Do not add a cargo-audit ignore.
5. When the vulnerable version is already absent from the manifest on the default branch and the alert is still `open`, dismiss it as `not_used`, as the section "The run does not stop" says. Do not dismiss an alert while the vulnerable version is still in the manifest.

List open alerts again at the end. The open list is empty.

## Code scanning (ax)

A security audit of this repository is unfinished while an alert is open on <https://github.com/GaryWenneker/ax/security/code-scanning>. Code scanning is CodeQL on the default branch. It is a different queue from Dependabot alerts and from the dependency monitor.

List open alerts before changing anything:

```bash
gh api --paginate "repos/GaryWenneker/ax/code-scanning/alerts?state=open&per_page=100" \
  --jq '.[] | {number, rule: .rule.id, severity: .rule.security_severity_level, cwe: [.rule.tags[] | select(startswith("external/cwe/"))], path: .most_recent_instance.location.path, line: .most_recent_instance.location.start_line, message: .most_recent_instance.message.text, url: .html_url}'
```

A 403, or a body that says code scanning is disabled, is a blocker. It is not an empty queue. Name that failure. Do not report zero alerts unless this command printed no rows.

Handle each open alert in number order. Record the number, rule, severity, CWE tags, path, line, and message.

1. Fix the code when the described harm can still happen. A missing workflow `permissions` block, an unbounded allocation taken from a request, a download that can reach a private or loopback address, and a replace that leaves later copies of the character in place are fixes. Land the fix on the default branch. Default setup closes the alert on its next analysis. Do not dismiss it to make the page empty while the code still does the thing.
2. Dismiss when the finding is no longer a vulnerability in this repository. The reason is one of `false positive`, `used in tests`, or `won't fix`, and the comment says which guard, test, or protocol makes the harm impossible. Dismiss these:
   - a test, e2e spec, or docs harness that is not the shipped program
   - a value that is already constrained to a safe token (`valid_id`, `valid_hash`, an allowlisted directory name) before it becomes a path
   - a command argument that is one argv element after URL validation, with no shell
   - a CLI print of an account name or session id the operator just supplied
   - a protocol that requires an empty password field and does not hard-code a secret, such as a SonarQube token sent as the username
   - a URL that configuration validation already requires to be `https`

```bash
gh api -X PATCH "repos/GaryWenneker/ax/code-scanning/alerts/<number>" \
  -f state=dismissed \
  -f dismissed_reason=false_positive \
  -f dismissed_comment="<why this finding cannot happen>"
```

`dismissed_reason` is `false positive`, `won't fix`, or `used in tests`. Write the comment in the language from ax settings `reviews.commentLanguage`.

3. When the fix is already on the default branch, wait for an analysis of that commit and list again. If that analysis still reports the alert and the harm cannot happen, dismiss it. Do not end the run while it is open.

List open alerts again at the end. The open list is empty. Do not delete a workflow, a test, or a docs harness just to remove its alert.

## Pull requests (ax)

A run of this skill is unfinished while any pull request is open on <https://github.com/GaryWenneker/ax/pulls>. LCM pull requests follow the lifecycle section. Every other open pull request is handled in number order the same way: merge when the diff is the intended change and the required checks are green; close, with a comment in the language from ax settings `reviews.commentLanguage`, when the pull request cannot land (red required check, peer-dependency break, conflict that is not the same change, or a release younger than 168 hours). Do not admin-merge a red check.

List open pull requests again at the end. The list is empty.

## Security and quality (ax)

A run of this skill is unfinished while any of these pages still has an open item:

- Dependabot: <https://github.com/GaryWenneker/ax/security/dependabot>
- Code scanning: <https://github.com/GaryWenneker/ax/security/code-scanning>
- Secret scanning: <https://github.com/GaryWenneker/ax/security/secret-scanning>
- Code quality: <https://github.com/GaryWenneker/ax/security/code-quality>

Secret scanning and code quality use the same rule as Dependabot and code scanning. Fix the finding when the harm can still happen. Dismiss only when it cannot, with `dismissed_reason` and a comment in the language from ax settings `reviews.commentLanguage`. A 403 is retried until a list comes back. A 404 means that product has no alert list.

```bash
gh api "repos/GaryWenneker/ax/secret-scanning/alerts?state=open" --jq 'length'
gh api "repos/GaryWenneker/ax/code-quality/alerts?state=open" --jq 'length'
```

List each page again at the end. Every open count is zero.

## GitHub issues (ax)

A run of this skill is unfinished while an issue is open on <https://github.com/GaryWenneker/ax/issues>. List them before changing anything:

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

3. Do not run `npm audit fix --force`. An issue that is still open after the fix is closed in the same run. Do not leave it for a later session.

An issue that is not one of those security items is still processed. Close it. Use `completed` when the behavior is already in the product. Use `not planned` when it is a roadmap item this run is not implementing. The close comment, in the language from ax settings `reviews.commentLanguage`, names which of those two it is. The open issue list at the end of the run is empty.

## Push, then release (ax)

After the fixes, push every local commit on the release branch to `origin`. A dirty tree that is not part of this audit stays uncommitted. Do not start the release while a commit that belongs to the audit is only local.

Start the release only when all of these are true:

- the dependency monitor is green on the commit that will be released
- LCM pull requests are merged or closed
- the pull request list, the issue list, Dependabot, code scanning, secret scanning, and code quality have no open items

Then, in this order:

1. Bump the patch version in every workspace `Cargo.toml`, `Cargo.lock`, and `crates/ax-web/web-ui/package.json`.
2. Push that bump. Let CI pass on that commit. The dependency monitor must still be green.
3. Publish the six platform packages with the release workflow. Do not point the site at the version before `ax-win32-x64.zip`, `ax-win32-arm64.zip`, `ax-linux-x64.tar.gz`, `ax-linux-arm64.tar.gz`, `ax-darwin-x64.tar.gz`, `ax-darwin-arm64.tar.gz`, and `SHA256SUMS` exist on the GitHub release.
4. Set `site/public/releases/latest.txt` to that tag and push it. Do not add a What's new section or a changelog. Do not run the Netlify deploy.

If any queue is still open, do not bump and do not publish. Go back to the open item and finish it. Do not end the run there.

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