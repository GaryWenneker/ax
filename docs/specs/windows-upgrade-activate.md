# SPEC — Windows upgrade activation

Status: approved ("implement", 2026-10-20)
Tier: 2
Isolation: this working tree (a fresh worktree has no `target-dev` build cache)

## Goal

`ax upgrade` on Windows must finish with the bundle at `%LOCALAPPDATA%\ax\current`, and a failed swap must not destroy that directory or look successful.

## Scenarios

1. Activating a staging directory that replaces an existing `current` leaves the new tree at `current`, removes staging, and removes the parked `current.old`.
2. When `current` is absent, activation renames staging onto `current`.
3. When the rename of staging onto `current` fails after `current` was parked, `current` is restored with its previous contents and staging is left in place.
4. Exactly one `upgrade-staging-*` directory that contains `ax.exe` (in `bin\` or at the root) is a recovery candidate. Zero matches, a directory without `ax.exe`, or two matches are not promoted.
5. The helper executable used to apply the upgrade is a copy outside the staging directory. A launch path inside staging is rejected.
6. An upgrade status of `error` is returned once and then removed. An `ok` status produces no message.
7. Recovery promotes when `current\bin\ax.exe` is missing and the running executable is outside staging. It waits for this process to exit when the running executable is inside staging. It does nothing when `current\bin\ax.exe` already exists. Two staging directories are reported and not promoted.
8. The cmd fallback parks `current` as `current.old` before moving staging, and moves `current.old` back if that move fails. It does not delete `current` first.

## Invariants

- `install.ps1` still installs directly into `current` and still deletes leftover `upgrade-staging-*` directories.
- Hook and MCP paths stay on `current\bin\ax.exe`.
- macOS and Linux still replace the running binary in place.
- No CodeGraph compare-matrix change: CodeGraph has no installer or self-upgrade.

## Setup

- No new dependencies.
- Tests live in `crates/ax-cli/src/commands/upgrade.rs`.
- No checkpoint commits unless asked.
