# SPEC: Rules and Skills blade: wider list, Markdown or Preview toggle

Tier 2 (UI behavior). Applies to the Rules blade and the Skills blade. The metadata pane stays as it is.

## Behaviors

1. **Wider records.** The record list next to an open blade defaults to 280px instead of 200px, and never goes below 240px. A width saved earlier in the browser that is narrower than 240px is raised to 240px. Dragging the list handle still works up to 480px.
2. **One view at a time.** The rule or skill body shows either the Markdown source or the rendered preview, never both side by side. The side-by-side split and its drag handle are gone from the blade.
3. **Toggle button.** The body pane header has a two-state toggle, "Markdown" and "Preview". The active state is marked with `aria-pressed="true"`.
4. **Default and memory.** The blade opens in Preview. The last choice is saved in the browser (`ax-web-policy-body-view`) and used for the next rule or skill, on both pages. An unknown saved value falls back to Preview.
5. **Editing is unchanged.** In Markdown view, typing updates the body and Save saves it, as today. Switching to Preview shows the unsaved text, with `[[links]]` still clickable.

## Must not change

- The metadata pane (fields, layout, width handle).
- The full-page rule and skill editors (`PolicyRuleEditor`, `PolicySkillEditor`) keep their current split view.
- Existing web-ui tests stay green; `npm run build` exits 0.

## Tests (RED first)

In `src/lib/policyBodyView.test.ts` (node test runner, like the other `lib` tests):

- `loadBodyView` returns `preview` when nothing is saved, `markdown` or `preview` when that is saved, and `preview` for a junk value.
- `saveBodyView('markdown')` then `loadBodyView()` returns `markdown`.
- `clampPolicyListWidth`: `200 → 240`, `100 → 240`, `300 → 300`, `900 → 480`, `NaN → 280`.

Then a manual browser check on both pages: toggle, type in Markdown, switch to Preview, reload, and confirm the choice is kept.

## Setup

No new dependencies. New files: `src/lib/policyBodyView.ts`, its test, and this spec. Work in the current checkout (branch `feat/review-comment-language`, already dirty); no commits unless you ask.
