## Evidence Report — database integrity check and repair (Tier 3)

- Spec approval: not obtained (autonomous run). The plan was started with "go". `docs/specs/memory-db-integrity.md` was not reviewed on its own before the code. Confidence is lower because of that.
- Source state: git `85fa176a9b4b6ce5e33687bee268767d87731c60`, plus uncommitted files. The gauntlet prints the hashes below and exits if a mutant is not restored. `crates/ax-db/src/integrity.rs` after the run: `90acfb09491e0752f6eb6099aa4d381bb25296245b06f512e482ff992f8c48f2`.
- Toolchain: rustc 1.100.0 (0ef1bc386 2026-11-09), cargo 1.100.0 (3acc79c0c 2026-11-05), cargo-llvm-cov 0.9.1. No new dependencies.
- Entry point: `./scripts/check-memory-db-integrity.sh` (sha256 `b2d1eae97158b3daf3efd8e16453a6eafb567f14a98b443e43307cf338e3d227`). That script does not run coverage. Coverage was a separate command on the same `integrity.rs` hash, after the script printed `GAUNTLET_OK`.
- Independent verification: not performed against this source state. Tier 3 downgrade. No fresh-context verifier ran.
- Isolation: none. The change is uncommitted on the working tree. A worktree was not used.

Hashes printed at the start of the passing run:

| File | sha256 |
|---|---|
| crates/ax-db/src/integrity.rs | 90acfb09491e0752f6eb6099aa4d381bb25296245b06f512e482ff992f8c48f2 |
| crates/ax-db/tests/integrity.rs | 49577ac622cd29443de5971d2e7964913b2649781aa89f95e52291aff5dcdca3 |
| crates/ax-cli/src/commands/db.rs | a28ab52fb2359e534bc2d91e799038c6a41d4a95301b69382faaeef78564d4a6 |
| crates/ax-cli/src/main.rs | 2f8eacb2429f7637feef209eb535f5de8118bdf4e9f4a94ce3e02409f789309a |
| crates/ax-memory/tests/vector_repair.rs | ac0a6c34858eeea9d9936e98f6296fc862d63f7a9200bd75055fa4cacef2eaa2 |
| docs/specs/memory-db-integrity.md | 37bccc3be2c672484ad7b70231410a7746307ec62b8934031514573c14eceafb |

### Spec → Test mapping

| Scenario | Test | Status |
|---|---|---|
| 1. Healthy database, repair writes nothing | `crates/ax-db/tests/integrity.rs::a_healthy_database_checks_clean_and_repair_writes_nothing` | pass |
| 2. Missing database is not created | `a_missing_database_is_not_created` | pass |
| 3. Garbage bytes are copied, original stays, `--salvage` records `salvage_refused` | `garbage_bytes_are_copied_and_the_original_stays` | pass |
| 4. Orphan cue is deleted, memory and backup stay | `an_orphan_cue_is_removed_and_the_memory_and_backup_stay` | pass |
| 5. Broken full-text index is rebuilt | `a_broken_full_text_index_is_rebuilt` | pass |
| 6. Missing `idx_memory_cues_cue` is created | `a_missing_cue_index_is_created` | pass |
| 7. Foreign-key row is reported and left | `a_foreign_key_row_is_reported_and_left_in_place` | pass |
| 8. Behind schema is migrated | `a_behind_schema_is_migrated` | pass |
| 9. Dangling vector-map row is deleted | `a_dangling_vector_map_row_is_deleted` | pass |
| 10. Foreign embedding model is reported | `a_foreign_embedding_model_is_reported` | pass |
| 11. Write-ahead log with frames is checkpointed | `a_write_ahead_log_with_frames_is_checkpointed` | pass |
| 12. Dry-run lists the cue repair and writes nothing | `dry_run_lists_the_cue_repair_and_writes_nothing` | pass |
| 13. `BEGIN EXCLUSIVE` leaves the cue | `a_locked_database_is_left_unchanged` | pass |
| 14. Backup directory that is not a directory leaves the cue | `a_blocked_backup_directory_leaves_the_cue` | pass |
| 15. Plain SQLite file with no memory tables passes | `a_plain_sqlite_file_passes` | pass |
| 16. Incident file names `ax db repair` | `an_incident_file_names_the_repair_command` | pass |
| 17. `reembed_stale` rewrites `other-model` | `crates/ax-memory/tests/vector_repair.rs::reembed_stale_rewrites_a_foreign_model` | pass |
| 18. `rebuild_vector_index` replaces a dangling map after a backup | `rebuild_vector_index_replaces_a_dangling_map_after_backup` | pass |
| 19. Orphan link is deleted, a link between existing memories stays, backup keeps the orphan | `crates/ax-db/tests/integrity.rs::an_orphan_link_is_removed_and_the_kept_link_stays` | pass |
| 20. Vector-map row for a missing memory is deleted, the row for an existing memory stays | `a_vector_row_for_a_missing_memory_is_deleted` | pass |
| 21. Dangling edge, file contents, and unresolved ref are deleted; kept rows and the backup stay | `dangling_graph_rows_are_removed_and_the_kept_rows_stay` | pass |
| 22. A short embedding is reported, the memory stays, follow-up is `reembed` | `a_short_embedding_is_reported_and_the_memory_stays` | pass |
| Must NOT: delete a memory while removing an orphan cue | scenario 4 | pass |
| Must NOT: delete a foreign-key row or back that file up | scenario 7 | pass |
| Must NOT: replace a file that is not a database | scenario 3 | pass |
| Must NOT: write a backup on dry-run or while a writer holds the lock | scenarios 12 and 13 | pass |
| Safety sentence: repair removes orphaned links | scenario 19 | pass |
| Safety sentence: a vector-map row whose memory is gone is finding `vector_orphan_memory` | scenario 20 | pass |
| `ax doctor` quick check | compiled into `ax-cli`. The gauntlet did not run `ax doctor` | unverified |
| CLI follow-up `follow_memory` (re-embed then rebuild vectors) | the two library functions are tested. The CLI function was not run against a database that needed them | unverified |

