# Repair backup uses VACUUM INTO

Spec approval: not obtained (autonomous run). The approved plan plus "continue" authorized this slice. Isolation: the working tree.

Tier 2. This does not open the live project database. The original file is not renamed or deleted.

## Scenarios

1. A repair of a database that has uncheckpointed WAL frames writes one backup file. That file has no `-wal` sidecar. Opening the backup alone returns the row that existed only in the log.
2. A file that is not a database is still copied as bytes. VACUUM INTO is not required to succeed for that file.

## Must not

- Rename or delete the database being repaired.
- Open the live project database.
- Checkpoint the original before the backup exists.

## Evidence

RED: the backup still contained `NEEDLE-VACUUM-UNIQUE` (`VACUUM INTO drops the deleted page`).

Mutant: `backup_database` always returned the byte copy. The test failed with `assertion failed: !sidecar.exists()`. Restored before the final run.

Final, after rustfmt:

- `cargo test -p ax-db --offline --test vacuum_backup --test integrity -- --test-threads=8`
- integrity: 29 passed, 0 failed
- vacuum_backup: 1 passed, 0 failed
- `cargo clippy -p ax-db --offline --all-targets -- -D warnings` exited 0. The workspace warnings for `gix` and `self_update` are pre-existing.

Known limit: a readable database that `VACUUM INTO` rejects for a reason other than damage also falls back to the byte copy. The garbage-file test covers the unreadable path.
