# SPEC — link picker: toolbar button, every kind, filters and quick search

Tier 2 feature. Isolation: none (the working tree is already mid-change and uncommitted by user convention); no commits.

## Problem

1. The WYSIWYG toolbar's link button asks for a web URL. It should open the picker that links a rule, skill or memory.
2. The `[[` picker shows only rules: with an empty query it keeps the first 20 targets, and rules come first (the project has 47 rules, 31 skills, 171 memories).
3. There is no way to narrow by kind, and no way to search on tags.

## Behaviour

### Toolbar button (WYSIWYG)

- B1 Clicking the link button (or Ctrl/Cmd+K) opens the picker under the button, with a focused search box.
- B2 Picking a target inserts `[[target]]` at the cursor. With text selected, it inserts `[[target|selected text]]`, so the selection becomes the link label.
- B3 Typing a query that starts with `http://` or `https://` shows one first row, "Link to URL <query>"; picking it makes a normal web link on the selection (the old behaviour, still reachable).
- B4 Escape or a click outside closes the picker and returns focus to the editor with the selection intact.

### Picker content (both `[[` and the button, in Markdown and WYSIWYG)

- P1 An empty query shows every kind. Rows are grouped under "Rules", "Skills", "Memory" headers with a count each.
- P2 Filter chips above the list: All · Rules · Skills · Memory (outline badges). Clicking one narrows the list; the active chip is `aria-pressed`.
- P3 The list scrolls; it shows at most 100 rows and then "N more — refine the search".
- P4 The item being edited is still left out (existing `selfKey` behaviour).

### Quick search syntax

- S1 Plain words match name, title and tags, case-insensitive; every word must match (`azure review` → items matching both).
- S2 `#tag` matches only tags, exact tag or tag prefix (`#az` matches `azure`, `azdo`).
- S3 `rule:`, `skill:`, `memory:` (also `r:`, `s:`, `m:`) set the kind filter from the keyboard, e.g. `[[s:#azure`.
- S4 Ranking: exact name match first, then name prefix, then name contains, then tag or title only.
- S5 Each row shows up to three tags as quiet outline badges, with the matched tag highlighted.

### Server

- A1 `/api/links/graph` nodes gain `tags: string[]` (rules, skills, memories; global items too). Additive only; existing fields unchanged.

## Must not

- Change `[[` typing behaviour beyond the new content and syntax (arrows, Enter, Escape stay as they are).
- Change the saved Markdown format (`[[target]]`, `[[target|label]]`).
- Break `e2e/policy-wysiwyg.spec.ts`, `e2e/policy-link-origin.spec.ts` or the unit suite.

## Tests (planned)

- Unit `src/lib/linkPicker.test.ts`: parseQuery (S1–S3), filter + rank (S4, P1, P4), cap (P3), URL detection (B3).
- Rust `crates/ax-web/tests/links_api.rs`: graph nodes carry tags (A1).
- e2e `e2e/link-picker-search.spec.ts`: B1, B2, B3, B4, P1 (all three kinds visible on an empty query), P2, S2, S3.
- Mutants: `scripts/link-picker-mutants.sh`.

## Files

`crates/ax-web/src/links_api.rs`, `crates/ax-web/tests/links_api.rs`, `web-ui/src/lib/linkPicker.ts` (+test), `web-ui/src/components/LinkPicker.tsx`, `WysiwygEditor.tsx`, `MarkdownEditor.tsx`, `index.css`, the new e2e spec and mutant script, `README.md` and `site/src/content/docs/guides/obsidian-vault.md` (docs), EVIDENCE. No new dependencies.
