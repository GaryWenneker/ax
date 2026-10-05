# Evidence — Green Vessel intro

Source: `dd0db6e` plus the uncommitted intro files. Spec: `/Users/gary/io/ax/docs/specs/green-vessel-intro.md`.

Spec approval: not obtained as a separate review of this file. The user wrote the acceptance criteria and asked for the CSS, HTML, and script. Confidence is lower because of that.

Tier 2. Isolation: working tree (the overlay has to land in the Command Center `ax web` embeds). `crates/ax-extraction/src/contracts.rs` was already dirty and was not edited.

## Behavior map

| Behavior | Check |
|---|---|
| Full-screen overlay, fixed, z-index 9999, pointer-events none | `index.html mounts a full-screen overlay` and `stylesheet keeps the green vessel contract` |
| 2.3s glow, `#00ed82`, inset neon shadow, opacity 0, opacity transition | same stylesheet test |
| `animationend` and the 2600ms fallback remove the node; second remove is safe | `animationend and the fallback timer each remove the overlay` |
| Chime starts with the glow; a rejected `play()` still adds `is-playing` | `a rejected play still starts the glow and retries once on click` |
| One later click or key retries audio once | same |
| No `localStorage` / `sessionStorage` | `start adds the playing class and does not touch storage` (storage getters throw) and `intro source does not mention web storage` |
| Second start does not play again | same start test |
| `prefers-reduced-motion` removes the overlay and skips `is-playing` | `reduced motion removes the overlay without the playing class` |
| Chime schedules three oscillators | `scheduleChime starts three oscillators on the destination` |
| Non-finite CSS duration is rejected | `checker fails when the duration is not a finite number of seconds` |
| Client routes do not replay | `main.tsx` calls `startGreenVesselIntro` once at module evaluation, not inside the router |

## Gauntlet

Final run after the last edit to `greenVesselIntro.ts`. Node v26.8.2. TypeScript 5.9.3. Vite 5.4.21.

| Layer | Command | Result |
|---|---|---|
| Intro tests | `node --experimental-strip-types --test src/greenVesselIntro.test.ts` | 10 pass, 0 fail |
| Web UI suite | `node --experimental-strip-types --test src/*.test.ts src/lib/*.test.ts src/components/agent/*.test.ts` | 280 pass, 0 fail, 54 suites |
| Types | `tsc` inside `npm run build` | exit 0 |
| Lint | — | skipped: `package.json` has no lint script |
| Coverage | — | skipped: no coverage tool on the node:test runner |
| Mutation | `bash scripts/green-vessel-mutants.sh` | 6/6 killed |
| Property tests | — | skipped: no parse/round-trip invariant beyond the CSS checker |
| Production build | `npm run build` in `crates/ax-web/web-ui` | exit 0. `dist/index.html` contains `id="ax-green-vessel"` |
| Supply chain | — | no new dependencies. `package-lock.json` only aligned the package version `6.4.0` → `7.0.0` to match `package.json` |
| Suite health | single run, no shared state | 280/280, duration 415ms |

Mutation script: `/Users/gary/io/ax/scripts/green-vessel-mutants.sh`. It restores each file and exits non-zero unless every mutant is killed. The non-finite-duration test is the negative control for the CSS checker: a stylesheet with `..s` is rejected (`true !== false` before the `Number.isFinite` fix).

## Review rounds

| Round | Skills | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `javascript-review` usable, `typescript-review` usable, `react-review` usable. `old-coder-api` not applicable (no HTTP change) | 3 minor: comment on the swallowed retry, comment on the page-level audio context, vendor-prefix cast | comments; cast replaced by an `AudioHost` type |
| 2 | same | 1: CSS checker treated a non-finite duration as valid | `Number.isFinite` plus a failing-then-passing test |
| 3 | same, after `tsc` (`Window` has no `AudioContext` in TypeScript 5.9) | 0 | — |

React: the intro is not a component. It runs once from `main.tsx` so React StrictMode cannot play it twice.

## Known limits

- The CSS checker matches substrings. It does not parse CSS.
- Node tests count oscillators on a fake context. They do not play sound through a device.
- `prefers-reduced-motion: reduce` skips the glow. That is an accessibility exception to "play every time".
- The MCP quality drawer uses z-index 12000, above this overlay. It is closed during the intro.
- A hard refresh still depends on the browser allowing `AudioContext`. A blocked chime does not stop the glow. The next click or key tries once.
