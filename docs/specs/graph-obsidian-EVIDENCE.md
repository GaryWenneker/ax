# EVIDENCE: Obsidian-style Graph

Spec: `docs/specs/graph-obsidian.md`. Tier 2.
Spec approval: the user approved the plan ("Implement the plan as specified"). The SPEC file itself was written from that plan and was not separately reviewed.
Source state: HEAD `b7caeb1` plus uncommitted working-tree changes (no commits made; repo already had many unrelated uncommitted changes).

## Spec mapping
| Behavior | Test |
|---|---|
| Obsidian defaults | `graphObsidian.test.ts` "has Obsidian defaults" |
| Missing/corrupt storage -> defaults | "falls back to defaults on missing or corrupt storage" |
| Partial merge + round-trip | "merges partial storage over defaults and round-trips" |
| Search / orphans / attachments / existingOnly filters | four `graphFilter` tests |
| Group color: first match, empty never matches | "groupColor: first match wins…" |
| Hover: node + neighbors + incident edges; null empty | two `graphHover` tests |
| Start cluster within radius, deterministic | "seeds every node inside the start cluster" |
| Slider -> force params | "maps sliders to force parameters" |
| Panel sections, persistence, reset, hover | `e2e/graph-obsidian.spec.ts` (3 tests) |
| Must not: existing suite stays green | full `node --test` run, 169/169 |
| Must not: Domain view, search, pan/zoom unchanged | not covered by an automated test; manual check only (see limits) |

## Gauntlet (final fresh run after the last edit)
- Build and embed: `scripts/reinstall-cli.sh`, then `ax web`; served `assets/index-D3xY3BOd.js` = `dist/index.html`.
- Types: `npx tsc --noEmit -p .` produced no errors.
- Unit: `node --test src/*.test.ts src/lib/*.test.ts` gave 169 pass, 0 fail, run twice (node:test has no shuffle option, so order-randomized runs were skipped).
- Mutation: `scripts/graph-obsidian-mutants.sh` killed 9/9 manual mutants. Each mutant is checked with `cmp` to prove it was applied.
- E2E: `npx playwright test e2e/graph-obsidian.spec.ts --project=system-chrome` gave 3 passed. The bundled Chromium is missing in this sandbox, so installed Chrome was used.
- Supply chain: `d3-force@3.0.0` (ISC) plus `@types/d3-force` added; `npm audit --omit=dev` found 0 vulnerabilities.
- Lint: skipped, because web-ui has no ESLint config.
- Coverage on changed lines: skipped, because node:test coverage is not wired up for this project. The pure modules are covered by the tests above; Graph.tsx canvas code is covered only by e2e and the screenshots.
- Real execution: screenshots `docs/specs/graph-obsidian-settled.png` (single cluster) and `graph-obsidian-zoom.png` (zoomed, hover in accent color). They were taken one build before the final build; the only change in between is lower label opacity for dimmed nodes.

## Process notes
- RED was not observed separately. The tests and the modules were written in the same step, and the 9/9 mutant run is the evidence that the tests can fail.
- A mid-task visual check showed separate islands and oversized labels. Fixes: lower repel strength (`-3 * repelForce`), stronger center pull, and labels kept at a constant screen size.

## Revision 1 (user feedback: overlapping labels, thick hover lines, labels too early)
- New spec behavior: `graphLabels.placeLabels` places labels greedily by priority and skips any label that overlaps one already placed (3 tests in `graphObsidian.test.ts`).
- Links are drawn at a constant thin screen width; hover changes only their color.
- Labels are drawn at a fixed 11px in screen space. They fade in later (zoom 1.8 or more at the default threshold). The hovered node and its neighbors always show labels.
- Spacing: repel changed to `-8 * repelForce`, and collide radius to node radius + 14.
- Fresh run after these edits: 172/172 unit tests pass, 12/12 mutants killed, 3/3 e2e tests pass (system-chrome), tsc is clean, and the served bundle `index-DawGdNk8.js` matches `dist`. The screenshots (`graph-obsidian-settled.png`, `-mid.png`, `-zoom.png`) come from this build.

## Revision 2 (labels grow a little with zoom, with a hard cap)
- `labelPx(scale)` returns 11px up to zoom 1, then `11 * scale^0.25`, capped at 15px (test "label size…").
- Fresh run: 173/173 unit tests pass, 14/14 mutants killed (including "cap removed" and "growth too steep"), 3/3 e2e tests pass, tsc is clean, and the served bundle `index-B__KyMjA.js` matches `dist`.

## Revision 3 (clicking a node showed no blade)
- Root cause (it predates this work and is present in HEAD `b7caeb1`): in the multi-project graph, node ids are global (`g…`). The page hid the detail blade for those ids because `/api/node` cannot load them.
- Fix: a blade built from the loaded graph data (kind, file, project, links, and clickable connections that recenter the view), wrapped in `ResizableBlade`.
- RED first: the e2e test "clicking a node opens a detail blade" failed ("element(s) not found"). Its canvas-width check also failed on the first attempt (width 0), because the blade filled the whole width.
- Fresh run: 173/173 unit tests pass, 4/4 e2e tests pass, tsc is clean, and the served bundle `index-mXPt9bo1.js` matches `dist`. Screenshot: `graph-obsidian-blade.png`.

## Revision 4 (settings closed by default, close on outside click)
- The panel starts closed. The Settings button toggles it, and a pointerdown outside `.graph-settings` or the toggle button closes it.
- RED first: the new e2e test "settings start closed…" failed before the change. The other e2e tests now open the panel through the button.
- Fresh run: 173/173 unit tests pass, 5/5 e2e tests pass, tsc is clean, and the served bundle `index-BsZ3Cz-G.js` matches `dist`.

## Revision 5 (pulse ring on the selected node, and a "Show selection" button)
- `selectionPulse(t)` makes a ring that grows outward and fades, looping every 1600 ms (test "selectionPulse…", RED observed first). The canvas redraws every frame while a node is selected.
- "Show selection" centers the view on the selected node (checked in the e2e blade test).
- `draw` now reads the selection from `selectedRef`. Before, the d3 tick handler kept an old `selected` value from the first render.
- Fresh run: 174/174 unit tests pass, 16/16 mutants killed, 5/5 e2e tests pass, tsc is clean, and the served bundle `index-W3z__h0Y.js` matches `dist`. Screenshot: `graph-obsidian-selection.png`.

## Known limits
- Hidden (filtered) nodes are removed from the simulation, and they keep their last positions.
- "Labels" in Obsidian means tags. ax has no tags, so here the toggle means "always show labels".
- The Domain view now also draws in world space: its shapes scale with zoom.
