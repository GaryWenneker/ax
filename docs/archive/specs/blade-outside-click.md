# SPEC — A click outside the blade closes it

Status: draft, awaiting approval · Tier 2 · Branch `calm-list-style`

Request (2026-09-28 15:19, translated): "Clicking outside must of course also make the blade go away."

## Behaviors

1. With the Logging blade open, a click anywhere outside the blade that is not on a Logging list row closes it
   (page header, filters, the empty list area, sidebar, status bar).
2. A click on another Logging list row does not close it: the blade switches to that row (unchanged).
3. Clicks inside the blade never close it (steps, sections, Copy, prev/next).
4. Pop-ups that belong to the blade but are rendered elsewhere (hover cards, menus, the image lightbox, modals)
   do not close it when clicked.
5. The Memory blade behaves the same: a click outside it, not on a Memory list row, closes it; a row click
   switches it.
6. A drag that starts inside the blade (text selection) and ends outside does not close it.

## Revision 1 (user at approval, 15:21: "for all blades. clicking the same record should toggle selected/unselected")

Scope becomes every blade in Command Center: Logging, Memory, Rules, Skills, Files, Nodes (node detail),
Savings (token view) and Graph (domain node). Rules and Skills already close on an outside click; they keep
their own handler (it respects unsaved edits) and only get the toggle.

7. **Same row toggles.** A plain click (no Cmd/Ctrl/Shift) on the row whose blade is open closes the blade and
   deselects that row. Clicking it again opens it again.
8. Behaviors 1–6 apply to every blade in the scope, each with its own list rows as the "switch, don't close"
   area.

Tests: e2e `e2e/blade-dismiss.spec.ts` per page where the local index has rows (outside click closes, other row
switches, same row toggles, inside click keeps). A page without data in this repo is recorded in EVIDENCE as
not covered by e2e, not silently skipped.

## Must NOT change

Esc and the close button still close; row click and keyboard navigation unchanged; all existing tests.

## Tests (RED first) — `e2e/logging-blade.spec.ts`

- Logging: open the blade, click the page header → blade gone; open, click another row → blade stays;
  open, click inside the blade → stays; drag-select from inside to outside → stays.
- Memory: open the blade, click the page header → gone; click another row → stays.
- Mutant: the outside-click handler disabled.

Implementation: one shared hook `useBladeDismiss(open, onClose, keepSelector)` in
`src/lib/useBladeDismiss.ts` (pointerdown outside `.memory-blade` and outside `keepSelector`, `[role="menu"]`,
`[role="dialog"]`, `.ax-modal-overlay`, `.memory-diff-hover`, `.info-hover`), used by `McpTraceLive` and `Memory`.
No new dependencies.