### Revision 2026-11-14 — scenarios 21 and 22

The table above this heading is the earlier gauntlet. It does not include these two tests. After the last edit of `integrity.rs`:

- `cargo test -p ax-db --offline -- --test-threads=8`: 45 passed, 0 failed, 0 ignored.
- `cargo clippy -p ax-db --all-targets --offline -- -D warnings`: exit 0. Workspace warnings for unused `gix` and `self_update` remain.
- `rustfmt --edition 2021 --check` on `integrity.rs` and `tests/integrity.rs`: `FMT_OK`.

Mutants, restored before that suite:

| Mutant | Result |
|---|---|
| `DELETE FROM edges WHERE 0` | remaining `["foreign_key", "orphan_edges"]` |
| `DELETE FROM file_contents WHERE 0` | remaining `["orphan_file_contents"]` |
| length compare `!= -1` | subject `"2"`; the test requires `"1"` so the 1024-byte row is not counted |

No anchors table exists in `schema.sql`. Torn-page salvage was not added. Coverage with a fail-under gate was not re-run, so the 558/153 figure above does not cover these lines.

### Gauntlet (final fresh run)

The script `./scripts/check-memory-db-integrity.sh` exited 0 and printed `GAUNTLET_OK`.

| Layer | Command | Result |
|---|---|---|
| Tests | inside the script: `cargo test -p ax-db --offline -- --test-threads=8` then `cargo test -p ax-memory --test vector_repair --offline -- --test-threads=4` | ax-db: 41 passed, 0 failed (16 lib, 16 integrity, 4 migration_v17, 1 migration_v18, 1 migration_v19, 1 migration_v20, 1 migration_v23, 1 source_store, 0 doc-tests). vector_repair: 2 passed, 0 failed |
| Types | `cargo build -p ax-cli --offline` inside the script | exit 0. Linker warning `__eh_frame section too large` on `ax-cli`. Pre-existing unused-dependency warning for `windows-sys` |
| Lint | `cargo clippy -p ax-db --tests --offline -- -W clippy::all` and `rustfmt --edition 2021 --check` on the four new Rust files | clippy exit 0. It printed two pre-existing workspace warnings (`gix`, `self_update`). No warning named `integrity.rs`. rustfmt --check printed nothing and the script continued |
| Changed-line coverage | `cargo llvm-cov -p ax-db --test integrity --offline --text -- --test-threads=8` | not a pass. In `integrity.rs`: 558 executable lines ran, 153 did not. The command exited 0 because no fail-under threshold was set. ax-cli and ax-memory were not in this run |
| Mutation | the script, three hand-written mutants, restored before the suite | 3/3 killed. Orphan `DELETE` replaced with `SELECT 1` → `an_orphan_cue_is_removed_and_the_memory_and_backup_stay` failed with remaining `orphan_cues`. FTS rebuild replaced with `SELECT 1` → `a_broken_full_text_index_is_rebuilt` failed with `database disk image is malformed`. Missing-file report replaced with an empty report → `a_missing_database_is_not_created` failed, left `[]`, right `["missing"]`. Source hash after restore matched the hash at the start |
| Property-based | none | not run |
| Real execution | the script builds `target-dev/debug/ax` and runs it on a temp directory | `db check --help` and `db repair --help` exit 0. Missing database: JSON finding `missing`, `CHECK_EXIT:1`. Dry-run repair: `dry_run: true`, `backup: null`, `REPAIR_EXIT:1`, no backup files. `--db nope`: `BAD_DB_EXIT:1` and `db must be project, usage, global, or all`. With `.ax` present, `integrity.json` has `"repair": "ax db repair"` |
| Supply chain | no dependency change | new dependencies: none. No audit command was run |
| Suite health | one run, `--test-threads=8` | not randomized |

