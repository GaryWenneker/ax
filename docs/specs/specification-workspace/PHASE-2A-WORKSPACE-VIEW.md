# Phase 2A — the workspace view

Derived from [SPEC.md](SPEC.md), sections 4 and 6. The owner's feedback on 2026-11-22, with a screenshot of the old flat list, was "De specs ziet er nog steeds niet uit zoals ik dat zou willen. Check opnieuw de spec." This slice therefore brings the workspace layout forward, ahead of the Phase 1 data work. It reads only data that exists today and writes nothing new. Not approved separately; the product spec was accepted with "treat as accepted".

Tier 2: a UI change with no new storage, permission, or concurrency.

## Behaviors

| # | Behavior | Check |
| --- | --- | --- |
| W1 | The page is called "Specs & Plans" in the sidebar and the page header (owner decision 4). | `specsPage.test.ts`: the nav label is `Specs & Plans` |
| W2 | The default view is the workspace: explorer on the left, the open document in the center, context and evidence on the right. "All documents" switches to the current list, unchanged. | `specWorkspacePage.test.ts`; browser screenshots |
| W3 | With the hierarchy flag on, each spec is one expandable row with its plans nested under it, by the same plan id. | `specWorkspace.test.ts`: `hierarchy specs keep their plans nested with one identity` |
| W4 | A legacy plan without a confirmed parent sits under "Needs review", never under an invented spec (criterion 10). | `legacy plans without a parent go to Needs review` |
| W5 | Plans whose relation is not stored are labelled "Unclassified", never "Phase" or "Alternative" (criterion 1). | `plans without a stored relation are unclassified` |
| W6 | Search matches title, id, and path. A spec stays visible when one of its plans matches, showing only the matching plans; a spec that matches shows all its plans. The status filter applies to every section. | `search keeps a spec when only one of its plans matches`, `search on a spec keeps all its plans`, `the status filter applies to every section` |
| W7 | The document header shows title, id, type, status, version, source, and the path in quieter text, with tabs Overview, Content, Plans (specs only), History, Validation, Dependencies. Content is the existing editor (Markdown, WYSIWYG, Split, Diff, version chips, Save, Approve) inside the center pane. | `specWorkspacePage.test.ts`; browser |
| W8 | The context panel has tabs Activity, Sessions, Turns, Memory, Logging, Decisions, Rules & Skills, Context Manifest, each with a count or a stated missing-data reason. Evidence that is not indexed says so and never reads as "no history". | `specWorkspacePage.test.ts`; browser |
| W9 | Sessions, Logging, and Memory list the document's bound sessions; each opens Logging or Memory filtered by that session. Rows in the explorer have no Logging or Memory buttons. | `specWorkspacePage.test.ts`; browser |
| W10 | "New plan" opens in `ModalShell`, not as an inline form (rule `modal-forms`). | `specWorkspacePage.test.ts` |
| W11 | Markdown checkboxes in the open document are counted and shown as "not tracked yet". | `checklist lines are counted, not tracked` |
| W12 | Revised after the first browser check: at 1200 px the explorer was hidden, which left a 1024 px window almost empty. Now, above 1280 px all three panes show. From 900 to 1280 px the explorer and the document show, and the context panel is a drawer. Below 900 px the explorer is a drawer too. Below 720 px one pane shows at a time, chosen with a bar at the bottom of the workspace. | `specWorkspacePage.test.ts`; browser screenshots at 1440, 1024, and 390 px |
| W13 | Every document appears once in "All documents". | `all documents lists every document once` |

## Must not

- Loading the page writes nothing to the database. Check: `sqlite_master` and row counts on the live database are identical before and after a page load.
- The current list, filters, blade, and editor keep working in the All documents view. Check: existing `specsPage.test.ts` assertions pass, except the nav label, which owner decision 4 changes on purpose.
- No fabricated parent, phase, or alternative (W4, W5).
- Lists use the calm list primitives, badges follow `outline-badges`, and contrast follows `wcag-contrast`.
- No new dependency.

## Not in this slice

Resizable panes (this slice has collapse buttons only), stored relation types, checklist tracking, filters by document id with a return link, historical mode, triage actions, and richer hierarchy titles from the server. Each one belongs to a later Phase 1 to 4 slice.

## Checks

- `npm test` and `tsc --noEmit` in `crates/ax-web/web-ui`.
- `cargo build --release -p ax-cli`, with the embedded bundle matching `dist/index.html`.
- Browser screenshots in a temporary project with the hierarchy on, and on the live project after a restart. Desktop, 1024 px, and 390 px.

Setup: the main tree, because the work builds on the uncommitted overhaul. No commits. No new dependency.
