# Telemetry worker security remediation

Autonomous implementation, no separate spec approval.

Baseline: six npm audit vulnerabilities, predominantly `sharp`, `undici`, `wrangler/miniflare` and `ws`.

## Acceptance criteria
- Update `wrangler` within its existing major version to a registry release at least 168 hours old.
- Override vulnerable transitive package families to published patches: `sharp@0.35.5`, `undici@7.29.1`, `ws@8.21.0`, if compatible with the upgraded runtime.
- Rebuild `telemetry-worker/package-lock.json` using npm's cutoff-aware resolver, never hand-editing integrity fields.
- Pass the existing age checker, strict OSV query gate, npm audit at moderate or higher (zero moderate/high/critical findings), `npm ci`, Cloudflare types, TypeScript compilation and Wrangler dry-run. Skip any commit on failure.
- Make no production deployment and do not auto-merge. Protect the ability of AX clients to report anonymous usage to the telemetry worker.
- The temporary, branch-only workflow is removed after successful lockfile regeneration.

## Test evidence
See the dedicated GitHub Actions run and follow-on PR dependency checks. Unexecuted tests must not be claimed as passing.
