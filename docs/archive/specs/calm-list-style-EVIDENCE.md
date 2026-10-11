# EVIDENCE — calm list style (Rules, Skills, Logging)

Spec: `docs/specs/calm-list-style.md` (approved by the user in chat: "Yes, build it as written"; Revision 1 appended during implementation, test mechanism only).
Tier 2. Branch `calm-list-style`, spec commit `b7caeb1`; implementation uncommitted on top of the user's existing uncommitted tree.
Source state: shasum of the 11 changed files = `63839c0d0c5c` (list in `scripts/calm-lists-mutants.sh` / below).
Tools: node v26.8.2 (`node:test`), TypeScript 5.9.3, Playwright 1.62.0 with system Google Chrome (`--project=system-chrome`).

Rerun: `bash scripts/reinstall-cli.sh`, restart `ax web`, then `bash scripts/calm-lists-gauntlet.sh` and `bash scripts/calm-lists-mutants.sh`.

## Behavior → test

| Spec item | Verified by |
|---|---|
| 1 Rules rows are `ItemRow variant="graph"` with badges and toggle | `e2e/calm-lists.spec.ts` "Rules list uses Memory rows with groups"; mutant M1 |
| 2 Skills rows, same | "Skills list uses Memory rows with groups"; M1 |
| 3 Groups are quiet headers that collapse | "Rules group header collapses its rows"; M3 |
| 4 Logging rows use the graph row, keep `data-entry-id`, no badge overflow | "Logging list uses Memory rows"; M4; existing `logging-text.spec.ts` (tool filter badge, text badge, inspector) |
| 5 Selected row = highlight + 1px border; lists scroll | `expectSelectable` (class + 1px border); existing `nodes-scroll.spec.ts`; M2 |
| 6 No tables, outline square badges with icons | `expectCalmList`: zero `table`, every `.page-item-badge` has `.badge-icon`, radius `0px` |
| Row text (subtitle/meta) | `src/lib/calmRows.test.ts` (7 tests); M5 |
| Must not change API calls, filters, blade, toggles | Same handlers reused unchanged (`onRuleRowClick`, `toggleEnabled`, `toggleItemStorage`, `onPolicyMenu`); sort keeps `sortRules`/`sortSkills`; not separately e2e-tested for toggle clicks (known limit) |

## Gauntlet (one fresh run after the last edit)

| Layer | Command | Result |
|---|---|---|
| Unit suite | `node --test src/*.test.ts src/lib/*.test.ts` | 133/133 pass (baseline 126/126 + 7 new) |
| Types | `npx tsc --noEmit` | 0 errors |
| Production build | `npm run build` | exit 0 |
| Served bundle | dist vs `curl /` | match |
| E2E | playwright calm-lists + logging-text + nodes-scroll | 10/10 pass |
| Mutation (manual, 5) | `scripts/calm-lists-mutants.sh` | 5/5 killed, clean control passed first, sources restored (hash check) |
| RED observed | calmRows stub: 7/7 fail; calm-lists on old build: 4/4 fail; overflow check on unfixed build: 2002 overflowing badges | |
| Lint | — | skipped: web-ui has no ESLint config |
| Coverage | — | skipped: no coverage tool in web-ui; mutants stand in |
| Supply chain | — | no dependency changes |
| Real execution | screenshots of Rules, Skills, Logging in `ax web` | `docs/specs/calm-*.png` |

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | old-coder usable, react-review usable, typescript-review usable | 2 (1 major: nested interactive in `role=button` rows; 1 minor: unchecked `as K` cast) | both |
| 2 | same | 0 | — |

## Notes and known limits

- `e2e/mobile-smoke.spec.ts` "logging table allows horizontal scroll" only asserts when `.mcp-trace-table` exists; with no table it now passes without checking anything.
- `logging-text.spec.ts` appends fixture lines to the project's real MCP verbose log (existing test behavior).
- Old table CSS (`.policy-table-row`, `.mcp-trace-table`) is left in `index.css`; other surfaces may still use it.
- In split view (detail blade open) rows hide subtitle and badges except the git dot, like the old id-only table.
- Playwright config gains a `system-chrome` project because no Playwright browser is downloaded on this machine.

## Revision 2 — smaller badges, inset timeline (fresh run after last edit, HEAD b7caeb1 + working tree)

| Spec item | Verified by |
|---|---|
| 7. badges ≤14px, ≤10px text, border ≤1px | `e2e/calm-lists.spec.ts` `expectCalmList` (RED first: received 16) |
| 8. dot ≥12px from row edge, dot ≤12px | same helper, node gap/size assertions |
| 9. no inset origin stripe; dot shows origin color | same helper, `boxShadow` has no `inset`; `--node-color` from `policyDbAccent` |

- Served bundle `index-CCbjjJqA.js` = `dist/index.html`
- `tsc --noEmit`: clean · unit: 133 pass / 0 fail · e2e (calm, logging, md-editor): 13/13
- Mutants: 5/5 killed, sources restored (`scripts/calm-lists-mutants.sh`)
- Not run: `e2e/mobile-smoke.spec.ts` fails under the 1280px desktop project (it expects a phone viewport); outside this gauntlet, not touched.
- Rule `outline-badges` amended: calm-list badges 14px / hairline.
- Screenshot: `docs/specs/calm-skills-rows.png`
