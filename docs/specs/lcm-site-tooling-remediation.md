# Website build-tool supply-chain remediation

Status: autonomous user-authorized remediation, no separate SPEC approval.

The Astro/Starlight remediation removed the website's critical vulnerability but left 14 high and 1 low npm audit findings. The high findings primarily come from outdated direct development dependencies `@netlify/functions@4` and `puppeteer@24`, plus their transitive dependencies.

## Acceptance criteria
- Upgrade only direct build/test tool dependencies to previously tested publication-age-eligible `@netlify/functions@6.0.2` and `puppeteer@25.12.0`; retain Astro/Starlight and patched selector-parser override.
- Resolve the lockfile from the official registry using npm's strict 7-day publication cutoff and optionally apply safe, non-forceful audit fixes.
- All newly introduced packages pass the repository's 168-hour age and OSV checks.
- `npm audit --package-lock-only --audit-level=moderate` is clean: any outstanding moderate, high or critical advisory blocks the change.
- npm ci, website tests and Astro production build pass; no Netlify deploy occurs.
- The self-removing feature-only workflow regenerates the lockfile. Only checked files enter the PR; no auto-merge.
