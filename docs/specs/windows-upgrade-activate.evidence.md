# EVIDENCE — Windows upgrade activation

Spec: `docs/specs/windows-upgrade-activate.md` (approved by "implement", 2026-10-20). Tier 2.
Source: HEAD `29c75ddc` plus uncommitted `crates/ax-cli/src/commands/upgrade.rs`.
Spec approval: obtained ("implement").

## Scenario map

| Behavior | Test |
|---|---|
| Replace `current`, drop staging and `current.old` | `activate_replaces_current_and_drops_the_parked_copy` |
| Place staging when `current` is missing | `activate_places_staging_when_current_is_missing` |
| Restore `current` when the staging rename fails | `activate_restores_current_when_the_staging_rename_fails` |
| One staging bundle is the recovery candidate | `one_staging_bundle_is_the_only_recovery_candidate` |
| Two staging bundles are not promoted | `two_staging_bundles_are_not_promoted` |
| Helper copy is outside staging | `helper_copy_lives_outside_staging` |
| Helper inside staging is rejected | `helper_inside_staging_is_rejected` |
| Error status once, ok is silent | `error_status_is_reported_once_and_ok_is_silent` |
| Recovery depends on `current` and the running exe | `recovery_depends_on_current_and_where_the_process_runs` |
| Cmd fallback parks before moving and restores | `cmd_batch_parks_current_before_moving_staging` |
| `bin\ax.exe` is published from the root binary | `publish_bin_exe_copies_the_root_binary_into_bin` |
| A fresh promotion claim blocks a second claim | `a_fresh_promotion_claim_blocks_a_second_claim` |

## Gauntlet

| Layer | Result |
|---|---|
| Tests | `cargo test -p ax-cli --bin ax upgrade::tests` — 13 passed, 0 failed, 116 filtered out |
| rustfmt | `rustfmt --edition 2021 --check crates/ax-cli/src/commands/upgrade.rs` — exit 0 |
| Types | Host compile of the test binary succeeded. `x86_64-pc-windows-msvc` is not installed, so the `cfg(windows)` glue was not typechecked |
| Lint | skipped: `cargo clippy -D warnings` is crate-wide and this change did not run it |
| Coverage | skipped: no changed-line coverage tool was run |
| Mutation | 5/5 manual mutants killed (rollback removed, promote while current exists, helper inside staging, batch restore removed, status file kept) |
| Property tests | skipped: the swap is a directory rename, not a parser |
| Full ax-cli suite | skipped: only `upgrade::tests` was run |
| Supply chain | no dependency changes |

## Review rounds

| Round | Skills | Findings | Fixed |
|---|---|---|---|
| 1 | old-coder usable, rust-review usable | 3 (batch wiped `current.old` with no `current`; explicit upgrade stole an in-flight claim; discarded IO errors) | all three |
| 2 | same | 0 | — |

## Known limits

- A machine already stuck needs one later `ax` start that is not `ax upgrade` / `ax upgrade-apply`. If that process is running from inside `upgrade-staging-*`, a helper moves the folder after that process exits.
- Hook and MCP paths stay on `current\bin\ax.exe`. A `settings.json` path that points at `upgrade-staging-*` stops working once the folder is renamed.
- `install.ps1` is unchanged.
- CodeGraph has no installer. The compare matrix was not changed.
