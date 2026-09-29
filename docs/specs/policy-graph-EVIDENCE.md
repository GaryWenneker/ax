# Policy graph — EVIDENCE

Spec: `docs/specs/policy-graph.md` (approved: "Yes, build it as specified"; Revision 1 added during implementation, see below).
Tier 2. Source state: HEAD `b7caeb1` plus the uncommitted working tree (no commits requested).

## Spec revision (visible)
Revision 1: `ForceGraphCanvas` is a new shared component for the policy overlay. `Graph.tsx` is **not** migrated onto it. The two graphs share the tested lib modules instead (settings, filter, hover, forces, labels). `labelAlpha` moved from `Graph.tsx` into `lib/graphLabels.ts`, and the settings load/save functions gained an optional storage key.

## Behaviour → verification
| Behaviour | Verified by |
|---|---|
| `GET /api/links/graph` lists rules, skills, memories, and global items with key `kind:origin:id` and a label | `tests/links_api.rs::g1_graph_lists_every_item_and_each_resolved_link` |
| Edges: one per resolved link; no unknown targets, no self-links, no duplicates | g1 (exact edge set), `links_api::tests::duplicate_and_self_links_make_no_extra_edges` |
| Turn memories are left out (as nodes and as edge ends) | `g2_graph_leaves_out_turn_memories`, `links_api::tests::turn_memories_are_left_out` |
| Existing `/api/links` behaviour unchanged | a1–a4 in `tests/links_api.rs` (green) |
| Client model: drops edges to unknown keys, degree, global flag, outgoing/backlinks direction, null payload | `src/lib/policyGraph.test.ts` (4 tests) |
| Settings stored under their own key | `graphObsidian.test.ts` › graph settings storage key |
| Graph button on the Rules page opens the overlay; legend shows Rule/Skill/Memory/Global; settings closed by default, open via the button, close on outside click; Esc closes | `e2e/policy-graph.spec.ts` test 1 |
| Graph button on the Skills page | e2e test 2 |
| The open item is preselected (pulse ring, blade, Show selection) | e2e test 3 |
| Clicking a node opens its blade; Open closes the overlay and navigates to the item | e2e test 4 |
| No navigation entry added | not changed (diff); no test |
| Code Graph page unchanged | `e2e/graph-obsidian.spec.ts` 5/5, `graph-obsidian-mutants.sh` 16/16 |

## Gauntlet (final fresh run, after the last code edit)
- Rust: `cargo test -q -p ax-web`. All binaries pass: 63 lib tests (incl. 3 new ones), `links_api` 7/7, and the other suites green.
- Clippy: `cargo clippy -q -p ax-web --tests` reports no warnings in `links_api`.
- TS unit: `node --test src/*.test.ts src/lib/*.test.ts` passes 180/180.
- Types: `npx tsc -p tsconfig.json --noEmit` exits 0.
- Lint: skipped. The web-ui has no ESLint config (`eslint.config.*` is missing), so there is no linter to run.
- Mutation: `scripts/policy-graph-mutants.sh` kills 12/12 (7 TS, 5 Rust). The first run had one survivor: removing the `turn` flag from store memories survived because no fixture had a turn memory. Fixed by adding a turn memory to the fixture plus test g2. `scripts/graph-obsidian-mutants.sh` still kills 16/16.
- E2E: `npx playwright test e2e/policy-graph.spec.ts e2e/graph-obsidian.spec.ts --project=system-chrome` passes 9/9.
- Real execution: `./scripts/reinstall-cli.sh` (with `CARGO_TARGET_DIR` unset), then `ax web`. The served bundle `index-Ct4miRPx.js` matches `dist/index.html`. The live `/api/links/graph` returns 246 nodes and 139 edges. Screenshot: `docs/specs/policy-graph.png`.
- Supply chain: no new dependencies. No new network, process, or file access; the endpoint only reads existing stores.
- Coverage on changed lines: no coverage tool is set up for the web-ui or the Rust crates here. Skipped; mutants and e2e stand in.

## Issues along the way
- Early live checks returned 404 on `/api/links/graph`. Cause: the environment sets `CARGO_TARGET_DIR` to a sandbox cache, so builds did not update `target-dev/release/ax`. Rebuilding with `env -u CARGO_TARGET_DIR` fixed it.
- The e2e selector `Graph` also matched the nav item "Graph". The test is now scoped to `#main-content`.
- The canvas component was not built test-first. Its behaviour is covered by e2e and by the shared lib tests.
