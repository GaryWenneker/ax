# Verification evidence

The implementation was developed in an isolated checkout based on main `3c8e127be575cd9f312594e0b5c1d876749b190c`. The verified implementation/frontend tree is `522c37ced1cd59f9b4f3d933b16d4a724265dd0e`; the final follow-up removes version-history prose and adds its policy/regression check. No package manifests or lockfiles changed. Dependencies were installed from the existing locks. No workflows, release publication or deployment were triggered.

The user authorized autonomous implementation and merge. The evolving requirements are recorded in SPEC.md; a separate final formal specification approval was not obtained. Ax MCP was initially unavailable. When it became available, the latest documentation directive was saved with `ax_policy_capture` and verified with `ax_rules`: `no-per-version-updates`, CRITICAL, alwaysApply, priority 100. The server first rejected the cached chat identifier as belonging to another connection; retrying without that identifier saved the rule successfully. The shared rule is included with this change.

## Checks and scenarios

| Scope | Command / check | Observed result |
| --- | --- | --- |
| Affected backend packages | `AX_SKIP_WEB_BUILD=1 cargo test -p ax-web -p ax-policy -p ax-mcp -p ax-global-db --lib` | 27 global-db + 171 MCP + 251 policy + 103 web tests passed: 552 total |
| Final review detail changes | `AX_SKIP_WEB_BUILD=1 cargo test -p ax-web review_tests --lib` | Both focused tests passed after the changed-field detail was added |
| Rust lint | `AX_SKIP_WEB_BUILD=1 cargo clippy -p ax-web -p ax-policy -p ax-mcp -p ax-global-db --all-targets -- -D warnings` | Exit 0 |
| Embedded frontend | `cd crates/ax-web/web-ui && npm run build` | TypeScript and Vite exited 0; only the existing large-bundle advisory remained |
| Contrast | `cd crates/ax-web/web-ui && node --experimental-strip-types --test src/lib/themes.test.ts` | 12 tests passed, including all six accents in both modes |
| Browser checks | Playwright: access-settings, azure-portal, control-spacing, policy-review-density, remote-host, replication-setup, remote-mcp-publish and remote-mcp specs; desktop-chrome/mobile-chrome | 66 passed against the rebuilt binary; the final screenshot/scrollability fixture also passed its two targeted runs |
| Website | `cd site && npm test` | 50 passed after the evergreen documentation change |
| Website motion | `cd site && AX_CHROME_EXECUTABLE=<local Chrome executable> node scripts/run-local-motion-check.mjs` | At 1440px and 390px, respectively 31% and 24% of rendered pixels visibly changed over 1.2 seconds. Pause, explicit Play under reduced motion, reload persistence and the reduced-motion default passed |
| Evergreen documentation negative control | New `evergreen-docs.test.ts` run before editing Introduction | Exit 1 with the existing version-history headings; after removal, the complete site suite passed |

Browser tests use deterministic network-boundary fixtures. Additional installed-binary execution used isolated Primary and two replica projects and real local HTTP endpoints: graph nodes/edges, cached source, memories, logs and durable history exchanged and survived a restart; owner CSRF/stale-workspace guards rejected invalid requests; generated host configuration/client registration, PKCE, host approval, token exchange, MCP initialize and refresh succeeded. Removing a live OAuth grant or active replica was rejected. Removed OAuth credentials could not refresh; a hidden revoked replica could not bootstrap again with the shared key. Review endpoints returned new/replacement classifications and changed fields, rejected a stale inspection, and supported approval/rejection. The real host screenshot was captured after the mocked browser suite to avoid publishing a fixture error state.

The shipped browser specs verify native select behavior and action gaps across all Settings sections, both themes, small viewports, typed decisions, cancellation, sequential bulk partial-failure handling, visible comparison content, accent persistence/contrast, unchanged header/footer surfaces, aligned savings baselines, and sidebar removal/restoration on Quality Gates. Screenshots were visually inspected.

## Review record and limits

The final documentation-only delta was reviewed after loading the old-coder/review-loop instructions; it removes historical prose without changing installation links or release identifiers. API, Rust, TypeScript and React review skills were also available. Prior implementation checks above were performed during the original autonomous/degraded workflow; this report does not claim a retrospectively complete old-coder gauntlet.

| Review | Scope | Findings remaining |
| --- | --- | --- |
| Documentation delta | Evergreen Introduction, shared rule, regression check; rule retrieval verified via MCP | 0 |

Changed-line coverage, mutation testing, randomized test ordering and independent fresh-context verification were not performed. No complete coverage or independent security-proof claim is made. Real execution was local; real-LAN discovery and deployment remain unverified. Historical imports did not record their originating peer, so the UI explicitly reports that absence. The policy-import queue is distinct from graph/memory/log/history replication.

To repeat the browser checks, run a locally built `ax web` with a disposable test project, then set `AX_WEB_URL` to that server and `AX_CHROME_EXECUTABLE` to an installed Chromium-compatible executable before running the listed Playwright specs. Optional screenshots are written when `AX_ACCESS_SCREENSHOTS=1`. The standalone motion runner starts and stops its own local Astro preview and never deploys it.
