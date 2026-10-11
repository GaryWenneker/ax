# SPEC — Tight hover actions, no browser right-click

Status: draft, awaiting approval · Tier 2 · Branch: `calm-list-style`

## Request (2026-09-28 14:14, translated)

"Edit, delete and similar buttons on hover should look like the GitLens screenshot — certainly not as far
apart as now. Where there is no right-click context menu, a right-click must do nothing. No browser-like
right-click behavior anywhere in the application; it must work like a persistent application."

Answers folded in: text fields and editors keep the browser menu; the icon group holds edit, delete and
MD/DB; the checkbox and toggle stay separate on the far right; the rule is saved as CRITICAL.

## Behaviors

1. **Tight icon group.** On row hover (Rules, Skills; any calm list with `.calm-row-action`), the edit, delete
   and MD/DB buttons are 22×22px icon buttons with 14px glyphs, **≤2px apart**, no border. Hovering over a
   button shows a subtle square background (`rgba(255,255,255,0.08)`); delete turns red on hover.
2. **Separate controls.** The checkbox and the on/off toggle sit 8px after the icon group, on the far right.
   The priority (`p85`) sits right before the group; the row keeps no wide empty reserve.
3. **No browser right-click.** Anywhere in Command Center, a right-click outside a text field does nothing:
   no browser menu (`contextmenu` default prevented).
4. **Own menus still work.** Rules/Skills rows still open the app's own row menu on right-click.
5. **Text fields keep it.** Right-click in `input`, `textarea` or `[contenteditable]` keeps the browser menu
   (copy/paste).

6. **Dimmed toggles** (added at approval, 14:20: "dim the toggles but you can still see they are active; when
   I hover one, it becomes as bright as now, only the one I hover"). In calm-list rows, every on/off toggle
   is shown dimmed (`opacity: 0.5`); an "on" toggle still has its green track, so on vs off stays visible.
   Hovering or focusing a toggle brings that toggle alone to full brightness (`opacity: 1`).

## Revision 1 (user, 14:27: "the checkbox and all the other things must be under each other; when I hover
one of those records, its toggle must have the normal full color")

7. **Aligned columns.** In every Rules/Skills row the priority, the icon group, the checkbox and the toggle
   sit at the same x position as in every other row. Rows without MD/DB or without a toggle (global rows)
   keep an empty slot of the same size.
6′. **Replaces 6.** Toggles are dimmed (`opacity: 0.5`) while their row is not hovered; hovering the row (or
   focusing inside it) brings that row's toggle to full color. Toggles of other rows stay dimmed.

## Must NOT change

- Calm-list, outline-badges rules; all existing unit and e2e tests; row click and keyboard behavior.

## Tests (RED first)

- e2e `e2e/app-feel.spec.ts`:
  - hover a Rules row: the gaps between the visible `.calm-row-action` buttons are ≤2px, each is ≤22px wide;
  - a `contextmenu` event on the Stats page body and on a Logging row is `defaultPrevented`;
  - a `contextmenu` on the search input is **not** `defaultPrevented`;
  - right-click on a Rules row still opens the row menu.
- Mutants added to `scripts/calm-lists-mutants.sh`: the global guard removed; the text-field exception removed;
  the icon gap back to 6px.

## Setup plan

No new dependencies. New files: this spec, `e2e/app-feel.spec.ts`, `src/lib/contextMenuGuard.ts`,
EVIDENCE `docs/specs/app-feel-EVIDENCE.md`. Edits: `src/main.tsx` (install guard), `src/index.css`.
Rule saved via `ax_policy_capture` after this approval.
