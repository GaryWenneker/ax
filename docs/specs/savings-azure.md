# SPEC — Savings page uses Azure Portal colors

Status: autonomous run (spec approval not obtained).
Tier: 2. Isolation: landing tree, so `ax web` can embed the same files.

## Goal

The Savings page charts, heatmap, and token-path graph use the same Azure Portal surfaces and accent as the rest of Command Center, in both Dark and Light.

## Behaviors

### S1 — Series color is the portal accent
Under `html[data-ax-layout="portal"]`, timeline bars, hour bars, “with ax” bars, and the token-path matched stroke use `var(--accent)`. They do not use the GitHub green `#238636` / neon `#39ff8c`.

### S2 — “Without ax” is neutral
Comparison bars and swatches for the without-ax series use `var(--border-hi)`, not the orange gradient `#d18616`.

### S3 — Heatmap scale follows the accent
Portal heatmap levels `--1` through `--4` are mixes of `var(--accent)` on `var(--bg-panel)`. Level `--4` is `var(--accent)`. Empty cells mix `var(--text)`, so Light stays visible.

### S4 — Token path has no neon glow
The matched glow stroke is `transparent`. The plot canvas background is `var(--bg-input)`. Axis labels use `var(--text-dim)`. The plot fade stops use `var(--bg-input)`.

### S5 — Hero numbers match other pages
`.sv-hero-stat-value--ok` is `var(--text-hi)`, same weight as Stats figures. The active time-window chip uses `var(--bg-active)` and `var(--accent)`.

## Must not
- Change savings calculations, API calls, or chart data.
- Remove Azure Light or Dark.
- Reintroduce titlebar wave glow.

## Setup
No new dependencies. Test: `crates/ax-web/web-ui/src/savingsPortal.test.ts`. No checkpoint commit.
