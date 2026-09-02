# SPEC: Wider content container on smaller screens

**Tier:** 1 (layout CSS)
**spec approval:** not obtained (autonomous; user asked to fill width on smaller screens)

## B1 — Laptop band (1377–1919px)

`.workspace > .container:not(.container--full)` must **not** use `max-width: calc(var(--layout-max) - var(--sidebar-w))`. It fills the workspace (`max-width: none`).

## B2 — Tablet (max-width 899px)

The same container is `max-width: none` with `margin-inline: 0` (not centered `layout-max`).

## B3 — Ultrawide (1920+)

Still `max-width: none` (existing). Letterbox `html` background applies only from 1920px, not 1377px.

## Invariants

- Sidebar layout unchanged.
- `container--full` still unconstrained.
- Do not copy Mach-O onto `~/.local/bin`.

## Gauntlet

`bash tools/gauntlet-content-width.sh`
