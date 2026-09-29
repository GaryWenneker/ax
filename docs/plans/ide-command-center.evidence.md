# EVIDENCE — Command Center inside every IDE

Spec: `docs/plans/ide-command-center.md` (approved by the user: "approve", 2026-09-29 15:53; revisions R1–R7 appended there).
Tier: 2. Source state: HEAD `b7caeb1` + uncommitted working tree on `feat/webdav-mount` (no commit made; many pre-existing uncommitted changes from others).
Entry point: `scripts/ide-panel-gauntlet.sh` (needs `ax web` on 7070 built from this tree). Final fresh run: all layers passed, 2026-09-29 ~16:27.

## Behavior → test

| Spec | Verified by |
|---|---|
| A1–A3 loading / error / ready | `ideInstall.test.ts: connectAllText…`; Playwright `shows loading until…`, `a failed status load…` (both RED against the old build first) |
| B1, B4 URL + CSP | `ide/vscode/test/core.test.ts: webview html has csp…`, `…rejects a port…` |
| B3 panel reused | `core.test.ts: panel is reused on second open` |
| B2 starts ax web | not unit-tested (VS Code API glue in `extension.ts`); see limitations |
| C1, C3, C4 install/uninstall via CLI + note | `command_center::vscode_extension_install_runs_cli_with_vsix` |
| C2 CLI resolution | `resolve_vscode_cli_prefers_path_then_app_bundle`; `missing_cli_is_note_not_error` |
| C5 badge | `panelLabel…` unit test; Playwright `shows whether the Command Center panel…`; `agent_install_guard` asserts `panel` false for cursor, null for claude |
| R6 up-to-date skip | `current_panel_version_is_detected`; `extension_version_matches_package_json` |
| D1 detection | `jetbrains_detects_product_config_dirs`; guard test asserts `jetbrains` listed |
| D2/D3 install/uninstall own file only | `jetbrains_install_and_uninstall_touch_only_own_file` |
| D4 plugin | `CommandCenterTest` (3 JUnit tests) |
| E1/E2 Zed task | `zed_task_merge_is_idempotent_and_preserves_existing`, `zed_uninstall_removes_only_ax_task` |
| N1 existing connect unchanged | full `ax-installer` suite, `agent_install_guard`, `init_ides` green; one assertion changed intentionally (R7) |
| N2 read-only cannot install | `agent_install_guard` (403 read-only) |
| N3 no network during install | artifacts `include_bytes!` from `crates/ax-installer/assets/`; installer code has no network calls (review) |
| N4 other extensions/plugins/tasks untouched | JetBrains + Zed tests above; VS Code uses `--uninstall-extension wenneker.ax-command-center` only |

## Gauntlet (final run)

- `cargo test -p ax-installer`: 59 passed, 0 failed.
- `cargo test -p ax-web --test agent_install_guard`: 1 passed. `cargo test -p ax-cli --test init_ides`: 7 passed.
- `cargo clippy -p ax-installer --all-targets`: 0 warnings in changed files (1 pre-existing in `cli_catalog.rs`). Gate negative control: fails on an injected `--> crates/ax-installer/src/targets.rs` line.
- VS Code extension: `node --test` 3/3; `tsc --noEmit` clean. JetBrains: JUnit 3/3 (`gradlew test`).
- web-ui: `node --test ideInstall.test.ts` 5/5; `tsc --noEmit` clean; Playwright `ide-install.spec.ts` 5/5 (system Chrome) against a served bundle equal to `dist/index.html`.
- Mutation (`scripts/ide-panel-mutants.sh`): 7/7 killed, restores verified by sha. Negative control: a harmless mutant is reported SURVIVED, exit 1. Java: throwaway mutant (project dir check) killed, 1 of 3 tests failed.
- Coverage on changed lines: skipped — `cargo-llvm-cov` not installed; mutation is the substitute.
- Real execution: Connect via `POST /api/agent/install` for Cursor and VS Code installed `wenneker.ax-command-center-0.1.0` in `~/.cursor/extensions` and `~/.vscode/extensions`; second Connect "up to date" in 20 ms; `ax web` sends no X-Frame-Options/frame-ancestors; Settings shows Panel badges.
- Supply chain: new dev deps only in `ide/vscode` (`@types/vscode` 1.90.0, `@vscode/vsce` 4.0.0, `typescript` 5.9.3, `@types/node`) and `ide/jetbrains` (Gradle 9.8.0 wrapper, `org.jetbrains.intellij.platform` 2.19.0, JUnit 4.13.2). `npm audit` not run. No new Rust crates. New capability: ax-installer now spawns IDE CLIs and writes into JetBrains `plugins/` and Zed `tasks.json`.

## Known limitations

- Opening the panel inside Cursor/VS Code was not observed by me (needs a window reload); the user must confirm after reload.
- JetBrains, Zed, Windsurf, Kiro, Antigravity are not installed on this Mac: covered by unit tests only.
- No MCP config for JetBrains (R4).
- Suite randomized-order run not done.
- Unrelated, found during the run: `AgentsSettingsSection.tsx` was deleted and `Settings.tsx` edited (16:18) outside this work; the AI Agents section no longer renders.
