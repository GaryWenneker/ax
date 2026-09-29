# EVIDENCE — Disconnecting Claude Code sticks, and Takumi removed

Spec: docs/specs/claude-disconnect-sticks.md. Approved as written with D1–D4 ("Yes"). D5 was added from the user's reply ("ik wil overal takumi eruit hebben"), shown as a revised spec, and approved again ("Yes, build it"). Tier 2. No commits: the source state is the working tree.

## Spec to tests
| Behavior | Verified by |
|---|---|
| D1 Disconnect cleans the project `.mcp.json` and keeps other servers | `init_ides::d1_disconnecting_claude_cleans_the_project_mcp_json` (RED: "ax still in the project .mcp.json"); mutant that drops `.mcp.json` from the loop is killed |
| D2 status shows Found after Disconnect | real run: `POST /api/agent/uninstall {"targets":["claude"]}` reported `/Users/gary/io/ax/.mcp.json`; afterwards the file is `{"mcpServers":{}}` and `/api/agent/status` says claude found=true, connected=false |
| D3 pack import refreshes only configured and saved IDEs | `ide_choice::pack_import_only_refreshes_configured_ides` (RED on a stub); 2 mutants killed. The call site in `run_pack_import` has no integration test (see limits) |
| D4 other uninstalls unchanged | only the Claude function changed; the other `uninstall_*` functions are untouched; all suites are green |
| D5 Takumi removed | `ide_choice::takumi_is_not_a_target` (12 targets, `takumi` is an unknown id; RED before removal); `scripts/no-takumi-gate.sh` is clean. Its negative control (a temp file containing "takumi") made the gate fail with exit 1. The real status lists 12 targets with no takumi |
| N1 no file deleted, other servers kept | the `d1` test keeps `"other"`; the real run left `{"mcpServers":{}}` |
| N2 Connect unchanged | install code is untouched except that the Takumi arm is removed |

## Gauntlet (final run, after the last code edit)
- Tests: `cargo test -p ax-installer -p ax-cli -p ax-web -p ax-mcp -p ax-db`, all green except one `ax-mcp` test in the first full run (see below). Totals: ax-installer 49, ax-cli 76 + 7 `init_ides`, ax-mcp 81.
- Mutation: `scripts/init-ides-mutants.sh` killed 30/30. A first run stopped fail-closed on a stale pattern (`uninstall_targets(&dropped)`); the pattern was fixed and the script rerun.
- Clippy (`ax-installer`, `ax-cli`, `ax-web`, all targets): the only hit in touched files is an older `manual_map` in `resolve_from_candidates` (cli_catalog.rs, unchanged code; its line moved because the Takumi entry was removed). `tsc --noEmit`: ok.
- Real execution: reinstall, `ax web` returns 200, and the served bundle `assets/index-DU-f5QAA.js` matches `dist/index.html`. Disconnect results are in the D2 row.

## Flaky test, disclosed
`ax-mcp server::policy_integration::unchanged_catalog_and_memory_titles_are_sent_once_per_session` failed once in the parallel run of five crates. It passed 2/2 alone with my one `ax-mcp` change (a hint string in proxy.rs) reverted. With the change restored it passed 3/3 alone and 3/3 in the full `ax-mcp` suite. It is unrelated to this change.

## Limits
- The `run_pack_import` wiring (status, then `pack_refresh_targets`, then `install_targets`) is tested only through the pure function, not end to end.
- The gate guards the spelling `takumi`/`匠`. `crates/ax-installer/src/ide_choice.rs` is excluded because its test names takumi on purpose, and `docs/specs`, `docs/audits`, `dist` and lockfiles are excluded as history or generated files.
- Existing ax config in Takumi's own files (`.vscode/mcp.json` in Takumi workspaces) is not cleaned up; `ax uninstall --target=vscode` cleans the same file.
- Removed: the embed parameters `?takumi=1` and `?bonzai=1`, and showing the `bonzaicoder` folder as "takumi".
