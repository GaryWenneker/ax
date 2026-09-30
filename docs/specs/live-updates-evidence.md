# Evidence: Command Center live updates

Source: branch `live-updates` (see `git log`). Spec: `docs/specs/live-updates.md` (approved; revision 1 disclosed, revisions 2 and 3 approved). Tier 2.

## Spec to test

| Behavior | Verified by |
|---|---|
| A memory write produces exactly one `memory` event | `tests/changes_feed.rs::a_new_memory_produces_one_memory_event` |
| An idle project sends no events for 5 s | `tests/changes_feed.rs::an_idle_project_sends_no_events` |
| Only changed topics are reported; a first-seen topic is not a change | `changes::tests` (3 unit tests) |
| First load and a new scope (filter, page, search, folder) mark nothing new | `live.test.ts`: first load, `trackNewKeys` (2 tests) |
| Change events are parsed and validated; reloads are debounced | `live.test.ts`: `parseChange`, `createDebouncer` |
| Spark cap: at most 20 sparks, a summary for more | `live.test.ts`: `planSparks` |
| Spark has two growing rings and ends after 1.2 s | `live.test.ts`: `sparkPhase` |
| Off-screen node gets an edge marker pointing at it | `live.test.ts`: `edgeMarker` (2 tests) |
| Page refreshes and the new row glows without F5 | Real run on :7070: a memory written from the CLI glowed on the Memory page 2.6 s after the write |
| New nodes beyond the Density limit are added and sparked (rev 2) | Real run: probe file with two functions; graph went from 100 to 109 nodes, two-ring sparks visible, camera unchanged |
| Only truly new nodes, not rewritten ones (rev 3) | `tests/graph_recent.rs` (5 tests) and `node_tracker::tests` (4 tests) |
| Camera and layout do not move on a graph update | Code: `mergeLive` adds nodes and calls `ensureSimulation(0.25)`; the transform is untouched. Not covered by an automated test |
| No per-page polling | Memory's 5 s timer and the status bar's 30 s timer were removed |
| No new dependencies | `package.json` and `Cargo.toml` unchanged |

## Gauntlet (final run, after the last edit)

| Layer | Command | Result |
|---|---|---|
| Rust tests | `cargo test -p ax-web` | 163 passed, 0 failed |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Web UI tests | `node --test src/lib/*.test.ts` | 142 passed, 0 failed |
| Types and build | `npm run build` (tsc + vite) | exit 0 |
| Mutants (throwaway) | first-load returns all keys; drop the scope check; first-seen topic counts as change; no mtime gate and every tick emits; tracker records the first snapshot; tracker records every id | 6 of 6 killed, source restored |
| Real run | `ax web --port 7070`; served bundle matches `dist/index.html`; `/api/changes` returns `text/event-stream`; Memory glow and graph spark seen | pass |

## Known limits

- Deleted graph nodes stay drawn until the next full load.
- Live-added graph nodes stay until the page reloads or Density changes.
- The node tracker lives in memory; restarting `ax web` sets a new baseline.
- In the real run, the first revision 2 attempt flooded the view with about 195 rewritten nodes; revision 3 fixed that.
- The Cursor sandbox sets `CARGO_TARGET_DIR`, so `scripts/reinstall-cli.sh` shipped a stale binary until run with `env -u CARGO_TARGET_DIR`.
