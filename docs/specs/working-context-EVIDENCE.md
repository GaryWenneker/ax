# Evidence: working context

Spec: `docs/specs/working-context.md`. Spec approval: not obtained. The request was to add the sound delta; this spec text was not separately approved.

Source: uncommitted changes on `feat/conversation-context-cache` in `/Users/gary/io/ax-ctxcache` (HEAD `be0f133` plus this diff). Not installed into the ax binary on PATH.

## Mapping

| Behavior | Test |
|---|---|
| Same text, same 16-hex hash | `same_text_has_the_same_hash_and_a_different_fact_does_not` |
| Index change marks notes stale and keeps them | `a_changed_index_marks_notes_stale_without_dropping_them` |
| add/get round-trip and dedupe | `add_then_get_round_trips_and_dedupes` |
| Other conversation does not see it | `another_conversation_does_not_see_the_snapshot` |
| Over 800 tokens is rejected, stored snapshot unchanged | `over_limit_add_leaves_the_stored_snapshot_unchanged` |
| 13th entry rejected | `thirteenth_short_fact_is_rejected` |
| Duplicate add does not clear stale | `duplicate_add_does_not_clear_a_stale_fingerprint` |
| compact replaces and confirms the index; missing section rejected | `compact_replaces_everything_and_confirms_the_index` |
| update replaces one section; clear deletes | `update_replaces_one_section_and_clear_removes_the_row` |
| Off switch writes nothing | `off_switch_rejects_and_writes_nothing` |
| Newline and 201-character entry rejected | `newline_and_overlong_entries_are_rejected` |
| `ax_session` is not a graph-reuse tool | `cacheable_tools_are_the_read_only_graph_tools` |
| Seeded rules name it and `seedVersion` is 2 | `every_seeded_surface_names_the_conversation_cache`, `conversation_cache_templates_bump_seed_version`, `sync_fix_brings_pre_cache_projects_the_conversation_cache_text` |
| Advertised in the default tool list | `default_advertises_turn_contract_and_graph_reads`, `every_policy_referenced_tool_is_classified` |
| `ax_context` task input unchanged | no edit to that match arm |
| Five prompts keep one hash until the notes change, then a new index marks that hash stale | `later_prompts_reuse_the_same_content_hash` |
| MCP: record, repeat, different symbol, later hit, stale after edit, compact confirms, second chat starts cold, first chat returns | `later_prompts_pick_up_the_snapshot_and_the_graph_cache` |

## Gauntlet

- `cargo test -p ax-usage --lib -- working_context`: 12 passed, 0 failed (0.40s), after the multi-prompt tests.
- `cargo test -p ax-mcp --lib -- reuse_integration`: 8 passed, 0 failed (3.54s), including the multi-prompt MCP conversation.
- Policy and MCP filters named above: 3 + 3 passed.
- Full suite `cargo test -p ax-usage -p ax-mcp -p ax-policy`: 490 passed, 0 failed.
- Clippy `--all-targets -- -D warnings`: first run failed with 7 `await_holding_lock` errors, because the test guard `env_lock()` held a `std::sync::Mutex` across `.await`. It now uses `tokio::sync::Mutex`. Rerun: 0 warnings. `reuse_integration`: 8 passed in each of 3 runs.
- Coverage, cargo-mutants: not run for this delta.
- Manual mutants, each restored after the suite killed it: stale check inverted; dedupe removed; 800-token cap removed. 3/3 killed (`cargo test -p ax-usage --lib` exit 101 on the targeted test).
- Not run: the L7 live-agent benchmark. It is still blocked because the Claude CLI is not logged in.

## Known gaps

- No cap or eviction across conversations. `mcp_working_context` keeps one row per conversation and project until `clear`, so the table grows with every chat that writes notes.
- If the index cannot be read when `ax_session` runs, the fingerprint is stored empty, and an empty fingerprint is never shown as stale. Preflight skips the block in that case, but a later `get` on a readable index still reports `stale=false`.
- No review loop has run on this delta yet.
