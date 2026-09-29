# SPEC — Brighter text in the selected row

Status: draft, awaiting approval · Tier 2 · Branch `calm-list-style`

Request (2026-09-28 14:38, translated): "Text in a selected record must be brighter, lighter letters than the others."

## Behaviors

1. In every calm list (Memory, Rules, Skills, Logging), the selected row's title is white `#ffffff`
   (other rows: `#cccccc`).
2. The selected row's subtitle and meta (time, priority) are `#e6e6e6`, lighter than in other rows, and fully
   opaque; any dimming the other rows have does not apply to the selected row.
3. Unselected rows keep their current colors; a disabled row that is selected also gets the brighter text.

## Revision 1 (user at approval, 14:41: "Logging should scroll the same way as Memory, and both should have a
limited scroll")

Interpretation (flagged to the user): "limited" = the scroll stays inside the list's own panel.

4. Logging's list scroller and Memory's list both have `overscroll-behavior: none`: no scroll chaining to the
   page and no browser rubber-band bounce at the top or bottom.
5. Neither page scrolls as a whole: `document.scrollingElement` height equals the viewport on both.

## Must NOT change

Row borders, backgrounds, badges and every existing test.

## Tests (RED first)

- e2e `e2e/calm-lists.spec.ts`: select a row on Memory, Rules and Logging; its title color is `rgb(255, 255, 255)`,
  and its subtitle is lighter than an unselected row's subtitle.
- Mutant in `scripts/calm-lists-mutants.sh`: the selected-text rule removed.

No new dependencies.
