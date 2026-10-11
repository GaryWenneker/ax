# Green Vessel intro

Tier 2. Isolation: the working tree. The overlay has to land in the Command Center source that `ax web` embeds. No new dependencies.

Spec approval: the user wrote the acceptance criteria in the request and asked for the CSS, HTML, and script to be delivered. A separate approval of this file was not obtained.

## Behaviors

1. On each full page load, `#ax-green-vessel` covers the viewport (`position: fixed; inset: 0; z-index: 9999`), background `#000`, `pointer-events: none`. The page underneath stays hidden.
2. Starting the intro adds `is-playing` and runs `@keyframes ax-green-vessel` for 3.6s. The black cover holds through 50%, then `background-color` fades to `transparent` over the second half (a soft sfumato, not a cut). The rim rises, holds, then dissolves in place: only opacity changes, the colored shape does not scale or shrink. The glow uses `#00ed82` and includes `box-shadow: inset 0 0 60px #00ed82, 0 0 40px #00ed82`. The animation ends at `opacity: 0`. A `transition: opacity` is set on the overlay. The visible peak matches the reference still: a large dark center (`mask-image` is an ellipse, transparent through 74%) and a soft edge wash (`blur` at least 36px). Teal (`70, 230, 220`) covers the top. Green covers the sides and bottom, and the right edge is brighter (`40, 220, 140` at 0.72) than the left (`40, 210, 130` at 0.42). Magenta and amber are absent. The bloom holds at opacity 0.78. The fade is opacity only.
3. `animationend` removes the overlay. A 4000ms timeout removes it if `animationend` never fires. `remove()` is safe to call twice.
4. The chime starts in the same turn as `is-playing`. `play()` rejection is caught. The overlay still gets `is-playing`.
5. When the first `play()` rejects, the next `pointerdown` or `keydown` retries `play()` once. A later event does not play again.
6. Nothing in the intro reads or writes `localStorage` or `sessionStorage`. A second `startGreenVesselIntro` on the same overlay does not call `play()` again.
7. `prefers-reduced-motion: reduce` removes the overlay immediately and does not add `is-playing`. The chime is still attempted.
8. Client-side route changes do not call `startGreenVesselIntro` again. Only `main.tsx` module evaluation does.

## Must not

- Skip the intro based on a stored flag.
- Add a package.
- Change `crates/ax-extraction/src/contracts.rs`.

## Setup

- Files: `crates/ax-web/web-ui/src/greenVesselIntro.ts`, `greenVesselIntro.css`, `greenVesselIntro.test.ts`, `index.html`, `main.tsx`, this spec, `site/src/content/docs/guides/command-center.md`.
- Tests: `node --experimental-strip-types --test src/greenVesselIntro.test.ts` from `crates/ax-web/web-ui`.
- No checkpoint commits unless the user asks.