The installed `ax` on `PATH` was not replaced. The binary this run built is `target-dev/debug/ax`.

### Revision 2026-11-14 — scenarios 21 and 22

Command after the last edit of `integrity.rs`: `cargo test -p ax-db --offline -- --test-threads=8`

Result: 45 passed, 0 failed, 0 ignored (16 lib, 20 integrity, 4 migration_v17, 1 migration_v18, 1 migration_v19, 1 migration_v20, 1 migration_v23, 1 source_store_coverage).

`cargo clippy -p ax-db --all-targets --offline -- -D warnings` exited 0. Workspace warnings for unused `gix` and `self_update` remain.

`rustfmt --edition 2021 --check` on `crates/ax-db/src/integrity.rs` and `crates/ax-db/tests/integrity.rs` printed `FMT_OK`.

| Scenario | Test |
|---|---|
| 21 dangling edge, file contents, unresolved ref | `dangling_graph_rows_are_removed_and_the_kept_rows_stay` |
| 22 short embedding stays, follow-up `reembed` | `a_short_embedding_is_reported_and_the_memory_stays` |

Mutants, restored before the suite above:

| Mutant | Result |
|---|---|
| `DELETE FROM edges WHERE 0` | remaining findings `["foreign_key", "orphan_edges"]` |
| `DELETE FROM file_contents WHERE 0` | remaining findings `["orphan_file_contents"]` |
| length compare `!= -1` | subject was `"2"` (the 1024-byte row was counted); expected `"1"` |

No anchors table exists in `schema.sql`, so scenario 21 does not scan one. Torn-page salvage was not added. Coverage with a fail-under gate was not re-run. The earlier coverage numbers in this file do not include these two tests.

### Revision 2026-11-14 — scenario 23

A `writers_active` detail names pid and command from `lsof -F pc` when that command sees a holder. An empty holder list leaves the sqlite message unchanged. `--stop-writers` still only stops the project daemon.

`ax-utils` is already an `ax-db` dependency. It is also a dev-dependency so the integrity test can read the same holder list. No new crate was added.

Commands after the last edit:

- `cargo clippy -p ax-utils -p ax-db --all-targets --offline -- -D warnings`: exit 0. Pre-existing unused `gix`, `self_update`, and `ax-utils`'s unused `anyhow` warnings remain. They are cargo manifest warnings, not rustc warnings denied by `-D warnings`.
- `cargo test -p ax-utils --offline -- --test-threads=8`: 16 passed, 0 failed (15 lib, 1 `home_dir`, 0 doc-tests).
- `cargo test -p ax-db --offline -- --test-threads=8`: 45 passed, 0 failed, 0 ignored (16 lib, 20 integrity, 4 migration_v17, 1 each of v18, v19, v20, v23, source_store_coverage, 0 doc-tests).
- `rustfmt --edition 2021 --check` on `crates/ax-utils/src/process.rs`, `crates/ax-db/src/integrity.rs`, and `crates/ax-db/tests/integrity.rs`: `FMT_OK`.

| Scenario | Test |
|---|---|
| 23 lsof fields, once per pid, skip 0, ignore a command with no pid | `process::tests::lsof_fields_list_each_holder_once` |
| 23 append `holders: 7 ax-mcp, 9` | `process::tests::holders_are_appended_to_a_lock_message` |
| 23 empty holder list leaves the sqlite message | `process::tests::an_empty_holder_list_leaves_the_lock_message` |
| 23 a held lock stays, and the detail names this process when lsof sees it | `a_locked_database_is_left_unchanged` |

