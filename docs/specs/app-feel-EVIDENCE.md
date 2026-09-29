# EVIDENCE — Tight hover actions, no browser right-click (docs/specs/app-feel.md)

Spec approval: obtained 2026-09-28 ("Voor mij mag je aan de slag"), with behavior 6 (dimmed toggles) added
in that same reply and folded into the spec verbatim. Tier 2. Branch `calm-list-style`, HEAD b7caeb1 + working tree.
Bundle `index-Ci45LkLY.js` (served = dist).

## Behavior → test

| Spec | Verified by |
|---|---|
| 1. tight icon group (≤22px, ≤2px apart) | e2e `row hover actions form a tight icon group` · mutant M12 |
| 2. checkbox + toggle apart on the right | CSS margin 8px; screenshot `docs/specs/hover-after.png` (no separate assertion) |
| 3. no browser right-click | e2e `right-click does nothing outside text fields` (Stats `main`, Logging row) · mutant M10 |
| 4. own row menu still opens | e2e `own row menu still opens on right-click` (passed before the change: kept as regression armor) |
| 5. text fields keep the menu | same e2e, search input not prevented · mutant M11 |
| 6. dimmed toggles, hovered one bright | e2e `toggles are dimmed until hovered…` · mutant M13 |

## Gauntlet (final run after the last edit)

- `npx tsc --noEmit`: exit 0
- unit: 139 pass / 0 fail
- e2e `app-feel`, `calm-lists`, `logging-text`, `md-editor-caret`: 19/19 (RED first: 3 of 4 new tests failed — 26px buttons, toggle opacity 1, menu not prevented)
- `bash scripts/calm-lists-mutants.sh`: 13/13 killed, sources restored
- Review round 1 (typescript-review, react-review, checked by hand): 0 findings

## Revision 1 (aligned columns; row hover brightens the toggle)

Changed by the user at 14:27; spec text in `app-feel.md` "Revision 1". Bundle `index-DKYvoO86.js` (served = dist).

| Spec | Verified by |
|---|---|
| 7. aligned columns (priority, checkbox, last control) on Rules and Skills | e2e `row controls line up in columns across rows` (RED first: 2 distinct x positions) · mutant M15 |
| 6′. row hover brightens that row's toggle only | e2e `toggles are dimmed until their row is hovered…` · mutant M14 |

- e2e `app-feel`, `calm-lists`, `logging-text`, `md-editor-caret`: 20/20 · `tsc`: exit 0
- Mutants: 15/15 killed, sources restored
- Screenshot: `docs/specs/columns-after.png`
- Honest note: the first RED for 6′ was a timeout caused by a wrong locator in the test (a page-wide `has`
  filter), not by the behavior. The locator was fixed without changing any assertion, and mutant M14 proves
  the corrected test fails when the row-hover rule is missing.

## Notes

- Rule saved: `.agents/rules/no-browser-context-menu.mdc` (CRITICAL).
- Behavior 2 is shown by a screenshot only, with no separate assertion.
- A first green e2e run went to a Vite server that had already exited; it was discarded and everything was rerun against the rebuilt `ax web`.
