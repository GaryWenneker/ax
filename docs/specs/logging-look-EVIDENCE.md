# EVIDENCE — Logging look everywhere (docs/specs/logging-look.md)

Spec approval: obtained 2026-09-28 ("Yes, build it"; background = flat Logging panel; blue in / green out / grey internal).
Tier 2. Source state: HEAD b7caeb1 + working tree on branch `calm-list-style`. Bundle `index-BQCMsRYx.js`.

## Behavior → test

| Spec | Verified by |
|---|---|
| 1. every page card = Logging panel | e2e `every page panel uses the Logging background` (Skills, Rules, Settings: bg `rgba(0,0,0,0.28)`, no gradient) · mutant M9 |
| 2. readable log text | e2e `Logging is readable…`: title font not Cascadia/mono |
| 3. no milliseconds | unit `trace time` (2) · e2e meta matches `YYYY-MM-DD HH:MM:SS` · mutant M6 |
| 4. colored kinds | e2e: IN, OUT and internal badge border colors differ · mutant M8 |
| 5. prompt in vs returned | unit `trace direction` (4) · e2e: subtitle "Prompt in" / "Returned to agent", three distinct dot colors · mutant M7 |
| Must not change | full unit suite, `logging-text.spec.ts`, `md-editor-caret.spec.ts`, earlier calm-list e2e |

## Gauntlet (final fresh run after the last edit)

- `npx tsc --noEmit`: clean
- `node --test src/*.test.ts src/lib/*.test.ts`: 139 pass / 0 fail (RED seen first: 6 fail on stubs)
- e2e `calm-lists`, `logging-text`, `md-editor-caret` (system-chrome): 15/15 (RED seen first: 2 fail — `.005` shown, card transparent + gradient)
- `bash scripts/calm-lists-mutants.sh`: 9/9 killed, sources restored (shasum)
- Served bundle = `dist/index.html`
- Screenshots: `docs/specs/logging-look.png`, `docs/specs/logging-look-skills.png`

## Resolved along the way

- The Revision 2 "no inset stripe" check ran on every list, which conflicted with spec item 5's accent on Logging rows.
  The check is now scoped to Rules/Skills rows, the target it was written for. Assertion unchanged.
- The new e2e first read `.page-item-subtitle`; the real class is `.page-item-sub`. Selector fixed, assertion unchanged.

## Not run

- `e2e/mobile-smoke.spec.ts` needs a phone viewport; it fails in the 1280px desktop project and is outside this gauntlet.
- No dependency changes, so no audit.
