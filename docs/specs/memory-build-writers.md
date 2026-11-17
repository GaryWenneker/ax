# Older builds stop background writers

Spec approval: not obtained (autonomous run). The approved plan plus "continue" authorized this slice. Isolation: the working tree.

Tier 2. This does not open the project database and does not stop a running process.

## Scenarios

1. A stamp with `mtime_ms` 1 is older than a stamp with `mtime_ms` 2. Equal mtimes are the same build. An mtime of 0 is unknown and is not older and not newer.
2. `background_writers_allowed` is false when any registered stamp is newer than this one. It is true when this stamp is the newest or the same mtime.
3. `stale_mcp_lines` for this build and an older MCP pid 42 returns `["MCP pid 42 is an older binary. Restart that MCP server."]`. A newer MCP pid and a web pid are omitted.
4. `register_build` stores a role, pid, version, and mtime. A second registration for the same role and pid replaces the stamp. `writers_allowed` on that database matches scenario 2.
5. The background watch registers its build, skips capture and sync when a newer build is registered, and leaves the process running. `ax doctor` reports the older MCP line and the repair text `Restart the MCP server in Cursor`.

## Must not

- Stop MCP, `ax web`, or the daemon.
- Re-embed or open the live project database from this work.

## Evidence

Recorded after the last code edit. Spec approval: not obtained (autonomous run).

| Scenario | Test |
|---|---|
| 1–4 | `an_older_build_skips_writers_and_doctor_names_the_mcp_pid` |
| 5 | `background_watch_returns_when_a_newer_build_is_registered` asserts `file_contents` stays empty. Doctor joins `stale_mcp_lines` and sets the repair text `Restart the MCP server in Cursor`. |

Review rounds:

| Round | Skills | Findings | Fixed |
|---|---|---|---|
| 1 | old-coder usable, rust-review usable | 3: the watch loop restated the binary every poll, doctor treated a registry read error as no older MCP, and the skip-before-sync branch had no test | all three |
| 2 | same | 0 | — |

Final run, after rustfmt and the restored early return:

- `cargo test -p ax-db --offline --test builds -- --test-threads=8`: 1 passed, 0 failed.
- `cargo test -p ax-core --offline --test build_writers -- --test-threads=8`: 1 passed, 0 failed.
- Schema pin tests in the same session, before this evidence section: migration_v17 4 passed, migration_v18 1, migration_v19 1, migration_v20 1, migration_v23 1.
- `cargo clippy -p ax-db -p ax-core -p ax-cli --offline --all-targets -- -D warnings`: exit 0. Pre-existing warnings only: unused workspace `gix` and `self_update`, unused `windows-sys` in ax-cli.

RED, before the implementation: `an_older_build_skips_writers_and_doctor_names_the_mcp_pid` panicked at `crates/ax-db/tests/builds.rs:20` because `background_writers_allowed` returned true.

Mutant, restored before the final run: deleting the early `return` in `spawn_background_watch_as` failed `background_watch_returns_when_a_newer_build_is_registered` with `left: 1 right: 0`.

Known limits:

- An MCP pid that has exited stays in `ax_builds`. Doctor still names it.
- The doctor arm for a failed registry read is not executed by a test.
- An unknown mtime (`0`) stays allowed to write.
- This slice does not open the live project database and does not stop a process.
