# SPEC — Graph edits items, auto-group, memory file status + images + click-diff

Status: APPROVED ("Yes, build it as specified"; grouping = local text similarity, memory = merge from ax-hygiene). Tier 2.

## Findings (why memory broke)
The Memory blade in this checkout (`/Users/gary/io/ax`) calls four endpoints: `/api/memory/{id}/file-changes`, `/file-links`, `/diff` and `/image`. The server here does not have them. Live, all four return 404, so file colors, git links, diffs and images silently disappear. The handlers exist only as **uncommitted changes in `/Users/gary/io/ax-hygiene`**: `crates/ax-web/src/memory.rs` (+395 lines) and `crates/ax-memory` (history, turns, store, lib, about +900 lines). The `ax-memory` files in the two checkouts have diverged, so this is a merge, not a copy.

## 1. Policy graph: a click opens the edit blade
- Clicking a rule node in the overlay opens `PolicyRuleInlineWorkspace` (the same edit blade as the Rules page) on the right side of the overlay. A skill node opens `PolicySkillInlineWorkspace`. Both get the item's `origin` and `projectId`, so global items can be edited too.
- A memory node opens the Memory blade (as on the Memory page).
- The graph stays open and interactive next to the blade. The pulse ring stays on the edited node. Saving reloads the graph, so changed links show up at once.
- Closing the blade (×) keeps the graph open. Esc closes the blade first, then the graph.
- The info blade with the "Open" button goes away.

## 2. Auto-group ungrouped rules and skills
- A new **Auto-group** button on the Rules and Skills pages.
- It only considers items **without a group**, and it only uses **existing groups**. It never creates new groups.
- Scoring is deterministic and needs no LLM or network. Each item is tokenized from its id/name, description, tags, triggers/globs, and body. For each existing group, the item's token profile is compared to the profile of that group's members (cosine similarity over TF-IDF). The best group wins if its score is ≥ 0.15. Otherwise the item stays ungrouped ("no good match").
- It never applies directly. First a preview dialog shows each item with its proposed group and score, one checkbox per row (checked by default), and a group dropdown to change the proposal. **Apply** saves only the checked rows.
- A pure module `lib/autoGroup.ts` with node:test tests: items already grouped are untouched; no groups means no proposals; the best-scoring group wins; a score below the threshold gives no proposal; the result is the same for the same input.

## 3. Memory: file status, images and diff back
- Port the four endpoints and their `ax-memory` support from `ax-hygiene` into this checkout, merged with the local `ax-memory` changes (neither side's work may be lost). Bring along the tests from `ax-hygiene` (`crates/ax-memory/tests/history.rs` and the web tests) and make them green here.
- Result: file rows are colored again (added green, modified orange, deleted red), with git links, and images in the memory body render again.

### Revision 1 (section 3)
The support is not only in `ax-hygiene`'s uncommitted changes. It is also in **v5.2.0 on `origin/main`** (`crates/ax-memory/src/history.rs`, turn outcomes in `turns.rs`, `store.rs`, `lib.rs`), 25 commits this branch lacks. Chosen approach: **port file by file, with no git merge.** Bring `ax-memory` (history.rs, turns.rs, store.rs, lib.rs, Cargo.toml, tests/history.rs) and the four `memory.rs` endpoints from `ax-hygiene` into this checkout. Merge them by hand with the local `ax-memory` changes (format.rs +59, store.rs +43, tests/vault.rs +45), so neither side is lost. Nothing else from v5.2.0 comes along. When you later merge `origin/main` into this branch, the files may conflict on identical changes; I record that in EVIDENCE.

## 4. File diff only opens on click
- A file row in the Memory blade no longer opens a popup on hover. It opens on click, just like commits already do (`clickToOpen`). The popup shows the diff with line numbers (the existing `FileDiff`). Close, Esc, or a click outside closes it.
- The file's git link stays reachable through a separate small icon in the row, because a click on the row now opens the diff.
- The info text ("Hover a file…") is updated.

## Must not change
- The Rules/Skills page blades, the code Graph page, and the `/api/links*` endpoints.
- The existing a1–a4, g1 and g2 tests, and all unit and e2e suites.

## Tests
- Rust: the ported `ax-memory` history tests. New `ax-web` integration tests: `file-changes`, `diff` and `image` return 200 for a git memory and a turn memory, 404 for an unknown id, and 400 for a path traversal (`../`).
- TS: `autoGroup.test.ts`.
- E2E `e2e/policy-graph.spec.ts` (updated): a node click shows the rule edit blade (Metadata + Rule body) inside the overlay. Save stays in the overlay. Esc closes the blade, and a second Esc closes the graph.
- E2E `e2e/memory-files.spec.ts`: file rows have a status class; hover does **not** open a diff; a click opens it; an image in the body loads (`naturalWidth > 0`).
- E2E auto-group: the preview dialog shows proposals, and Apply puts the item in its group.
- Mutants: `scripts/policy-graph-mutants.sh` extended with autoGroup and click-to-open mutants.

## Setup
- Isolation: the working tree of `/Users/gary/io/ax` (same as before; no commits unless you ask). `ax-hygiene` is only **read**, never changed.
- No new dependencies.
- Rebuild with `env -u CARGO_TARGET_DIR ./scripts/reinstall-cli.sh`, then restart `ax web` and check the served bundle.
- Docs: `architecture-insights.md` (graph), plus the memory and policy guide pages.
