# Readable Logging rows — verification

Date: 2026-10-13. Source base: `01602b519855144d01ae1d9adc3f898c890db060`.
Compiled UI SHA-256 (index and linked assets): `2974c664f60556c934634c00cf2f89aaaeec8e80531db14c0af243f53fe1a5f3`.
Isolation: clean main-based `fix/logging-event-rows` branch in the isolated checkout; existing locked dependencies reused. Spec approval: not obtained (autonomous run). Tier 2, presentation fix; no HTTP/backend/CLI contract change.

## Result and acceptance mapping

| Requirement | Evidence |
|---|---|
| Readable Workspace sync, Memory capture-git and Ship actions with counts/timing | 14 presentation unit cases; real-domain browser fixture on both projects |
| Keep unknown/failed messages, literal human text, zero values and original entries | Unit cases, input immutability assertions, complete original raw-line browser assertion |
| Separate 12 px markers from text; remove unrelated dotted connector | Browser geometry at 393/900/1366/2048 px; minimum 12 px clearance, no row/page overflow |
| Compact mobile Time/Context | Real-domain browser check: centers within 2 px, short row height at most 72 px |
| Inspector step spacing and selected tool label | Mixed MCP/domain browser fixture checks gutter, selected workspace Tool/header and readable step title |
| Preserve sorting, resizing, filters, keyboard, live arrivals, inspector access and other lists | Logging, list-columns, column-accessibility, focus-row-density, admin-brand-home and control-spacing suites |
| Raw field remains searchable after humanizing rows | Event filter `duration_ms=162` still finds the Workspace row; its Raw line matches the complete original input |
| Current screenshots and documentation | Six refreshed Logging/Quality/inspector images; verified source/image hashes; README and two website guides updated; documentation audit zero issues |

Log parsing, call-group membership, payload inspection and navigation are unchanged. The selected inspector entry's tool now takes precedence over its group's fallback tool, preventing a Workspace event being titled as an earlier MCP tool. The 40 px gutter matches the existing calm list geometry. No production dependency or additional stylesheet system was added.

## Reproduce

From the repository root, with the locked web-ui/site dependencies installed:

```sh
AX_CHROME_EXECUTABLE=/path/to/chrome-headless-shell node docs/specs/logging-event-rows/verify.mjs
```

Optional individual stages: `--checks=build,unit,coverage,mutation,browser,site,audit`. Actual runtime used Node `v24.19.0`, Vite `5.4.21` and Chrome headless-shell `138.0.7204.168`. Workspace maintenance had removed the previous browser cache, so that pinned browser was restored in temporary storage via the existing Puppeteer browser installer; no package/lockfile change was made.

## Fresh final gauntlet

The verification entry point completed with exit 0 after the final implementation edit.

| Layer | Result |
|---|---|
| Production frontend build (`tsc && vite build`) | Passed; 1,073 modules; current `index-CqVnAn7X.css` and `index-CvQ4cfoO.js` |
| Relevant unit suite | 51 passed: 37 existing activity/calm-row/route/theme checks plus 14 presentation checks |
| Fail-closed coverage gate for `loggingPresentation.ts` | 100% lines, branches and functions; each threshold enforced at 100% |
| `mutation-check.mjs` | Baseline passed; all 3 real mutants executed and killed by assertion failures; working source unchanged |
| Relevant browser suite | 60 passed on desktop/mobile projects; real compiled UI, synthetic API/SSE boundaries |
| Site suite | 55 passed |
| Documentation audit after captures | 40 documents, 234 links, 68 image references to 44 unique images; zero issues |
| Handwritten diff/encoding checks | Source-only `git diff --cached --check -- . ':!crates/ax-web/web-ui/dist'` and changed UTF-8/BOM scan passed |

Coverage applies to the new pure presentation module, not every line of the existing React page or CSS. There is no configured dedicated ESLint/stylelint stage in this package; types, production bundling, diff checks, stack review and browser geometry provide separate evidence. No dependency changes required a new supply-chain audit. Browser fixtures do not prove real backend operations. The existing bundle-size/npm environment warnings and an unrelated `/api/links` fixture notice remain; captured Logging pages had no failed fixture requests or uncaught UI errors.

## RED and review rounds

