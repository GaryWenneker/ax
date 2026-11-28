# Second pass: usage and memory

Branch `fix/review2-usage-memory`, from `origin/main` at `09f8d831`. It covers the findings the first pass left open under the worklist keys `usage` and `memory`, the owner decision "usage-006 and usage-015: retention", and the `remember_with_id` flake from "Second-pass follow-ups".

Each finding was checked again against current `main` before it was fixed. RED means the new test was run and failed before the fix.

## Findings

| Id | State | Tests | RED seen |
|---|---|---|---|
| usage-006 | fixed (retention part; the session index part was fixed in pass 1) | `retention::tests::*` (activity and durable cases), `activity::tests::activity_retention_from_ship_toml_prunes_old_events_on_write`, `activity::tests::activity_is_kept_without_a_retention_setting`, `durable::tests::durable_retention_from_ship_toml_prunes_old_entries_on_write`, `durable::tests::durable_entries_are_kept_without_a_retention_setting` | yes (wiring tests failed with the pruning calls disabled) |
| usage-015 | fixed (retention part; the bounded read was fixed in pass 1) | `retention::tests::verbose_log_*`, `mcp_verbose_log::tests::rotation_applies_the_verbose_log_retention_setting`, `mcp_verbose_log::tests::rotation_keeps_old_logs_without_a_retention_setting` | yes (rotation test failed with the pruning call disabled) |
| flake: `remember_with_id` UPDATE without `retry_locked` | fixed | `remember_retry::remember_retries_the_derived_column_update`, `remember_retry::remember_retries_the_tier_stamp_update`, `remember_retry::remember_survives_concurrent_fts_rebuilds` | yes (the first two failed with code 1811, "vtable constructor failed: memories_fts") |
| usage-055 | fixed | `period::tests::a_day_whose_midnight_does_not_exist_starts_at_the_first_valid_time`, `period::tests::an_ambiguous_midnight_takes_the_earliest_instant`, `period::tests::a_custom_range_that_ends_before_it_starts_is_rejected` | yes |
| usage-056 | fixed | `pricing::tests::user_override_comes_from_the_loaded_file_without_rereading_it` | yes |
| usage-057 | fixed | `pricing_sync::tests::coding_agents_honour_the_days_window`, `pricing_sync::tests::catalog_rows_carry_their_benchmark_scores`, `pricing_sync::tests::upsert_and_history` (still checks that OpenRouter wins) | yes for the `days` window; the joined catalog query keeps the same output, so its test is a guard |
| usage-058 | fixed | `reuse_cache::tests::session_context_hashes_a_file_cited_by_many_entries_once`, `reuse_cache::tests::session_context_skips_disk_work_for_index_stale_entries` | yes for the memo (6 reads before, 1 after); the stale-row test already passed because a changed index fingerprint rejects rows before hashing, as the verifier noted |
| usage-059 | fixed | `savings::tests::imported_transcripts_store_the_project_path` (extended with a second import of unchanged files) | yes |
| usage-060 | fixed | `store::tests::usage_db_env_restores_the_variable_when_a_test_panics`, plus the converted savings, pricing_sync, pricing, reuse_cache and mcp_verbose_log tests | no: this is test hygiene. The guard's own test is new, and the check is the full ax-usage suite passing 20 times in a row on the affected modules |
| memory-029 | deferred (remaining part only) | none new | no |

### What changed

- **Retention (usage-006, usage-015).** As the owner decided, the default is still to keep everything. A new `[retention]` section in `.ax/ship.toml` takes `activity_days`, `activity_max_bytes`, `durable_days`, `durable_max_bytes`, `verbose_log_days` and `verbose_log_max_bytes`. A missing key, `0`, or a value that does not parse means no limit, and a `ship.toml` that does not parse deletes nothing. Cleanup runs at the same points as the existing TTL cleanup:
  - activity limits run when the project writes an activity event, and durable limits when it writes a durable entry, at most once every five minutes per process;
  - log limits run when the log rotates to a new day.

  Only the project's own rows are deleted. Unscoped legacy durable conversations, today's log file and an unmigrated legacy log are never touched. Documented in `guides/command-center.md` (History retention), `getting-started/configuration.md`, `guides/mcp-quality.md` and `reference/mcp-server.md`.