On this machine the locked test's detail was `database is locked; holders: <pid> integrity-…`. The assertion compares that detail with a second `lsof` of the same file while the exclusive lock is still held.

Mutants, restored before the suite above:

| Mutant | Result |
|---|---|
| `c` field ignored, command left empty | `lsof_fields_list_each_holder_once` left commands `""` |
| `with_holders` always returned the sqlite message | `holders_are_appended_to_a_lock_message` left `database is locked` |
| repair pushed the sqlite message without `with_holders` | `a_locked_database_is_left_unchanged` panicked on `database is locked` with no `holders:` |

Coverage with a fail-under gate was not re-run. The installed `ax` binary was not replaced. This slice does not stop MCP or `ax web`.

## Independent verification

- Verifier: none. Fresh context: no. Rounds: 0.
- Not performed. This is a declared downgrade for a Tier 3 change (the repair can delete rows).

### Layers not run as specified

- **N-A:** property-based tests. The checks are fixed SQLite fixtures, not a generative parser.
- **UNAVAILABLE:** none for the tools that were required and present. A syntax-tree mutation tool (cargo-mutants or equivalent) was not used.
- **SUBSTITUTED:** mutation was three source edits in the gauntlet script, not a compiler plugin. A kill means the named test failed with the expected text. It does not mean every other test would have failed. Randomized suite order was not run; one threaded run cannot show order dependence. Coverage was a report (`--text`, exit 0) and not a fail-under gate. `cargo clippy -W clippy::all` does not deny warnings, so the two workspace warnings did not fail the script.

### Dismissed review findings

- none — every finding was fixed or accepted as a known limit

### Structural blind spot

- The suite never runs against the live project database, `~/.ax/usage.db`, or `~/.ax/global.db`. Nothing here is evidence about those files.
- `--stop-writers` was not executed. The script does not start or stop a daemon.
- Page-level salvage is refused on purpose. The tests show the original bytes stay. They do not show recovery of a torn page.

### Honest notes

- The first implementation and the tests were written in the same stretch. RED was not a stub that failed before any repair code existed. Later failures that were fixed before this run: a garbage file was reported as more than one `not_a_database`; a full-text delete was invisible until `rank=1`; a foreign-key insert needed `PRAGMA foreign_keys = OFF` on the same connection; a writable check truncated a copied write-ahead log; a lock was discovered after a backup had already been considered. The final code returns one `not_a_database`, checks through a read-only pool when the log has frames, and probes `BEGIN IMMEDIATE` before copying.
- The gauntlet script's first run exited 1 with `mutant did not reach the expected failure` because `grep` treated `["missing"]` as a character class. The test had failed. The script did not count that as a kill. `grep -F` then recognized the same failure. That is the checker's negative control: an unrecognized failure is not a kill.
- `record_incident` does not create the parent directory. A check of a missing database whose `.ax` directory is absent writes no `integrity.json` (the CLI ignores that error). The gauntlet creates `.ax` first and then the file is written.
- Repair statements autocommit. They are not one transaction per action.
- Scenarios 19 and 20 were added after the first report marked those safety sentences unverified. `cargo test -p ax-db --test integrity --offline -- --test-threads=8` then reported 18 passed, 0 failed. Replacing the link `DELETE` with `SELECT 1` made `an_orphan_link_is_removed_and_the_kept_link_stays` fail with remaining `orphan_links`. Replacing the vector-map `DELETE` for a missing memory with `SELECT 1` made `a_vector_row_for_a_missing_memory_is_deleted` fail with remaining `vector_orphan_memory`. Both edits were restored. `integrity.rs` sha256 stayed `90acfb09491e0752f6eb6099aa4d381bb25296245b06f512e482ff992f8c48f2`.
- Scenarios 19 and 20 passed in that 18-test run. The coverage number above was measured before those two tests existed, so it does not include them.
- Index progress now names markdown docs, contracts, SQLite maintenance, graph episodes, activity engrams, memory consolidation, and memory re-embed as separate steps. `cargo test -p ax-core --test sync --offline -- --test-threads=8` reported 8 passed, 0 failed, including `index_names_each_maintenance_step`. Before the change that test saw only `SQLite maintenance`. Graph capture is still one write per node.
- Brain-memory finish spec remains 23/23. This integrity spec's 20 numbered scenarios passed.
