# SPEC — WYSIWYG mode for the rule and skill body editor

Tier 2 feature. Today the body card (Rules and Skills blades) has two views: **Markdown** (source) and **Preview** (read-only). This adds a third view, **WYSIWYG**: you edit the rendered text directly, with formatting shortcuts and a small toolbar, and it is saved as Markdown.

## Behaviors
- W1: the toggle shows **Markdown · WYSIWYG · Preview**. The choice is remembered, like today (`ax-web-policy-body-view`). An unknown stored value falls back to Preview, as it does now.
- W2: in WYSIWYG you can type and use:
  - headings H1–H3, bold, italic, inline code, bullet and numbered lists, block quotes, code blocks, and links;
  - toolbar buttons for the same things;
  - the shortcuts Cmd/Ctrl+B, Cmd/Ctrl+I and Cmd/Ctrl+K (link).
  Every edit updates the body as Markdown, so **Save** and **History** work as they do today.
- W3: no silent rewrite. Opening WYSIWYG and switching away without typing leaves the body byte-identical, and the Save button does not light up. The Markdown is only rewritten after a real edit.
- W4: things that must survive an edit in WYSIWYG:
  - `[[wiki links]]`, which are not escaped to `\[\[`;
  - GFM tables;
  - fenced code blocks with their language;
  - task lists;
  - HTML comments.
  Anything the editor can't show stays in the Markdown as written.
- W5: switching Markdown → WYSIWYG → Markdown after an edit shows the updated source. Switching items (another rule) loads that item's body; it never keeps the previous one.
- W6: dark theme matching Command Center; text contrast meets WCAG AA; the toolbar is keyboard reachable and has labels.

- W7 (Revision 2, at the user's request: "when [[ is used allow it to find a rule or skill or memory to connect to as a link"): typing `[[` opens a picker. It works in the WYSIWYG view and in the Markdown source view.
  - It lists rules, skills and memories, project and global, with a kind badge and the title. Typing after `[[` filters by id or title, case-insensitive, with at most 20 rows. Arrow keys move the selection; Enter or Tab inserts; Esc closes and leaves the typed `[[` alone; a click inserts.
  - Inserting writes the link the resolver already understands: `[[<rule id>]]`, `[[<skill name>]]`, or `[[memories/<stem>]]` for a memory. In WYSIWYG it shows as a link chip. The Backlinks and graph pick it up after Save, as today.
  - The item you are editing is not offered. Duplicate labels show their kind and origin, so they can be told apart.
  - Data comes from the existing `GET /api/links/graph`. If a node lacks the exact link target (the memory stem), the endpoint gets one extra field, `target`, which is additive and doesn't break anything. It is loaded once per blade, and a failed load shows "Links unavailable" in the picker; it never blocks typing.

## Must not
- N1: change the Markdown or Preview views, the Save format, or the API.
- N2: add a network call; the editor is bundled.

## Setup plan
- New dependencies (web-ui only). Revision 1, the user chose TipTap over Milkdown:
  - `@tiptap/react`, `@tiptap/pm` and `@tiptap/starter-kit`: the editor, and the headings, lists, quotes, code and bold/italic;
  - `@tiptap/extension-link`: links;
  - `@tiptap/extension-table` (with row, header and cell): GFM tables;
  - `@tiptap/extension-task-list` and `@tiptap/extension-task-item`: task lists;
  - `@tiptap/markdown`, TipTap's official Markdown parse/serialize extension: loads and saves the body as Markdown.
  - `@tiptap/suggestion`: the `[[` picker in WYSIWYG (W7). The Markdown view's picker is a small hand-written overlay on the existing textarea, with no extra dependency.
  - Rejected: Milkdown (the user's choice); Toast UI (outdated React wrapper, large).
- Extra risk with TipTap: its Markdown parser (markdown-it) differs from the preview's (remark), and it doesn't know `[[wiki links]]` or HTML comments. Mitigation: a small custom inline node for `[[links]]` and one for HTML comments, both serialized back verbatim. W4 is tested with fixtures, and anything else that fails the round-trip is reported in EVIDENCE rather than hidden.
- New files:
  - `src/components/WysiwygEditor.tsx`;
  - `src/lib/wysiwygMarkdown.ts` (the round-trip helpers) with `wysiwygMarkdown.test.ts`;
  - `e2e/policy-wysiwyg.spec.ts`.
- Edited: `PolicyBodyCard.tsx`, `lib/policyBodyView.ts` (+ test), `index.css`, the docs (`guides/command-center.md`, README).
- `npm audit` for the new dependencies; production build; bundle-size note in EVIDENCE.
- No commits. Rebuild and verify with `scripts/reinstall-cli.sh`.

## Tests (RED first)
- unit: `loadBodyView` accepts `wysiwyg`; round-trip fixtures (wiki links, table, fenced code, task list, HTML comment, nested list) go through parse → serialize with the W4 items intact.
- e2e (Playwright, system Chrome):
  - switch to WYSIWYG and back without typing, and Save stays disabled (W3);
  - type bold text with Cmd+B, and Markdown view shows `**…**` (W2, W5);
  - a body with `[[startup]]`, edited in WYSIWYG, still contains `[[startup]]` (W4);
  - open another skill, and the editor shows its body (W5);
  - W7, in both views: type `[[sta`, and the picker shows `startup`; Enter inserts `[[startup]]`; Esc leaves `[[sta`; a memory inserts `[[memories/<stem>]]`; the current item is not listed.
- unit (W7): `filterLinkTargets(items, query, selfKey)` handles ranking, the 20-row limit and excluding the current item; `linkText(item)` gives the right syntax per kind.
- Rust (only if `target` is added): `links_api` graph nodes carry `target`, and the existing graph test stays green.
- Gauntlet: vitest, tsc, lint, production build, e2e, manual mutants, review loop, EVIDENCE.

## Revision 3 (during implementation)

- W3: the clause "the Save button does not light up" is removed. Save is never disabled when a rule opens, in any view, including the Preview view that existed before this change, so the clause rested on a false premise. W3 is now checked as written in its first sentence: the body stays byte-identical after opening WYSIWYG and switching away, including for a body TipTap would normalise (`*` bullets, `__bold__`, an unpadded table).
- W4: a table survives with the same cells, but TipTap pads its columns (`| a   | b   |`). The check accepts any spacing inside the pipes.
- W5 gains two concrete checks: a client-side switch to another rule while WYSIWYG is open, and a restored revision; both must show the new body unchanged.
