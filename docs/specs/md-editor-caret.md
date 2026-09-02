# SPEC — Markdown source caret sync

- Tier: 2 (bug fix: click/caret desync in Command Center rule/skill editor)
- spec approval: the user pasted this document as the task contract (2026-09-01 revision including selection-ghosting)
- Setup plan:
  - Tools to install: none
  - Git: work on the current branch; no checkpoint commits unless asked
  - Isolation: none (CSS + one Playwright spec; worktree would drop `node_modules` and cannot run the gauntlet without a rebuild)
  - Files the gauntlet will add, by path:
    - `/Users/gary/io/ax/crates/ax-web/web-ui/e2e/md-editor-caret.spec.ts`
    - `/Users/gary/io/ax/tools/gauntlet-md-editor-caret.sh` (this machine)
    - `/Users/gary/io/ax/tools/gauntlet-md-editor-caret.ps1` (Windows entry, same layers)
  - Files that may change:
    - `/Users/gary/io/ax/crates/ax-web/web-ui/src/index.css` (font-metric sync for the uiw overlay)
    - `/Users/gary/io/ax/crates/ax-web/web-ui/src/components/MarkdownEditor.tsx` only if a CSS-only fix is insufficient
  - New dependencies: none

## Diagnosis (measured on live `http://127.0.0.1:7070` rule `agent-workflow`)

`@uiw/react-md-editor` paints source as two stacked layers:

1. Visible glyphs: `pre.w-md-editor-text-pre > code` (syntax highlight, `pointer-events: none`)
2. Caret + clicks: `textarea.w-md-editor-text-input` (transparent text, on top)

Our theme overrides font-size / line-height on `pre` and `textarea`, but uiw's more specific rule keeps the inner `code` at 14px / 18px. Live computed styles:

| Layer | font-size | line-height | content width |
|---|---|---|---|
| overlay `code` (what you see) | 14px | 18px | 388px |
| textarea (where the caret lives) | 13px (`--fs-base`) | 19.5px (`1.5`) | 403px box / ~383px text |

Clicks hit the textarea at the screen point of the overlay. Because wrap metrics differ, that point is a different character — the blue caret sits in `tools are available` while the user clicked later in the document.

**Selection (second screenshot):** the textarea is ` -webkit-text-fill-color: transparent ` so only the overlay is visible — until you select. Selected textarea glyphs become opaque (white on blue) while the overlay stays put at the *other* wrap. Result: a blue block that starts mid-word, plus a second offset copy of the same lines (`read only the files…` / `does **not** satisfy` ghosted on top of the overlay). Same root cause, different symptom.

## Scenarios

Feature: Markdown source caret matches the visible glyphs

  Scenario: overlay code and textarea share font metrics
    Given the rule editor is open on a document that contains the phrase `tools are available`
    When computed styles are read from `.w-md-editor-text-pre > code` and `textarea.w-md-editor-text-input`
    Then `font-size` is equal
    And `line-height` is equal
    And `font-family` is equal

  Scenario: click the last source line places the caret at the document end
    Given the rule editor is open on `agent-workflow` (or an equivalent body that ends with `policy-engine/)`)
    When the editor input area is scrolled to the bottom
    And the user clicks the last source line (the `Full guide` / policy-engine URL line) via the textarea at that visual Y
    Then `textarea.selectionStart` is within 2 characters of `textarea.value.length`
    And the 40 characters before the caret include `policy-engine`

  Scenario: click after a long wrapping bold line does not land mid-word
    Given the source contains a single long blockquote line that includes `**ABSOLUTE**` and ends with `tools are available.`
    When the user clicks at the visual end of that line (right edge of the last wrapped segment)
    Then `textarea.selectionStart` equals the index of the newline after that line (or `value.length` if it is the last line)
    And the caret is not inside the substring `available`

  Scenario: drag-select a wrapping paragraph does not ghost a second copy
    Given the rule editor is open on a body that contains both
      `read only the files the graph already pointed to`
      and `does **not** satisfy`
    When the user sets a textarea selection that covers those two phrases
    Then `textarea.scrollHeight` and `pre.w-md-editor-text-pre` height differ by at most 24px (textarea padding only)
    And computed `textarea::selection` `-webkit-text-fill-color` (or `color` if fill is not set) is `transparent`
    And a screenshot of the source pane does not show the same word painted twice at different offsets
      (automated check: overlay range rects for `does **not** satisfy` and the textarea selection highlight share the same first-line `top` within 3px)

## Must NOT

- Change save / load / frontmatter behavior of the rule or skill editor
- Change rendered Markdown preview HTML (right pane)
- Add npm / Cargo dependencies
- Remove syntax *colors* in the source pane (color-only highlight stays allowed)
- Change `--fs-base` or `--ui-scale` globally
- Leave `tmp-caret-probe.mjs` or other probe scripts in the tree

## Revisions

- 2026-09-01: initial spec from live metric probe on `agent-workflow` (overlay 14px/18px vs textarea 13px/19.5px).
- 2026-09-01: added selection-ghosting scenario after a second screenshot (blue highlight + double-painted `read only the files…` / `does **not** satisfy`). Same metric mismatch; selected textarea glyphs become visible on top of the overlay. Prior spec approval (if any) does not apply to this revision.
