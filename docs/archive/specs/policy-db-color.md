# SPEC: Rules/Skills database color

Tier 2. Spec approval: not obtained (autonomous run after user request for visual DB membership by color).

## Goal

Command Center Rules and Skills overviews show **which SQLite database** a row lives in using **solid color**, not a faint tint.

## Colors (fixed)

| Database | Fill | Ink |
|---|---|---|
| This project `.ax/ax.db` (`origin` missing or not `global`) | `#3ee4b2` | `#141414` |
| `~/.ax/global.db` (`origin === "global"`) | `#e0b341` | `#141414` |

## Behaviors

1. `policyDbAccent(undefined)` and `policyDbAccent("project")` return `#3ee4b2`.
2. `policyDbAccent("global")` returns `#e0b341`.
3. `policyDbRowStyle(origin)` returns `boxShadow: inset 5px 0 0 <accent>`.
4. Origin badges use those fills with `#141414` text (not `color-mix` translucent tints).
5. Compact (ID-only) and full tables apply the same left inset bar.

## Must not

- Change agent matching (still current project `ax.db` only).
- Use white text on teal/gold fills.

## Setup

No new npm packages. Isolation: none (UI + unit tests in tree). Gauntlet: `node --test` on `gitShare.test.ts`, `tsc` via `npm run build` in web-ui.
