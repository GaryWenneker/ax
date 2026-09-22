# SPEC: Full-bleed chrome, staged inner content

Tier 1. Spec approval: not obtained (autonomous run).

## B1 — Header and footer paint full viewport

`.app` is `width: 100%`. `.titlebar` and `.statusbar` span the full grid (edge to edge).

## B2 — Inner chrome matches the article stage

`.titlebar-inner` and `.statusbar-inner` use `width: min(100%, var(--stage-w))` and `margin-inline: auto`.

## B3 — Nav + article sit in the same stage

`.app` columns: `1fr | sidebar | sizer | minmax(0, layout-max) | 1fr` so gutters center the nav+content block with the header/footer inner.

## B4 — Ultrawide widths

`--layout-max`: 1100 / 1320 @1920 / 1480 @2560.

## Gauntlet

`bash tools/gauntlet-content-width.sh`
