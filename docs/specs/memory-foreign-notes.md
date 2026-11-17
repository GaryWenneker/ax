# Foreign notes stay out of injection

Spec approval: not obtained (autonomous run). The approved plan plus "continue" authorized this slice. Isolation: the working tree.

Tier 2. This does not open the live project database. Forgetting is a status change. The row stays.

## Scenarios

1. `remember` with source `blossom` or `foreign` stores the row with status `foreign`. A file path or tag whose segment is `blossom` does the same. `recall_for_prompt` does not return that row. A manual note with the same trigger is returned. Selecting the foreign row by id still returns it.
2. A new manual note whose body mentions Blossom stays `active` and is injectable.
3. Migration 33 flags an existing active row whose source, tags, files, title, or body names Blossom, including a row whose source is `mcp`. An existing active row that does not name Blossom stays `active`. Running the migration again does not fail.

## Must not

- Delete the row, its cues, or its embedding.
- Open the live project database.
- Treat a new note as foreign only because the body mentions Blossom.

## Evidence

Recorded after the last code edit. Spec approval: not obtained (autonomous run).

| Scenario | Test |
|---|---|
| 1 and 2 | `a_blossom_note_stays_out_of_preflight_and_remains_stored` |
| 3 | `migration_33_flags_an_existing_blossom_note` |

Final run, after rustfmt and both restored mutants:

- `cargo test -p ax-memory --offline --test foreign -- --test-threads=8`: 2 passed, 0 failed.
- `cargo test -p ax-memory --offline --test brain migration_31_rejects_a_heuristic_candidate_and_confirm_restores_it -- --test-threads=8`: 1 passed, 0 failed.
- `cargo test -p ax-db --offline --test migration_v17 --test migration_v23 -- --test-threads=8`: 5 passed, 0 failed.
- `cargo clippy -p ax-memory -p ax-db --offline --all-targets -- -D warnings`: exit 0. Pre-existing unused workspace `gix` and `self_update` warnings only.

RED, before the implementation: the write-path test listed `Blossom deploy path` among the injected titles, and the migration test left the leaked row `active`.

Mutants, each restored before the final run: `memory_is_foreign` returning false injected `Blossom deploy path`. Removing the title and body clauses from migration 33 left the leaked row `active`.

Known limits:

- The one-time migration flags any active row whose title or body contains the substring `blossom`, including a word such as `blossoming`.
- A new note is foreign only when the source is `blossom` or `foreign`, a tag is `blossom`, or a file path has a `blossom` segment.
- This slice does not open the live project database. The migration runs the next time a new binary opens a database.
