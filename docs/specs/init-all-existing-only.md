# SPEC — init --all refreshes existing ax projects

Status: approved ("doe het", 2026-10-20)
Tier: 2
Isolation: this working tree

## Goal

`ax init --all`, which install and upgrade run, refreshes directories that already contain `.ax/ax.db`. It does not create an ax project in a Git repo or other software project.

## Scenarios

1. A directory with `.ax/ax.db` is selected.
2. A Git repo without `.ax/ax.db` is not selected.
3. A directory with only a project manifest (`Cargo.toml`, `package.json`, `go.mod`) is not selected.
4. The home directory itself is not selected, even when it contains a manifest. A child that already has `.ax/ax.db` is selected.
5. An existing ax project stops the walk, so a nested package inside it is not selected separately.
6. A Git repo that is not an ax project does not stop the walk, so an ax project nested inside it is selected.
7. `ax init` on one directory is unchanged and can still create an ax project.

## Docs

Update the current help text, README, and installation/quickstart/CLI docs that say `--all` initializes every Git repo and manifest.
