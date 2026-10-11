# EVIDENCE — policy graph edit, auto-group, memory file status, images, click-only diff

- SPEC: `docs/specs/policy-graph-edit-autogroup-memory-diff.md` (approved: "Yes, build it as specified"; revision 1 approved: "Yes, build it as revised")
- Source state: HEAD `b7caeb1` plus an uncommitted working tree (no commits requested)
- Tier 2
- Isolation: none. The work was done in the user's working tree. The other uncommitted changes in that tree were not touched.

## Behaviors → tests

| Behavior | Verified by |
|---|---|
| Clicking a rule or skill node in the graph opens its edit blade (the inline editor with Save) | `e2e/policy-graph.spec.ts` "clicking a rule or skill node opens its edit blade"; "the open rule is preselected with its blade" (expects `Edit rule` + Save) |
| Esc closes the blade first, then the graph | `e2e/policy-graph.spec.ts` "the open rule is preselected…" (Esc → blade gone, overlay visible → Esc → overlay gone) |
| Saving in the graph blade redraws the graph | `onSaved` reloads `/api/links/graph`. **Not covered by a test**, because a real save would change your live policy. |
| Auto-group: only ungrouped items get proposals, the best group wins, a score below 0.15 gives no proposal, the ungrouped bucket is never a target, label and aliases count, results are deterministic, no groups gives no proposals | `src/lib/autoGroup.test.ts` (7 tests, RED 0/7 against a stub → GREEN 7/7) |
| Auto-group preview opens from Rules and Skills and saves nothing on Cancel | `e2e/policy-graph.spec.ts` "Auto-group › rules/skills page previews suggestions without saving" |
| Apply writes `frontmatter.group` via fetch + save | `AutoGroupModal.apply`. **Not covered by e2e** for the same reason (it would write to live policy). |
| Memory file rows are colored added/edited/deleted | `crates/ax-web/tests/memory_files.rs` m1–m4; `e2e/memory-files.spec.ts` (`memory-file--added`) |
| Turn memories record `Changes:` (A/M/D); a file that was already dirty and not touched again is not listed | `turn_hook` tests `t1_changes_record_added_modified_and_deleted` (RED → GREEN) and `t3_…` (a `Changes:` assertion added after a surviving mutant) |
| Hover shows no popup; a click opens the diff with lines; Esc closes only the popup | `e2e/memory-files.spec.ts` (RED on the hover step before the change; the "blade stays visible after Esc" assertion caught a real bug, fixed with a capture-phase listener) |
| Images in the memory preview render | `e2e/memory-files.spec.ts` (`naturalWidth > 0`); ported `/image` route unit tests |

## Gauntlet (final fresh run after the last code edit)

| Layer | Command | Result |
|---|---|---|
| Rust tests | `env -u CARGO_TARGET_DIR cargo test -q -p ax-memory -p ax-web -p ax-cli` | every suite ok, 0 failed |
| Web unit tests | `node --test src/**/*.test.ts src/*.test.ts` | 187/187 pass |
| Types | `npx tsc --noEmit -p .` | 0 errors |
| Clippy | `cargo clippy -p ax-memory -p ax-web -p ax-cli --all-targets` | 0 findings in changed files. Two style warnings (`question_mark`) in the ported `turns.rs` were fixed. `-D warnings` still fails on errors that already existed in untouched crates (`ax-installer`, `ax-telemetry`, and others); these were not fixed (out of scope). |
| ESLint | — | skipped: web-ui has no ESLint config |
| Mutation (manual, cmp-proved) | `env -u CARGO_TARGET_DIR bash scripts/policy-graph-mutants.sh` | **21/21 killed**. The first run was 20/21: the survivor `wanted.contains → true` in `file_change_lines` was killed after the `t3` assertion was added. |
| e2e (real execution) | `npx playwright test e2e/memory-files.spec.ts e2e/policy-graph.spec.ts e2e/graph-obsidian.spec.ts --project=system-chrome` against the rebuilt `ax web` (served bundle `index-BwmSjJ_G.js` = `dist/index.html`) | 12/12 pass |
| Real data | auto-group against the live `/api/policy/rules` and `/api/policy/skills` | 5 of 34 ungrouped rules and 12 of 18 ungrouped skills get a suggestion. Most language rules (go-*, rust-*, python-*, and so on) get no match, because the catalog has no group for them. |
| Supply chain | — | new deps in `ax-memory`: `chrono`, `blake3`, ported from ax-hygiene. Both are already in the workspace lockfile. No new npm deps. |

## Deviations and known limits

- **Memory nodes in the graph still use the info blade with Open**, not the memory blade. `MemoryBlade` is tied to the Memory page layout, which it portals into. The SPEC asked for the memory blade here; this is a declared deviation.
- `clickToOpen` has no mutants in the script (each mutant would need a web rebuild). It is covered by the e2e hover/click steps instead, which were observed RED.
- Turn retention changed from 30 to 90 days, with a backup under `.ax/backups/` before pruning (ported behavior).
- Added a singular-term coverage rule in `store.rs` so that "dropdowns" still matches "dropdown" when the local weak-term filter applies.
- Old turns that have only a `Files:` section stay uncolored.
- The port from ax-hygiene was done by hand, file by file. A later merge of origin/main may conflict in `ax-memory` and `ax-web/src/memory.rs`.
- Spec approval was obtained. No independent verification was done (Tier 2).
