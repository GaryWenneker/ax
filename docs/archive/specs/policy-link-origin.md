# SPEC — Links to a rule or skill open the right item (no false "not found")

Tier 2 bug fix. Reported: moving from one rule/skill to another (skill→rule, rule→skill, skill→skill, rule→rule) often shows "not found" for an item that exists, e.g. the global skill `azdo-pr-review`.

## Cause

A global item stored under another project needs `origin=global&projectId=<n>` on its API calls. The Rules and Skills pages only take the **name** from the URL. The origin comes from whichever row you last clicked, so:

- a `[[link]]`, the graph, the browser back button or a reload to a global item fetches it as a project item: 404;
- after opening a global item, a link to a project item is fetched as global: 404.

Row clicks also navigate without the origin, so the URL of an open global item can't be shared or reloaded.

## Behaviors

- L1: a URL with `origin=global&projectId=n` (from a link, the graph, back/forward or a reload) opens that global item. The body loads; no "not found".
- L2: after a global item is open, following a link to a project item opens the project item.
- L3: a URL with only a name (old links, typed URLs) opens the item when it exists. The project copy wins when there are both; otherwise the global copy, with its projectId, from the list.
- L4: clicking a row puts that row's origin and projectId in the URL, so reload and back/forward reopen the same item.
- Same for rules (`?id=`) and skills (`?name=`).

## Must not

- Change any API or the look of the pages.
- Break existing specs: `policy-graph.spec.ts`, `policy-wysiwyg.spec.ts`, and the blade-dismiss tests that pass today (6 fail already, see the WYSIWYG EVIDENCE).

## Tests

- RED already observed: `e2e/policy-link-origin.spec.ts` L1–L3 for skills and rules (6/6 fail today). Each test picks a global item the server only finds with its origin (the real failing case) and skips when the machine has none.
- New unit test `src/lib/policySelection.test.ts`: `resolveOpenTarget(rows, name, origin, projectId)`, covering the route origin, project-first, global fallback, and unknown names.
- L4: e2e — click a global row, and the URL carries `origin=global&projectId=`.
- Gauntlet: tsc, `npm run build`, the node tests, the e2e files above, manual mutants (added to a script), reinstall plus bundle check.

## Setup plan

- Isolation: none (working tree, as for the previous tasks; no commits).
- Files: `src/lib/policySelection.ts` (plus its test), `src/pages/PolicyRules.tsx`, `src/pages/PolicySkills.tsx`, `src/App.tsx`, `e2e/policy-link-origin.spec.ts`, `scripts/link-origin-mutants.sh`, EVIDENCE.
- No new dependencies.
