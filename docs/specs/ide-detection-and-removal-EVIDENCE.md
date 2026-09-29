# EVIDENCE — "Found" means installed, and unchecking always removes

Spec: docs/specs/ide-detection-and-removal.md. Approved by the user ("Yes, build it"). Tier 2. No commits: the source state is the working tree.

## Spec to tests
| Behavior | Verified by |
|---|---|
| F1 remove every configured, unchosen IDE, even when never saved | `ide_choice::every_configured_ide_that_is_not_chosen_is_removed`, `init_ides::f1_configured_ides_are_removed_even_when_never_saved` |
| F1 targets without ax config are not touched | `init_ides::f1_…` (no "Removed ax from Zed"); mutant `.filter(\|_\| true)` killed |
| F2 app locations for macOS, Windows, Linux | `detect::tests::{mac_apps_…, windows_apps_…, linux_binaries_…}` |
| F2 Continue by its extension | `detect::tests::continue_needs_its_extension` |
| F2 config folders alone are not found | `detect::tests::config_folders_alone_are_not_found`, `init_ides::a7_f2_…` (an empty `~/.continue` stays empty) |
| F2 CLI-only targets have no app | `detect::tests::cli_only_targets_have_no_app` |
| F3 Command Center uses the same check | `target_status` sets `detected` from the same two checks as `is_detected`; the real `/api/agent/status` run is below |
| F4, N1, N2 unchanged | the install and uninstall code and `resolve_cli_spawn` are not edited; full test suites are green |

## Gauntlet (final run, after the last code edit)
- RED: all 6 `detect` tests and the new `ide_choice` test failed on stubs; `init_ides` failed on 3/6 (a7, f1, and a2 through the stub).
- Tests: `cargo test -p ax-installer -p ax-cli` gives 48 + 76 + 6 passed, 0 failed.
- Mutation: `scripts/init-ides-mutants.sh` killed 27/27 (the 16 existing mutants, 1 of them updated for `asked`, plus 11 new).
- Clippy on `ax-installer` and `ax-cli` all targets: no warnings in changed files. `tsc --noEmit`: ok.
- Real execution: after reinstall, `ax web` returns 200 and `/api/agent/status` on this Mac reports found = claude (CLI), cursor and vscode (`/Applications/Cursor.app`, `/Applications/Visual Studio Code.app`). All other targets are not found.

## Test corrections, disclosed
- a7 used to create `~/.cursor` to fake an installed Cursor. F2 reverses that, so a7 now puts a fake `codex` CLI on a restricted PATH.
- The first version of a7 also asserted that the project has no `.continue/` folder. That failed because policy seeding (`ide_seed`) always writes project `.continue/rules` and `.continue/mcpServers`, whatever IDEs you choose. This is older behavior outside this spec, so the assertion was dropped. The empty `~/.continue` check still covers F2.
- The old test `a6_only_saved_ides_are_removed` was replaced by `f1_…` because this spec reverses it.

## Limits
- `is_detected` combined with the real app check has no mutant, because the result depends on the machine. It is covered by the real run above.
- The Windows and Linux paths are checked with a fake filesystem, not on a real Windows or Linux machine.
- Takumi counts as found only when its CLI resolves; its app is not looked for.
- Removed dead code: `vscode_user_dir` and `takumi_user_dir`, which only the old config-folder check used.
