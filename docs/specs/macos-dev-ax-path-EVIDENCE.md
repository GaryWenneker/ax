# EVIDENCE: macOS PATH always runs the latest local ax build

**SPEC:** `/Users/gary/io/ax/docs/specs/macos-dev-ax-path.md`
**spec approval:** not obtained as a separate review (user asked to implement PATH + update the private rule in the same message)
**tier:** 2
**source state:** working tree (uncommitted); gauntlet run 2026-09-01

## Mapping

| Behavior | Check |
|----------|--------|
| B1 `ax` from any cwd | `cd /tmp && ax --version` → `ax 4.6.0` (exit 0); matches `target-dev/release/ax --version` |
| B2 shim not Mach-O | `file ~/.local/bin/ax` → `POSIX shell script text executable` |
| B3 shim execs checkout | shim body `exec "/Users/gary/io/ax/target-dev/release/ax" "$@"` |
| B4 no Darwin cargo install | `scripts/reinstall-cli.sh` Darwin branch is `cargo build` + `install_macos_path_shim`; `cargo install` only in `else` |
| B5 MCP | `~/.cursor/mcp.json` `command` is `/Users/gary/io/ax/target-dev/release/ax` |
| B6 private rule | `/Users/gary/.ax/private_policy/rules/macos-cursor-ax-mcp-binary.mdc` rewritten |

## Gauntlet (fresh after last script chmod)

Command: `bash /Users/gary/io/ax/tools/gauntlet-macos-dev-ax-path.sh`

```
OK: shim=/Users/gary/.local/bin/ax file='POSIX shell script text executable, ASCII text'
OK: ax 4.6.0
OK: MCP command points at target-dev
```

exit 0

Negative control: gauntlet fails if `file` reports Mach-O on the shim (observed earlier: symlink to cargo Mach-O → `zsh: killed`). Checker fail-closed: missing shim, version mismatch, MCP path mismatch.

## Layers skipped

- Full crate test suite: PATH/shim only; no Rust/TS behavior change
- Mutation / coverage: N/A
- `bash scripts/reinstall-cli.sh` full Darwin path: skipped this run (would `cargo build --release` for minutes and is not required to prove the shim already installed)

## Human follow-up (not automated)

Existing zsh sessions: `hash -r`. Cursor: **MCP: Restart Servers** so MCP picks up `mcp.json`.
