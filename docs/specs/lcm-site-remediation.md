# Site dependency vulnerability remediation: executable specification

Status: autonomous implementation; no separate spec sign-off.

## Baseline and goals
Current `site/package-lock.json`: npm audit reports 31 advisories (1 critical, 28 high, 1 moderate, 1 low). A critical Astro issue is present. Pre-existing Dependabot upgrades (#67, #70, #110) are blocked by an insecure `postcss-selector-parser@6.1.4` in their resolved lockfiles and/or younger-than-seven-day transitive packages.

## Acceptance criteria
1. Move Astro and compatible Starlight to release branches with known fixes and publication dates at least 168 hours old (Astro 7.3.5, Starlight 0.42.5 as candidate minimums).
2. Use a scoped npm override for the unpatched transitive selector parser, `postcss-selector-parser@7.1.6` (released 2026-09-03 and patching GHSA-rj75-hqrm-r3gf).
3. Generate `site/package-lock.json` with npm and its seven-day `--before` cutoff; do not handcraft package integrity fields.
4. Enforce publication-age and OSV checks across the entire workspace's newly locked versions. No exceptions.
5. The new website vulnerability counts must not increase at any severity, and must decrease in at least one category. Critical advisories should be eliminated; otherwise note a blocker.
6. Run `npm ci`, website test suite, and actual Astro build. Only commit if all pass.
7. No Netlify deployment, no production or release workflow changes, and no automatic merge.
8. Report remaining advisories honestly. No audit suppression or fake-green badges.

## Temporary workflow
A branch-specific GitHub Actions workflow installs dependencies, applies the registry timestamp cutoff, verifies security checks, runs website tests/build and self-removes before publishing the lockfile to the security PR. No new runtime dependencies beyond those required by the patched website packages.
