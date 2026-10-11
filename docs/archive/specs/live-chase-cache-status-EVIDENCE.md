# Evidence: live text chase and cache status

Spec: `/Users/gary/io/ax/docs/specs/live-chase-cache-status.md`

spec approval: not obtained (the user asked to implement and ship this behavior in the same message, 2026-10-03)

Tier: 2

Isolation: this working tree (`live-updates`), because the release tags this checkout.

## Behavior map

| Spec | Check |
|---|---|
| New-row class runs a text chase and does not paint a background glow | `liveChase.test.ts` reads the marked CSS block |
| Reduced motion has no animation and no background glow | same checker |
| `ax_cache_status` returns context and file-token counts, no bodies | `cache_status::tests::status_lines_share_a_group_and_omit_bodies` |
| Status is available with no session; session counts appear only for a real group | `status_without_a_session_omits_session_counts` |
| Database and lock failures use fixed tokens | `database_and_lock_errors_use_fixed_tokens` |
| Group key strips unsafe characters and caps length | `group_key_strips_unsafe_characters_and_caps_length` |
| A store line names a valid cache id and refuses a bad id | `store_line_names_the_id_and_refuses_a_bad_id` |
| Context counts do not select bodies | `counts_live_and_expired_without_selecting_bodies` |
| `ax_cache_status` is never stubbed | `exempt_tools_are_the_turn_contract` |
| Logging joins adjacent lines that share `group=` and a lane | `traceGroups.test.ts` |
| A single visible line is not joined | same |
| File-token cache: miss, hit, eviction only on a new key past capacity, missing file changes no counter | tokenizer tests listed below |

## Gauntlet

Final numbers after the last behavior edit (context-cache counts restored onto the unformatted file) and the 6.1.2 version bump. UI sources were unchanged after the Node run, so that run is the UI result.

| Layer | Command | Result |
|---|---|---|
| ax-usage lib | `cargo test -p ax-usage --lib -- --test-threads=8` | 86 passed, 0 failed, 4.16s |
| ax-mcp tool filter | `cargo test -p ax-mcp --lib tool_filter -- --test-threads=8` | 6 passed, 0 failed |
| web-ui tests | `node --test src/*.test.ts src/lib/*.test.ts` in `crates/ax-web/web-ui` | 260 passed, 0 failed |
| types | `npx tsc --noEmit` in `crates/ax-web/web-ui` | exit 0 |
| production build | `npm run build` in `crates/ax-web/web-ui` | exit 0. `index-BqSQemne.js`, `index-Cj4aL5KL.css` |
| mutation | manual, one at a time, restored | 3/3 killed (see below) |
| live-chase negative control | old glow fixture, then the real block with the unprefixed `background-clip` removed | both fail the checker |
| rustfmt whole packages | skipped | `cargo fmt -p ax-usage -p ax-mcp -- --check` already fails on pre-existing files. Formatting `lib.rs` rewrote unrelated modules; those files were restored |
| cargo-mutants / Stryker | skipped | not installed |
| property tests | skipped | grouping and key rules are example tests, not a generator |
| suite order shuffle | skipped | `node --test` has no shuffle flag here; cargo tests ran at `--test-threads=8` and passed once |
| browser chase | skipped | the running Command Center embeds the previous binary. The chase is checked by the CSS contract test, not by a loaded page |
| `ax ship --evaluate` | not run in this evidence window | the connected MCP catalog has no `ax_ship` tool |

## Manual mutants

Each mutant was introduced, the suite was watched fail, then the line was restored.

1. `cache_group_key` always returned `global`. `group_key_strips_unsafe_characters_and_caps_length` failed (left `global`, right `etcpasswd`).
2. `traceGroupLinks` mapped every key to null. The join tests failed.
3. `admit_file_cache` skipped the clear (`if false &&`). `admit_clears_the_map_only_when_a_new_key_exceeds_capacity` expected 2 and got 0.

## Review

Round 1 found two problems and both were fixed: rustfmt had rewritten `server.rs` and `tool_filter.rs` (restored, then the small edits reapplied), and the store log used the file-only session id while status used `active_session_id` (store now uses `active_session_id` too).

Round 2, after restoring the accidental rustfmt on the other ax-usage modules and reapplying the context-cache counts: the focused tests passed. No further code finding.

## Known limits

File-token hit and miss counters are process-global. The hit/miss test asserts deltas. A parallel test that also counts files can move those counters; the full lib suite passed once at 8 threads.

The tool returns the two lines even when verbose logging is off. The Logging page only shows them when the verbose buffer recorded them.

Independent verification: not performed.
