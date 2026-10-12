# Logging and documentation verification

Date: 2026-10-12. Source base: `ea51c84567e61f9f2a0287894b2c373c927ac5cb`.
Compiled UI SHA-256 (index and linked assets): `884cb82cd53c8a449a67bec6ad698c656f40f75e0e790e788cb775795a8367d5`.

## Result

Logging now has one page heading and compact project/live controls. The redundant project banner and quality summary strip are removed; Quality and the project/log path remain reachable. Existing filters, sorting, column sizing, live events and inspector behavior are covered by the browser suite.

The feature matrix is in `AUDIT.md`. The source inventory covers 19 UI destinations, 14 Settings sections, 60 public top-level CLI names, 38 MCP tools and 174 mounted HTTP paths. Forty documentation files were checked. The final link/asset audit reports 234 links, 67 screenshot references to 43 unique images, and zero issues. The capture manifest contains 61 refreshed images, with source/image hashes and provenance. This is structural coverage plus observed/fixture-backed UI evidence, not execution of every backend feature.

## Reproduce the checks

Install the existing lockfiles with `npm ci` in `crates/ax-web/web-ui` and `site`. Set `AX_CHROME_EXECUTABLE` to an installed Chrome or headless-shell executable for Playwright. From the repository root:

```sh
node docs/specs/logging-docs-audit/verify.mjs
```

This runs the local UI build, 37 relevant unit tests, 102 desktop/mobile browser checks, 55 site tests, the documentation audit and the heading negative control. It does not build or deploy Netlify or the backend. For individual stages, use `--checks=build,unit,site,audit,heading` or `--checks=browser`.

| Check | Final evidence |
|---|---|
| `npm run build` in web-ui | Passed; 1,072 modules; tracked dist updated |
| `node --experimental-strip-types --test src/components/agent/activityModel.test.ts src/lib/calmRows.test.ts src/lib/routes.test.ts src/lib/themes.test.ts` | 37 passed |
| Combined browser suite via `AX_UI_DEMO=1 AX_UI_SPECS=... node docs/specs/list-columns/ui-check.mjs` | One complete run: 102 passed. Final capture refresh: 101 passed, one temporary preview `ERR_INVALID_RESPONSE`; affected remote-MCP file rerun: all 14 passed. Final control-spacing fixture repair: all 4 tests passed |
| `npm test` in site | 55 passed |
| `node site/scripts/audit-documentation.mjs` after capture recording | Passed with zero issues and the counts above |
| `node site/scripts/audit-documentation.mjs --negative-control` | Expected exit 1: an undocumented route is rejected |
| `node docs/specs/logging-docs-audit/check-heading-regression.mjs` | Passed: changing compiled Logging H1 to Trace makes the browser contract fail |
| Skill creator `quick_validate.py` | Passed for `ax-audit-documentation` |
| `git diff --cached --check -- . ':!crates/ax-web/web-ui/dist/**'` and UTF-8/BOM scan of changed text | Passed for handwritten source/docs; generated JS retains whitespace inside third-party template strings |

The browser stage selects column-accessibility, list-columns, logging-layout, focus-row-density, admin-brand-home, control-spacing, remote-host, remote-mcp-publish, replication-setup, review-compatibility and policy-review-density on both desktop/mobile projects. To refresh its 21 screenshot-hook images, additionally set `AX_SCREENSHOT_MANIFEST=1 AX_COLUMN_SCREENSHOTS=1 AX_ACCESS_SCREENSHOTS=1 AX_REMOTE_SCREENSHOTS=1 AX_REVIEW_SCREENSHOTS=1`. The recorder only includes known screenshot hooks changed during that run. After the final run's transport failure, completed hook images were recorded using that run's start time; the successful remote-MCP rerun recorded its four fresh images automatically. The final metadata pass also records all four Light/Dark control-spacing crops, including their fixture viewport separately from their cropped image dimensions.

## Screenshot reproduction and review

Set `AX_CHROME_PATH` for Puppeteer if the default browser is unavailable. Run `AX_SCREENSHOT_DEMO=1 AX_THEME=azure-dark node site/scripts/capture-screenshots.mjs` for current desktop Command Center pages and all Settings sections. Use `AX_VIEWPORT_WIDTH=393` and selected `AX_SHOTS` for mobile Logging, Memory, Stats and remote Settings. Run `node site/scripts/capture-homepage.mjs` for the public homepage at 1440 and 393 px. The homepage uses local Astro dev; no deployment is involved.

Capture scripts fail on uncaught application errors and missing/error fixture requests. Screenshots were visually reviewed in contact sheets, with detailed checks for Logging desktop/mobile, remote host, configured sharing/replication, review comparisons, accents and the homepage. Review found a host export schema error in the fixture; the fixture was repaired, a browser assertion was added and the new capture was inspected successfully. Synthetic metrics, project paths and credentials are examples, not production measurements.

## Review loops and repaired findings

1. Source/live/documentation comparison: repaired stale route labels, Settings organization, logging enablement/rotation guidance, missing MCP/HTTP entries and broken links; refreshed screenshots.
2. Independent application of the new skill: found delegated router omissions, missing screenshot error checks, incomplete provenance, stale tracing toggles, timezone distinctions, broken policy fragments, inaccurate captions and duplicated Light/Dark imagery. All were repaired and affected checks rerun.
3. Independent finished-diff check: corrected fixture-derived themes, separated cropped image size from declared viewport, fixed a same-page anchor and the host export fixture. A final crop review caught a VS Code selection displaying a Cursor path; the fixture now derives the destination and JSON schema from the selected client. All four control-spacing tests passed and its four images were recaptured. Final independent review reported zero unresolved findings. The temporary image-hash mismatches disappeared after capture manifest recording; the final audit passed.

The heading negative control is explicit regression evidence; an initial before-change browser run was blocked by browser startup infrastructure and is not claimed as a completed RED run. An earlier expanded suite had two Savings failures due to missing synthetic API data; the harness fixture was repaired and that suite passed without weakening assertions. Local Chrome singleton errors were resolved by using the installed headless shell. Existing bundle-size and npm environment warnings remain unrelated to the change.

## Delivery and limits

The skill is persisted in the personal skill repository and mirrored under `.agents/skills/ax-audit-documentation/` for repository agents. Its workflow compares code, observed UI and documentation, tracks evidence and screenshots, and repeats a fresh review until there are no unresolved findings. SPEC approval was not obtained during this autonomous run under the user's authorized request.

The live remote admin was inspected read-only. No production authentication, tokens, policy, replication, vault mounts or data were changed. CLI operations, real Primary/Replica synchronization and arbitrary API request bodies were not exhaustively executed. Ax's supplied session ID was rejected as belonging to another connection; session-scoped calls remained unavailable and this was reported as DEGRADED. Working graph/guard tools were used without that session.

No Netlify build/deployment, backend/Docker build, release, suspended workflow dispatch or server restart was performed. Merging source does not update the running admin or public documentation; publication remains the owner's separate deployment process.
