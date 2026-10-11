# SPEC — No edge glow animations

Status: autonomous run (spec approval not obtained; user asked to continue without a further question).
Tier: 2 (normal — visual chrome change, existing e2e encodes the opposite behavior).
Isolation: none — continue in the landing tree so `ax web` embed matches the files under test.

## Goal

Command Center must not play decorative glow animations along chrome edges. Azure Portal Dark/Light stays the only look. Titlebar mint waves, the reserved wave ribbon, and selected-node orbit dots along icon edges are off.

## Behaviors

### B1 — Titlebar waves are not mounted
Given any Azure theme (Dark default or Light),
When the shell renders,
Then `App.tsx` does not import or render `HeaderWaves`,
And the DOM has no `.cc-waves` node.

### B2 — Wave ribbon height is zero at every viewport
Given `:root` and the max-width 899px / 480px overrides,
When CSS is applied,
Then every `--titlebar-wave-h` assignment is `0px`.

### B3 — Wave CSS does not paint
Given leftover `.cc-waves` markup (embed, tests, or a stale DOM),
When styles apply,
Then `.cc-waves` is `display: none`.

### B4 — Viewport checks (mobile / tablet / desktop)
Given `/settings` at 393×851, 768×1024, and 1280×800,
When the page is ready,
Then `.cc-waves` is absent or not visible,
And `--titlebar-wave-h` computes to `0px`.

### B5 — Selected-node orbit glow is off in portal
Given portal layout,
When a graph-kind list item is selected,
Then the orbit/breathe pseudo-elements do not animate (`animation: none`).

### Must not
- Reintroduce Mint / VS Code / macOS themes.
- Restore Green Vessel overlay (`#ax-green-vessel` stays out of `index.html`).
- Change Azure Dark default or Light as the other choice.
- Change live row chase, ship pulse, or graph live-update sparks (those are not chrome-edge glows).

## Setup
- No new dependencies.
- Tests: `/Users/gary/io/ax/crates/ax-web/web-ui/src/edgeGlow.test.ts`, rewrite `/Users/gary/io/ax/crates/ax-web/web-ui/e2e/titlebar-wave.spec.ts`, add one case in `e2e/azure-portal.spec.ts`.
- Git: no checkpoint commits unless the user asks.
- After GREEN: `AX_FORCE_WEB_BUILD=1 bash scripts/reinstall-cli.sh`, restart `ax web --port 7070`, browser-check titlebar and a selected Memory/Nodes row.
