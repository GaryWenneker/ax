# EVIDENCE: global.db usable from ax

**Spec:** `/Users/gary/io/ax/docs/specs/global-db-usable.md`  
**Approval:** not obtained as a separate review (user asked to fix the gap list)  
**Tier:** 2

## Mapping

| Behavior | Test |
|---|---|
| G1 init tables | `g1_init_creates_tables` |
| G2 sync node | `g2_sync_copies_node` |
| G3 shared knowledge | `g3_shared_knowledge_from_same_file_hash` |
| G4 missing ax.db | `g4_missing_ax_db_errors` |
| G5 dry-run ax.db | `scripts/dry-run-migration.js` prefers `.ax/ax.db` |

## Gauntlet

```
unset CARGO_TARGET_DIR
CARGO_TARGET_DIR=/Users/gary/io/ax/target-dev cargo test -p ax-global-db
```

Result: **4 passed**, 0 failed.

Smoke (after last edit): `target-dev/release/ax` with `AX_GLOBAL_DB=/tmp/ax-global-test.db`:

- `global init` → created db
- `global sync /Users/gary/io/ax` → 6373 nodes, 1658 files, 19423 edges
- `global status` → 1 project, shared 0 (single project)

Layers skipped: mutation tool, Command Center UI (out of spec), PATH shim reinstall (blocked as extra install mutation).

## Limits

No Global/Project/Combined web UI. No watchdog. Existing `~/.ax/global.db` from the old CHECK-constrained schema should be removed before `ax global init`.
