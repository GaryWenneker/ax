# Spec: Memory list stays put, toggles follow selection, kind colors (draft)

User, 15:43: "wanneer ik onder Memory een record aanklik, dat de knoppen bovenin dan de content naar beneden
drukken. Dat wil ik niet … de lijst op dezelfde locatie blijft staan … als ik eenmaal weer weggeklikt heb, dat dan
de tuimelschakelaar wel zichtbaar blijft als actief … check ook bij andere schermen … categoriseren en dan de
rondjes aan de linkerkant dan ook specifieke kleuren … vergeet niet de legenda."

Tier 2. Isolation: the current working tree, as with every earlier spec in this series (the uncommitted UI work lives there).

## Findings

- Opening a blade narrows `.container` by 480px. `.settings-hero` then wraps its actions under the title and the
  Memory stats strip wraps labels ("Git commits") to two lines, so the list moves down.
- After a mouse click the row keeps DOM focus. The rules `…:is(:hover, :focus-within) > .settings-toggle` keep the
  toggle bright/visible after the row is deselected. Same selector family on calm lists (Rules, Skills, Logging).
- Memory kinds in ax.db today: git 158, turn 87, convention 3, bug_fix 3, decision 2, fix 1, architecture 1.

## Behaviors

1. **List does not move.** Memory at 1280×800: the top of the first list row is at the same y (±1px) before a row
   is clicked, while the blade is open, and after it closes. The header keeps its actions on the title line
   (the subtitle wraps instead) and the stats strip labels stay on one line.
2. **Blades on Rules, Skills and Logging**: same measurement for the first list row (±1px).
3. **Toggle follows hover, not mouse focus.** Click a row, click it again (deselect), move the mouse off the list:
   that row's toggle has the same opacity as an untouched row's toggle. Keyboard focus (Tab) still brightens
   it (`:focus-visible`). Checked on Memory, Rules, Skills.
4. **Kind categories color the row node.** Categories and colors:
   | Category | Kinds | Color |
   |---|---|---|
   | Commit | `git` | orange |
   | Chat turn | `turn` | blue |
   | Decision | `decision`, `architecture` | purple |
   | Convention | `convention` | teal |
   | Fix | `fix`, `bug_fix` | red |
   | Note | anything else | grey |
   Pure function `memoryCategory(kind)` in `src/memoryCategory.ts`, unit tested for every kind above plus an
   unknown kind.
5. **Legend** above the Memory list, same style as the Logging direction legend, one swatch + label per category.
   Only categories that occur in the loaded list are shown.
6. Conversation grouping (the connecting line between turns of one chat and the `n/m` badge) stays; see Revision 1.

## Revision 1 (user answer 15:5x: kind color on the node for turn rows)

Turn rows show the Chat turn color on the node. The per-conversation hue no longer colors the node; the
connecting line and the `n/m` badge still group turns of one chat. The approval ("Approved as written") was given in
the same exchange as this answer, so it covers the draft; this revision was not re-approved separately.

## Must NOT change

Existing e2e suites (calm-lists, app-feel, logging-blade, blade-dismiss toggle tests), unit tests, badge styles,
contrast rule (swatch colors ≥ 3:1 on the panel background, WCAG non-text contrast).

## Setup plan

No new dependencies. New files: `src/memoryCategory.ts`, `src/memoryCategory.test.ts`,
`e2e/memory-stable.spec.ts`. Mutants appended to `scripts/calm-lists-mutants.sh`.
