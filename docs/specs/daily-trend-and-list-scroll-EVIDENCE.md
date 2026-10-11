# EVIDENCE — Daily trend color, edge glow, list wheel

Spec: `/Users/gary/io/ax/docs/specs/daily-trend-and-list-scroll.md`
Glow spec: `/Users/gary/io/ax/docs/specs/no-edge-glow.md`
Spec approval: not obtained (autonomous run).
Tier: 2.
Source: landing tree, bundle `index-3arCM9HY.js` / `index-CTF8AcH6.css`, server `127.0.0.1:7070`.

## Mapping

| Behavior | Check |
|---|---|
| D1 Tokens saved uses accent | `savingsPortal.test.ts` S6. Browser: stop-color `var(--accent)`, computed `rgb(92, 177, 247)` on Azure Dark. |
| L1 record wheel reaches the page scroller | `listScroll.test.ts` L1. Rules: list `overflow: visible`, `.page-split-main` scrollTop 0 → 400 after scrolling a record, then back up 318px. Nodes: scroll on a row moved the list pane 287px. |
| L2 memory list still scrolls | `listScroll.test.ts` L2. Memory vault `overflow: auto`, scrollTop 0 → 300 on a record. |
| B1–B3, B5 no wave / no orbit | `edgeGlow.test.ts` 4 passed. Browser: 0 `.cc-waves`, no `#ax-green-vessel`, `--titlebar-wave-h: 0px`, no running CSS animations on Savings. |

## Gauntlet

`node --experimental-strip-types --test src/edgeGlow.test.ts src/listScroll.test.ts src/savingsPortal.test.ts`

13 pass, 0 fail (after the last edit). RED before the edit: 7 fail, 6 pass.

Skipped: mutation, coverage threshold, full web-ui suite, Playwright. The wheel check is the browser scroll above, not a Playwright spec.

## Limit

On Azure Dark the wave node and the green-vessel overlay were already absent before this rebuild. The wave component and its animation CSS were still in the app for any non-portal layout, and the orbit dots still animated on a selected graph row. Both are off now. A hard reload is required if a tab still has `index-B1LPTtH0.js`.
