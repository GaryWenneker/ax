# EVIDENCE — link picker: toolbar button, every kind, filters and quick search

Spec: `docs/specs/link-picker-search.md` (approved: "Yes, build it"). Tier 2. No commits (user convention); source state is the uncommitted working tree. Isolation: none, as declared in the spec.

## Why only rules showed

With an empty query the picker kept the first 20 targets, and the graph lists rules first (47 rules, 31 skills, 171 memories in this project). The cap is now 100 and is shared across kinds, so one long kind cannot hide the others.

## Behaviour → test

| Spec | Test | Result |
|---|---|---|
| B1 link button / ⌘K opens the picker, search focused | e2e B1 ×2 | pass |
| B2 insert at cursor; selection becomes the label | e2e B2 ×2; unit `wikiRaw` ×2 | pass |
| B3 http(s) query offers "Link to URL" | e2e B3; unit `isUrlQuery` ×2 | pass |
| B4 Escape and outside click close; focus back in editor | e2e B4 ×2 | pass |
| P1 every kind on an empty query, grouped with counts | e2e P1 ×2 (WYSIWYG button, Markdown `[[`); unit P1 ×2 + counts | pass |
| P2 filter chips; start at All on each open | e2e P2 ×2; unit P2 | pass |
| P3 cap 100 + "N more" | unit P3 | pass |
| P4 current item left out | unit P4 | pass |
| S1–S4 words, `#tag`, `s:`/`r:`/`m:`, ranking | unit S1–S4 (9 tests); e2e S2, S3 | pass |
| S5 up to three tags per row, matched tag highlighted | e2e S2 (`.link-picker-tag--hit`) | pass |
| A1 graph nodes carry `tags` | Rust `g3_graph_nodes_carry_tags` | pass |
| Must not: `[[` typing keys unchanged | `e2e/policy-wysiwyg.spec.ts` W7 ×4 | pass |
| Must not: saved Markdown format unchanged | e2e B2 asserts `[[target]]` / `[[target|label]]` | pass |
| Must not: break existing specs | runs below | pass (baseline only) |

## Gauntlet (final run after the last code edit)

- Rust: `cargo fmt -p ax-web -- --check` ok; `cargo test -p ax-web --lib --test links_api` → 71 + 8 pass.
- Clippy: **blocked**. `cargo clippy -p ax-web --all-targets -- -D warnings` stops on existing errors in `ax-installer/src/cli_catalog.rs`, `ax-telemetry/src/lib.rs`, `ax-types/src/lib.rs` (none touched here), so it never reaches `ax-web`.
- Types: `npx tsc -b --noEmit` → 0 errors.
- Unit: `node --experimental-strip-types --test src/*.test.ts src/lib/*.test.ts` → 229/229.
- Build: `npm run build` → exit 0.
- e2e: `npx playwright test e2e/link-picker-search.spec.ts e2e/policy-wysiwyg.spec.ts e2e/policy-link-origin.spec.ts e2e/policy-graph.spec.ts e2e/blade-dismiss.spec.ts --project=system-chrome` → 56 passed, 6 failed. The 6 are the known baseline (blade-dismiss on /memory, /logging, /nodes, /unresolved) that fail without these changes too.
- Mutation: `scripts/link-picker-mutants.sh` → 13/13 killed (9 library, 3 UI via e2e on a Vite dev server, 1 server via the Rust test). A first run left 2 survivors (prefix ranking, loose URL check); two unit tests were added for those spec cases, then 13/13.
- Real execution: `scripts/reinstall-cli.sh`, `ax web` restarted, `/` → 200, served bundle `index-D-9c_RYV.js` = `dist/index.html`. The live `/api/links/graph` returns 249 nodes, 241 with tags.
- Supply chain: no new dependencies.

## Problems found on the way

- The picker loaded its targets only when opened, so a quick search followed by Enter landed before they arrived and did nothing. Targets now load when the editor mounts (one cached request per page). e2e B2 covers it and a mutant restoring the old behaviour is killed.
- The chips first used the shared `badge` class, whose icon glyph ended up in each button's accessible name; they now have their own outline style.

## Review rounds

| Round | Skills (status) | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `old-coder-api` usable, `react-review` usable, `typescript-review` usable, `rust-review` usable | 3 minor: R1-1 chip kept between openings (React "State"); R1-2 `filterLinkTargets` only used by tests (dead code); R1-3 `KIND_PREFIX` typed as total `Record` (TS "Unions and narrowing") | R1-1 (RED e2e first), R1-2 (tests call `searchLinkTargets` through a local helper, assertions unchanged), R1-3 |
| 2 | same | 0 | — |

HTTP gates for the `tags` field: Boring ✓, Compatibility ✓ (additive), Authentication/Authorization N/A (local server, unchanged), Idempotency N/A (read), Blast radius ✓ (same query as before), Pagination N/A (existing bounded list), Expensive fields ✓ (tags already loaded), No leakage ✓.

## Known limits

- The 100-row cap counts every hidden match, but you reach them only by refining the search; there is no "show all".
- Focus returns to the editor on close (per spec B4), not to the toolbar button.
- Independent verification not performed (Tier 2).

## Follow-up: picker position

- Bug: the button picker appeared in the bottom right of the blade. It used `position: fixed`, and the blade's transform made it position relative to the blade instead of the viewport.
- Fix: `.wysiwyg-editor` is now `position: relative`. When the picker opens, its anchor is converted to an offset inside the editor, and the picker is placed with `position: absolute`. A portal was rejected: the Rules and Skills pages close the blade on any pointer-down outside the editor's DOM, and a portaled picker is outside it.
- RED: `B1 the picker opens right under the link button` was off by 319.47 px. GREEN: it now passes.
- B4 used to click the middle of the paragraph, which the correctly placed picker now covers. It now clicks the Formatting toolbar, which is still outside the picker. The assertion is unchanged.
- Fresh run: link-picker-search, policy-wysiwyg, policy-link-origin and blade-dismiss all pass, except the 6 blade-dismiss failures from before this work. Unit tests 30/30, tsc clean. The served bundle `index-DdG7KmNS.js` matches `dist/index.html`.
