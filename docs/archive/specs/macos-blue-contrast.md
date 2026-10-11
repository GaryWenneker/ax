# SPEC: macOS blue needs more contrast on dark chrome

**Tier:** 2 (theme tokens)
**Spec approval:** not obtained (autonomous; user said the blue is still too dark)

## Context

`#0a84ff` on `#1c1c1e` is only ~4.66:1. `ensureTextContrast` at WCAG AA (4.5) therefore left it unchanged. `ensureWhiteOnFill` darkened fills to `#0974e0` so white labels passed, which made the blue **darker**.

## Behaviors

### B1 — macOS accent is Apple light blue

Preset `macos.accent` is `#64d2ff` (not `#0a84ff`). Contrast vs `macos.bg` (`#1c1c1e`) is **≥ 7:1**.

### B2 — Accent *text* meets AAA

`ensureTextContrast(accent, bg)` default minimum is **7:1**. For macOS that keeps `#64d2ff` (already AAA). For leftover system blue `#0a84ff` it lifts to a lighter hue.

### B3 — Fills are not darkened for white-on-blue

`applyTheme` does **not** run `ensureWhiteOnFill` on `--accent`. Dark-theme fills that miss 7:1 vs `--bg` are **lightened** (same mixer as text), not darkened.

### B4 — Ink on accent fills stays readable

`--accent-on-fill` is `ensureTextContrast('#ffffff', fill)` so selected sidebar / primary-on-accent labels use dark ink on `#64d2ff`. macOS `.nav-item.active` uses that token, not `#ffffff`.

### B5 — Status bar ink matches the painted fill (WCAG AA)

The footer CSS uses `background: var(--accent)`. `applyTheme` must call `statusbarInk(themeAccentFill(theme))` — the same fill written to `--accent` — not `theme.statusbarBg` (macOS charcoal `#2c2c2e` produced **white** letters on **light blue**).

`statusbarInk` picks dark vs light by measured contrast of `#0d1412` / `#f3f3f3` against the fill, then `ensureTextContrast(..., 4.5)` so every theme meets AA. macOS: dark ink on `#64d2ff`.

Enforced by CRITICAL rule `.agents/rules/wcag-contrast.mdc` and tests W8/W9.

## Invariants

- Theme id `macos`, label `macOS`, charcoal surfaces unchanged.
- Other presets stay registered.
- No new npm/cargo dependencies.
- Windows three-exe / macOS Mach-O relink rules unchanged.

## Setup

Isolation: none (theme token edit). Files:

- `/Users/gary/io/ax/docs/specs/macos-blue-contrast.md`
- `/Users/gary/io/ax/crates/ax-web/web-ui/src/lib/themes.ts`
- `/Users/gary/io/ax/crates/ax-web/web-ui/src/lib/themes.test.ts`
- `/Users/gary/io/ax/crates/ax-web/web-ui/src/index.css`
- `/Users/gary/io/ax/tools/gauntlet-macos-theme.sh`
- `/Users/gary/io/ax/docs/specs/macos-blue-contrast-EVIDENCE.md`

Entry: `bash /Users/gary/io/ax/tools/gauntlet-macos-theme.sh`