- **Flake.** In `remember_with_id`, the derived-column UPDATE and the tier stamp in `brain::stamp_memory` now go through `retry_locked`, like the INSERT before them. The first two tests inject the transient error with a trigger that fails twice and then clears. The third runs 45 remembers on three tasks while another task rebuilds the FTS index in a loop.
- **usage-055.** Day bounds go through a helper that takes the time zone as a parameter. It uses `earliest()` and, when midnight does not exist, the first valid local time. A custom range whose `--to` is before `--from` is now rejected. Documented in `reference/cli.md`.
- **usage-056.** `PricingConfig.user_models` holds the models pricing.toml sets. `user_override_for` matches against that map and no longer reads the disk.
- **usage-057.** The dead AA, benchmark and coding-agent types, parsers and upserts are removed. The catalog reads benchmark scores in one joined query instead of one query per row. `list_coding_agents` uses its `days` window. The comments and the same-day ordering now name OpenRouter. The report's AA count fields stay, always 0, so the web API shape does not change. Documented in `reference/web-api.md`.
- **usage-058.** Rows the index rejects are dropped before any disk work. Within one call, hashes are memoised per file. The disk pass runs in `spawn_blocking`, and `store_reply` and `lookup` move their disk reads there too.
- **usage-059.** A transcript that is not newer than the stored `source_mtime` is skipped before it is read, and the upsert needs a strictly newer mtime. Parsing streams lines in `spawn_blocking`, and one session's events are written in one transaction. Review round 1 changed this: the session row and its events now share one transaction and the upsert accepts an equal mtime again (see below). Documented in `guides/token-savings.md`.
- **usage-060.** `UsageDbEnv` sets `AX_USAGE_DB` under the shared lock and restores it on drop, also on panic. The counterfactual mode and the reuse cap are passed as values in tests. Scratch files use `tempfile`. The pricing tests load built-in defaults from an empty folder.

### Deferred: memory-029 (remaining part)

Pass 1 fixed the user-visible part (empty `search_text`, wrong `trust_level`, one recall-only kind list). Two parts are left:
- One `insert_memory(executor, NewMemory)` that every insert path uses, run in a transaction, so that a failure between INSERT OR IGNORE and the follow-ups cannot leave a row that is never stamped.
- Removing the duplicated `register_cues`/`register_cues_on` and `index_archive_vector`/`index_episode_vector`.

Both need `stamp_memory`, `register_cues`, `index_archive_vector`, `stamp_current_turn` and `link_follows_in_session` made generic over `sqlx::Executor`, across store, brain, turns, sync and capture. That is the L-sized refactor the finding describes, with no change in behavior beyond the crash window.

A cheaper "repair on rerun" check, which treats a row with an empty `search_text` as unstamped, was considered and rejected. It would run `link_follows_in_session` again and could add duplicate links. `folder_sync::is_recall_only` stays as pass 1 left it, because widening it changes what shared export sends.

## Review rounds

| Round | Finding | State | Test | RED seen | Change |
|---|---|---|---|---|---|
| 1 | savings.rs:2256 (medium) import not atomic, never retried; mtime 0 skipped forever | fixed | `savings::tests::a_failed_event_write_is_retried_on_the_next_import`, `savings::tests::a_transcript_without_a_readable_mtime_is_never_skipped` | yes (first); no for the mtime unit test, written with the fix | `import_transcript` writes the session row and its usage events in one transaction; the upsert guard is `>=` again. `file_mtime_ms` returns `Option`; an unknown mtime is always imported and keeps the stored mtime. |
| 1 | savings.rs:2242 (low) project mapping frozen for unchanged transcripts | fixed | `savings::tests::an_unchanged_transcript_moves_to_a_project_found_later` | yes | A skipped transcript's usage events move to the project its folder maps to now (`remap_usage_event_project`). Documented in `guides/token-savings.md`. |
| 1 | durable.rs:102 (low) cleanup ran on read-only actions | fixed | `durable::tests::a_read_only_durable_call_does_not_prune`, `durable::tests::only_writing_durable_actions_trigger_cleanup` | yes (first) | `maybe_prune_durable` runs only when `durable_action_writes(action)`; `read`, `search`, `doc_get`, `task_resume` do not start it. `command-center.md` says so. |
| 1 | retention.rs:82 (low) one invalid key disabled every limit | fixed | `retention::tests::one_invalid_key_leaves_the_other_limits_in_place` | yes | Each key is read from a `toml::Table` on its own; a bad key is ignored with a warning naming it. Module doc and `command-center.md` updated. |
| 1 | period.rs:71 (nit) error named an unpassed `--to` | fixed | `period::tests::a_future_from_without_to_does_not_name_a_to_flag` | yes | Without `--to` the error reads `--from <date> is after today (<today>)`. |

Round 1 commands: `cargo test -p ax-usage` 525 passed, 0 failed, 1 ignored (lib 524 + 1 integration); `cargo clippy -p ax-usage --all-targets -- -D warnings` clean; `cargo fmt --check` clean.

## Commands run

All builds used `CARGO_TARGET_DIR=/Users/gary/io/ax-wt/target-pass2`.

| Command | Result |
|---|---|
| `cargo test -p ax-usage` | 518 passed, 0 failed, 1 ignored |
| `cargo test -p ax-memory` | 253 passed, 0 failed, 3 ignored |
| `cargo clippy -p ax-usage --all-targets -- -D warnings` | clean |
| `cargo clippy -p ax-memory --all-targets -- -D warnings` | clean |
| `cargo fmt --check -p ax-usage -p ax-memory` | clean |
| `cargo test -p ax-memory --test remember_retry`, 20 runs | 20 of 20 passed |
| `cargo test -p ax-usage --lib -- retention activity:: durable:: mcp_verbose_log:: store::`, 20 runs | 20 of 20 passed |
