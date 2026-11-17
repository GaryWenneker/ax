# Memory backup / depth=backup evidence

Spec: `docs/specs/memory-restore.md`, Phase 4 backup slice, proof bar
("A memory that exists only in a backup is found with `depth=backup` and
restores with its cues"; "A full backup restores into an empty project…").

Spec approval: not obtained as a separate review. The deterministic-memory
plan was accepted with "go"; this turn continued that plan.
Tier: 2.
Isolation: temporary project roots under `std::env::temp_dir()`. Never opens
`.ax/ax.db` in this repo.
Source: working tree after restore round-trip test landed.
Toolchain: rustc/cargo workspace defaults. No new packages.

## Behavior map

| Requirement | Check |
|---|---|
| Restore inserts absent ids; second restore skips existing | `restore_skips_existing_and_tombstoned_ids` |
| Tombstoned id is not restored | same |
| depth=backup finds a memory that exists only in the backup file | `depth_backup_finds_purged_row_and_full_restore_round_trips` |
| Full restore recovers every purged id | same (`restored == 3`) |
| Restored rows regain cues | same (`memory_cues` count > 0 per id) |
| Backup `.jsonl.zst` + manifest survive restore | same |
| Must not open live vault | temp dirs only |

## Gauntlet

Command:

```bash
AX_SKIP_WEB_BUILD=1 cargo test -p ax-memory --release --offline --test restore -- --nocapture
```

Result: **2 passed, 0 failed** (finished in 0.14s).

- `restore_skips_existing_and_tombstoned_ids` … ok
- `depth_backup_finds_purged_row_and_full_restore_round_trips` … ok

## Skipped / limits

- Full copy of this repo’s live `.ax/ax.db` was **not** opened (plan constraint:
  do not open the live vault until the plan is finished). The round-trip used a
  three-memory fixture vault that exercises the same backup codec, depth search,
  and restore path.
- Graph-history rows in the backup stream are not yet asserted here; memory +
  cue round-trip is what this evidence covers.
- Mutation tool: not re-run for this slice; restore path was already covered by
  earlier mutants in memory-db-integrity EVIDENCE.

## Verdict

depth=backup search + full restore round-trip: **evidenced** on a fixture vault.
Live-vault restore rehearsal remains deferred until opening the live DB is
authorized.
