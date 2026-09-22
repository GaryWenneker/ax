# EVIDENCE: Exclusive policy storage CLI

spec: `/Users/gary/io/ax/docs/specs/policy-storage-exclusive.md`
spec approval: not obtained (autonomous run)
tier: 2

## Mapping

| Behavior | Test / check |
|---|---|
| B1 status shows default | pre-existing `ax policy storage status` |
| B2 database exclusive deletes ax-policy files | `remove_ax_policy_files_leaves_cursor_bootstrap` |
| B2 does not delete `.cursor/` | same test |
| B3 `--keep-files` | CLI wiring `keep_files` on `exclusive_to_database` |
| B4 files `--yes` exports all via `export_policy_to_files_filtered(..., false)` to `.agents/` |
| B5 preview without `--yes` | `run_storage_set` returns before writes when `!yes` |

## Gauntlet (fresh)

```
cargo test -p ax-policy migrate
# migrate tests passed (cargo check -p ax-cli ok)
```

`bash tools/gauntlet-policy-storage.sh` — wiring + that unit test.

Skipped: mutation (CLI glue), full ax-cli install until reinstall.

## Known limits

Does not delete `.cursor/` copies even if they were imported. Private/disabled items are exported in files `--yes` (`git_export_only: false`).