Before implementation, the new unit contract failed 9 of 10 cases. Both new main-row browser checks failed on both projects: marker/text clearance was **-2 px**, and the domain headline was still the raw `[ax]` line. The inspector regression later failed on both projects with **-14 px** clearance. Mobile compaction failed with **17.695 px** separation between Time and Context; the same frozen assertion now passes. Initial coverage was 92.31% branches and correctly exited 1; realistic zero-value, watch-on, unknown-mode and fallback cases closed the gap without lowering thresholds.

| Round | Skills/status | Findings | Repair |
|---|---|---|---|
| 1 | old-coder, review-loop, TypeScript/React/JavaScript review usable | 3 minor: unsafe registry cast, positional parameter bundle, repeated field regex compilation | Typed partial registry, options object, single field scan; assertions unchanged |
| 2 | Same; screenshot/row inspection from ax-audit-documentation | Inspector gutter/tool caption and raw-line crop; mobile metadata used separate rows | Shared gutter, selected-tool precedence, readable inspector steps, raw-line capture scroll, compact mobile metadata; browser RED then GREEN |
| 3 | Same | Repeated presentation work per inspector step; capture caption overclaimed visible structured fields | One presentation result per step; caption describes visible steps/raw line |
| 4 | Same; finished diff and final PNG review | 0 unresolved findings | Final gauntlet and documentation audit green |
| 5 | Finalization: old-coder/review-loop and TypeScript/React/JavaScript skills usable; six PNGs inspected again | Evidence incorrectly described the unrestricted whitespace check as passing | Corrected the evidence: generated third-party bundle strings contain trailing whitespace; the source-only check passes. Generated strings were left byte-for-byte as built. |
| 6 | Same; final implementation/documentation diff and screenshot provenance | 0 unresolved findings | Fresh complete verification entry point exited 0: build, 51 unit checks, 100% presentation coverage, 3/3 mutants, 60 browser checks, 55 site tests and zero audit issues. |

This was a second-pass builder review using the audit skill, not a new independent-agent or Tier 3 verifier run. The human did not pre-approve this SPEC; that limitation is explicit.

## Screenshot reproduction

```sh
AX_SCREENSHOT_DEMO=1 AX_THEME=azure-dark AX_SHOTS=cc-logging.png,cc-logging-inspector.png,cc-mcp-quality.png AX_CHROME_PATH=/path/to/chrome-headless-shell node site/scripts/capture-screenshots.mjs
AX_SCREENSHOT_DEMO=1 AX_THEME=azure-dark AX_VIEWPORT_WIDTH=393 AX_SHOTS=cc-logging.png AX_CHROME_PATH=/path/to/chrome-headless-shell node site/scripts/capture-screenshots.mjs
AX_COLUMN_SCREENSHOTS=1 AX_SCREENSHOT_MANIFEST=1 AX_CHROME_EXECUTABLE=/path/to/chrome-headless-shell node docs/specs/logging-event-rows/verify.mjs --checks=browser
```

The six images are Logging desktop/mobile, Logging columns desktop/mobile, Quality, and the open inspector/raw line. All six share the compiled hash above; each PNG hash was checked against the manifest. Captured data is synthetic and shaped like the user's Workspace/Memory/Ship examples. The inspector manifest records the selected title and actual raw-fold open state. Existing unaffected images retain their previous provenance. Each refreshed PNG was visually reviewed; the inspector shows the original raw entry, and mobile puts timestamps and short kind chips beside each other.

## Publication limits

Ax's supplied session ID was rejected with `Chat belongs to another connection`; Mode: DEGRADED was reported. Working graph/guard/skill calls without that ID were used. No production data/settings, backend/Docker build, Netlify build/deployment, workflow dispatch, release, binary reinstall or server restart was performed. Source and compiled web assets are delivered through the PR. The owner's normal rebuild/publication process must update the embedded UI in the running Ax binary before the new layout appears on the live admin.

Finalization resumed the existing staged `fix/logging-event-rows` checkout, preserving its implementation and screenshots. The full verification command above was rerun successfully after recovery. A local Git push dry-run failed because this environment has no HTTPS credentials; publication uses the authorized GitHub connector, with exact local/remote tree identity checked before merge.
