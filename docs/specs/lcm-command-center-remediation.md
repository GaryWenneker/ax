# Command Center Vite dependency remediation

Status: autonomous implementation, no independent SPEC approval.

Baseline: npm audit reports two vulnerabilities (1 high, 1 moderate) in the Command Center lockfile, caused by Vite 5 and affected Esbuild <=0.24.2. Nonbreaking automatic npm audit fix cannot cross the Vite major boundary.

## Acceptance criteria
- Upgrade Vite to at least 7.3.6 (released months before 2026-10-13) and its React SWC plugin to a compatible published major (4.3.3+).
- Regenerate lockfile via npm using a timestamp cutoff of seven days and never accept too-recent releases.
- Verify every added registry package is older than 168 hours and has no known OSV advisories.
- Require `npm audit --package-lock-only --audit-level=moderate` to return zero significant vulnerabilities.
- Verify `npm ci`, `npm run build` (TypeScript and Vite) and the repository's UI unit test command. Avoid asserting that historically broken browser e2e workflows passed.
- Do not deploy, publish, enable unrelated CI or auto-merge. The temporary workflow must remove itself before the PR.
