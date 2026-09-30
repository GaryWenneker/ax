# Evidence: Command Center live updates

Source: commit `9d12335` on branch `live-updates`. Spec: `docs/specs/live-updates.md` (approved, plus revision 1). Tier 2.

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
| Camera and layout do not move on a graph update | Code: `mergeLive` adds nodes and calls `ensureSimulation(0.25)`; the transform is untouched. Not covered by an automated test |
| No per-page polling | Memory's 5 s timer and the status bar's 30 s timer were removed |
| No new dependencies | `package.json` and `Cargo.toml` unchanged |

## Gauntlet (final run, after the last edit)

| Layer | Command | Result |
|---|---|---|
| Rust tests | `cargo test -p ax-web` | 154 passed, 0 failed |
| Clippy | `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 |
| Web UI tests | `node --test src/lib/*.test.ts` | 142 passed, 0 failed |
| Types and build | `npm run build` (tsc + vite) | exit 0 |
| Mutants (throwaway) | first-load returns all keys; drop the scope check; first-seen topic counts as change; no mtime gate and every tick emits | 4 of 4 killed, source restored |
| Real run | `ax web --port 7070`; served bundle matches `dist/index.html`; `/api/changes` returns `text/event-stream` | pass |

## Known limits

- The graph shows the top N nodes by degree (50 to 600). A new node outside that set is not drawn, so it gets no spark. In the real run, a sync added nodes, but none reached the top 100, so no spark was shown.
- The spark and glow visuals were not captured in a screenshot; the Memory glow was confirmed by a DOM watcher.
- Deleted graph nodes stay drawn until the next full load.
- Spec approval was obtained ("Approved — build it"). Revision 1 was not separately approved.
