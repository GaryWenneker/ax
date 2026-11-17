# Build output is noise

Spec approval: not obtained (autonomous run). The approved plan plus "continue" authorized this slice. Isolation: the working tree.

Tier 2. This does not open the live project database. Rows are demoted, not deleted.

## Scenarios

1. A path is build output when any segment is `target`, `target-dev`, `node_modules`, or `dist`. `crates/ax-memory/src/brain.rs` is not. `target-design/note.rs` is not.
2. Migration 34 sets an existing active row to status `noise`, tier `t3`, and enabled `0` when its files or body name one of those directories. A source row stays `active` and enabled. Running the migration again does not fail.
3. Archive search does not return the noise row. `get` by id still returns it.
4. A later graph capture writes an episode for a source file and writes none for a build-output file, including when that symbol disappears.

## Must not

- Delete the row, its cues, or its embedding.
- Open the live project database.
- Demote a path whose name only begins with `target`.

## Evidence

Recorded after the last code edit. Spec approval: not obtained (autonomous run).

| Scenario | Test |
|---|---|
| 2, 3, and the `target-design` case in 1 | `migration_34_demotes_build_output_and_keeps_it_fetchable` |
| 4 | `graph_capture_skips_build_output` |

Final run, after rustfmt and both restored mutants:

- `cargo test -p ax-memory --offline --test build_output -- --test-threads=8`: 2 passed, 0 failed.
- `cargo test -p ax-memory --offline --test brain migration_31_rejects_a_heuristic_candidate_and_confirm_restores_it`: 1 passed, 0 failed.
- `cargo test -p ax-db --offline --test migration_v23`: 1 passed, 0 failed.
- `cargo clippy -p ax-memory -p ax-db --offline --all-targets -- -D warnings`: exit 0.

RED, before the implementation: the migration left `("active", "t3", 1)`, and graph capture wrote 2 build-output rows.

Mutants, each restored before the final run: `is_build_output_path` returning false wrote 2 build-output rows (`left: 2 right: 0`). Removing the `target-dev` clauses left the row `active`.

Known limits:

- A backslash path is in the migration text and is not covered by the fixture, which uses forward slashes.
- Noise rows stay in the table with `enabled = 0`. Search skips them because it requires `enabled = 1`.
- This slice does not open the live project database. Migration 34 runs the next time a new binary opens a database.
