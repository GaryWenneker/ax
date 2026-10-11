# Evidence — No edge glow animations

Spec: `/Users/gary/io/ax/docs/specs/no-edge-glow.md`
Spec approval: not obtained (autonomous run; user asked to continue without a further question).
Source: git `48241f0` plus uncommitted web-ui/spec files in the landing tree.
Binary: `target-dev/release/ax` sha256 `6283b042ae61a798d7ca1b05f4cfbc355cb1d8a9…` (2026-10-11 18:10), PATH shim `~/.local/bin/ax` → that Mach-O.
Served bundle: `http://127.0.0.1:7070/assets/index-CG0TKuXK.js`

## Behavior map

| ID | Test | Result |
|---|---|---|
| B1 | `edgeGlow.test.ts` — App.tsx has no `HeaderWaves` | pass |
| B2 | `edgeGlow.test.ts` — every `--titlebar-wave-h` in `index.css` is `0px` | pass |
| B3 | `edgeGlow.test.ts` — portal hides `.cc-waves` | pass |
| B4 | `e2e/titlebar-wave.spec.ts` — mobile/tablet/desktop, `--project=system-chrome` | 3 passed (1.9s) |
| B5 | `edgeGlow.test.ts` — portal disables node `::before`/`::after` animation | pass |
| Must not: Green Vessel overlay | `greenVesselIntro.test.ts` — `index.html` has no `#ax-green-vessel` | pass |
| Must not: Azure-only | `themes.test.ts` A1–A5 | pass |

## Gauntlet

Command (unit): `cd /Users/gary/io/ax/crates/ax-web/web-ui && node --experimental-strip-types --test src/edgeGlow.test.ts src/lib/themes.test.ts src/greenVesselIntro.test.ts`

- Full related suite: **30 passed, 0 failed** (80ms).
- Static types: `npx tsc --noEmit` in web-ui, exit 0.
- Lint/format: skipped — no project lint command for this CSS/JSX slice.
- Changed-line coverage: skipped — checks are file-source assertions, not a coverage runner with fail-under.
- Mutation: RED before GREEN — all 4 `edgeGlow` tests failed (`HeaderWaves` still imported, `--titlebar-wave-h: 12px`, missing portal hide/orbit rules). After GREEN they pass. No mutation tool in this tree.
- Property tests: skipped — not a parser/math change.
- Real execution: hard-reload `http://127.0.0.1:7070/stats` and `/nodes`. CDP: `theme=azure-dark`, `layout=portal`, `.cc-waves` count 0, `#ax-green-vessel` absent, `--titlebar-wave-h=0px`, script `index-CG0TKuXK.js`. Titlebar is flat Azure chrome with no mint ribbon.
- E2E: `npx playwright test e2e/titlebar-wave.spec.ts --project=system-chrome` → **3 passed**. Bundled Playwright Chromium is not installed here (`mobile-chrome` / `desktop-chrome` cannot launch); system Chrome is the executable layer.
- Supply chain: no new dependencies.
- Suite health: unit tests are deterministic file reads. Playwright order not shuffled.

## Stale compiled e2e

Playwright’s default glob also matches `e2e/*.spec.js`. A leftover compiled `titlebar-wave.spec.js` still asserted mint waves. That file (plus `.d.ts` / `.js.map`) was deleted so only the TypeScript spec remains.

## Known limits

- `HeaderWaves.tsx` still exists on disk; it is unmounted and tree-shaken from the JS bundle (`HeaderWaves` / `cc-waves` absent from `index-CG0TKuXK.js`).
- Graph live-update spark rings are unchanged (spec must-not).
- Independent verification: not performed.
