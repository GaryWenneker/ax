# EVIDENCE — Folder picker and IDE install in Settings

- SPEC: `docs/specs/folder-picker-and-ide-install.md` (Tier 3). Approved by the user ("Yes, build it as written"). Revision 1 moved the picker route to `/api/vault/folder-picker`.
- Source state: uncommitted work on top of `b7caeb1`. No commits. Isolation: none (it builds on uncommitted work).

## Behavior → test

| Behavior | Test |
|---|---|
| P1 chosen / cancel / empty / failed dialog | `tests/folder_picker.rs::picker_route_scenarios` |
| P2 dialog per OS | `folder_picker::tests::each_os_gets_its_own_dialog` |
| P3 no dialog → 501 | `picker_route_scenarios` |
| P4 trimming, roots kept | `folder_picker::tests::clean_path_trims_newline_and_trailing_separator` |
| P5 local-only, readonly, one dialog at a time (409) | `picker_route_scenarios` |
| P6 Choose… fills path and name; no-dialog note | `e2e/vault-folders.spec.ts` "Choose… fills…", `vaultMount.test.ts` "a picked path suggests a valid folder name" |
| I1, I2, I5 list, connect, result, state refresh | `e2e/ide-install.spec.ts` (mocked `/api/agent/*`), `ideInstall.test.ts` |
| I2 real install/uninstall writes and removes files | `tests/agent_install_guard.rs` (temp `HOME`; `~/.cursor/mcp.json` created, `results[].files` on uninstall, status flips) |
| I3 reuses the installer | this work edited no file in `crates/ax-installer`; the card calls the existing `/api/agent/*` routes |
| I4 install routes local-only and 403 in readonly | `agent_install_guard.rs` (4 routes × foreign origin and remote host; refused install wrote nothing) |
| N1 | no installer changes |
| N2 | readonly still answers `{ok:false,error}` (now with 403); uninstall keeps `reports` (count) and adds `results`; the agent terminal uses the same origin, so it passes the check |
| N3 | no manifest changes |

## Gauntlet (final run, after the last code edit)

| Layer | Command | Result |
|---|---|---|
| Rust tests | `env -u CARGO_TARGET_DIR cargo test -q --no-fail-fast -p ax-web -p ax-installer` | 183 passed, 0 failed |
| Clippy | `cargo clippy -q -p ax-web --all-targets` | 0 warnings in the changed files |
| Types | `npx tsc --noEmit -p .` | exit 0 |
| Unit (TS) | `node --test src/*.test.ts src/lib/*.test.ts` | 192/192 |
| Mutation | `env -u CARGO_TARGET_DIR bash scripts/picker-ide-mutants.sh` | 16/16 killed. First run 15/16: ignoring a failed dialog's exit code survived. I added the "failed dialog output is ignored" case, and it was killed on the rerun |
| Real execution | `reinstall-cli.sh`, `ax web`; served bundle `index-Cr2Fs8ev.js` = `dist/index.html` | OK |
| E2E | `playwright test vault-folders ide-install memory-files calm-lists --project=system-chrome` | 14 passed |
| Supply chain | no new dependencies | n/a |
| Capability diff | new: ax web can start a dialog process (osascript / powershell / zenity / kdialog) | P5 guards it: local only, not readonly, one at a time; `kill_on_drop` closes it when the request ends |

## Review rounds

| Round | Findings | Resolution |
|---|---|---|
| 1 | 0 in scope. Outside the spec: `GET /api/agent/status` is readable cross-origin like the rest of `/api` and lists config paths | recorded under known limits |

## Known limits

- The real OS dialogs were not clicked in an automated run. Tests use `AX_FOLDER_PICKER_CMD`, so the osascript, PowerShell, zenity and kdialog command lines are only checked by unit tests, not executed. Try **Choose…** once on each OS.
- `GET /api/agent/status` (and other read routes under `/api`) can still be read by other web pages through the permissive CORS layer.
- New targets (VSCodium, VS Code Insiders, JetBrains) and a marketplace extension are out of scope.
- Independent verification: not performed.
