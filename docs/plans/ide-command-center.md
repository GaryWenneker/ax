# SPEC — Command Center inside every IDE

Status: approved ("approve", 2026-09-29 15:53)
Tier: 2 (writes into the user's IDE installs; reversible via Disconnect)

## Goal

From Settings → IDEs & agents, one click on **Connect** makes ax fully present in that IDE:
the MCP server and hooks (already done today) **plus** the Command Center (`ax web`) inside the IDE.

## Scope per IDE

| IDE | How the Command Center appears |
|---|---|
| Cursor, VS Code, Windsurf, Antigravity, Kiro (VS Code family) | ax extension: panel "ax Command Center" showing `http://127.0.0.1:<port>/?embed=1`, plus a status-bar button |
| JetBrains (IntelliJ, Rider, WebStorm, PyCharm, GoLand, …) — **new target** | ax plugin: tool window "ax" showing the same page (JCEF browser) |
| Zed | No web panels in Zed. A task "ax: Open Command Center" in `~/.config/zed/tasks.json` opens it in the browser |
| Claude Code, Codex, Gemini CLI, opencode, Hermes, Continue | Out of scope: terminal agents / an extension living inside another IDE. MCP connect stays as it is |

## Behaviors

### A. Loading state for the card
- A1: While `/api/agent/status` is loading, the card shows `Loading IDEs…` and **not** "Every IDE found here is connected"; "Connect all found" is disabled.
- A2: After loading, the current text and list appear unchanged.
- A3: If loading fails, the error is shown and no "all connected" text.

### B. VS Code-family extension
- B1: Command `ax: Open Command Center` opens a webview panel with an iframe to `http://127.0.0.1:<port>/?embed=1`; port from setting `ax.webPort` (default 7070).
- B2: If port does not answer, the extension starts `ax web` in the first workspace folder (hidden terminal), waits max. 15 s, then loads the page; otherwise it shows "ax web did not start: <reason>" with a Retry button.
- B3: Status bar button `ax` opens the same panel; a second click reuses the panel instead of opening a new one.
- B4: The webview CSP only allows frame `http://127.0.0.1:<port>`; no other remote origins.

### C. Installing the extension from ax
- C1: Connect on a VS Code-family IDE runs `<cli> --install-extension <temp>/ax-command-center.vsix --force`. The `.vsix` is embedded in the ax binary.
- C2: CLI resolution: PATH (`code`, `cursor`, `windsurf`, `antigravity`, `kiro`) → macOS app bundle (`/Applications/<App>.app/Contents/Resources/app/bin/<cli>`) → Windows `%LOCALAPPDATA%\Programs\<App>\bin\<cli>.cmd`. None found → report note "Extension not installed: <IDE> CLI not found", MCP connect still succeeds.
- C3: Disconnect runs `<cli> --uninstall-extension wenneker.ax-command-center`; a failure becomes a note, not an error.
- C4: The Connect report lists the extension line: `Extension: ax Command Center <version> installed`.
- C5: The card row shows a second badge `Panel` / `No panel` so you can see whether the Command Center is present.

### D. JetBrains
- D1: New installer target `jetbrains`, detected via config folders (`~/Library/Application Support/JetBrains/<Product><version>/`, Linux `~/.config/JetBrains/…`, Windows `%APPDATA%\JetBrains\…`).
- D2: Connect unpacks the embedded plugin into `<configdir>/plugins/ax-command-center/` for **every** product found, writes MCP config where that product supports it, and reports "Restart <product>".
- D3: Disconnect removes only that `ax-command-center` folder.
- D4: Plugin: tool window "ax" with JBCefBrowser on the same URL; if JCEF is unavailable, a button "Open in browser".

### E. Zed
- E1: Connect adds task `ax: Open Command Center` to `~/.config/zed/tasks.json` (merge; existing tasks and comments stay intact; running twice gives one task).
- E2: Disconnect removes only that task.

## Invariants (must NOT)
- N1: Existing MCP/hook connect for all 12 targets stays the same (existing tests stay green).
- N2: Read-only / shared `ax web` cannot install anything (existing `agent_install_guard` test + new routes in it).
- N3: No network access during install; everything comes from the binary.
- N4: Never touch other extensions/plugins or user tasks.

## Tests (names)
- web-ui vitest: `IdeInstallCard shows loading until status resolves`, `…shows error without all-connected text`
- Playwright `ide-install.spec.ts`: delayed status → loading visible → list visible; badge `Panel`.
- Rust `ax-installer`: `resolve_vscode_cli_prefers_path_then_app_bundle`, `vscode_extension_install_runs_cli_with_vsix` (process runner as injected boundary), `missing_cli_is_note_not_error`, `jetbrains_detects_product_config_dirs`, `jetbrains_install_and_uninstall_touch_only_own_folder`, `zed_task_merge_is_idempotent_and_preserves_existing`, `zed_uninstall_removes_only_ax_task`.
- Extension (node:test): `webview html has csp frame-src for port only`, `panel is reused on second open`.
- ax-web: `agent_install_guard` extended; report contains extension line.

## Gauntlet
cargo test (ax-installer, ax-web) · clippy · tsc + eslint + vitest + playwright (web-ui) · tsc + node:test (extension) · changed-line coverage (cargo-llvm-cov if present, otherwise recorded as skipped) · 4–5 manual mutants in CLI resolution/Zed merge/loading state · **real execution**: Connect from the web UI to Cursor and VS Code on this Mac, open panel, screenshot. JetBrains/Zed/Windsurf/Kiro/Antigravity are not installed here → real execution only via unit tests + the built plugin zip; recorded as a limitation.

## Setup plan
- New folders: `ide/vscode/` (extension, TypeScript) and `ide/jetbrains/` (Kotlin plugin, Gradle wrapper).
- Script `scripts/build-ide-plugins.sh` builds both and places `ax-command-center.vsix` and `ax-command-center-jetbrains.zip` in `crates/ax-installer/assets/` (committed, like `web-ui/dist`); `include_bytes!` in ax-installer.
- New dependencies:
  - `@types/vscode` (dev) — types for the extension API.
  - `@vscode/vsce` (dev) — the official packager for `.vsix`.
  - `typescript` (dev) — already present in the repo.
  - Gradle wrapper 8.x + `org.jetbrains.intellij.platform` Gradle plugin 2.x + Kotlin JVM plugin — the only supported way to build a JetBrains plugin. The JDK 21 is already installed. Requires network (Gradle/JetBrains repos) once during the build.
  - No new Rust crates.
- Isolation: **in the current working tree on `feat/webdav-mount`**. A worktree would miss your many uncommitted changes (including `IdeInstallCard.tsx` itself, which is untracked). No commits without your go-ahead.
- After every web-ui/CLI change: web rebuild + reinstall of the ax binary and verification that the served bundle matches.

## Revisions during implementation (append-only)

- R1 (tests A): the web-ui has no vitest; unit tests use `node:test` (`src/ideInstall.test.ts`), the UI is covered by Playwright. Behavior unchanged.
- R2 (D4): the JetBrains plugin is written in Java instead of Kotlin — one dependency less (no Kotlin Gradle plugin).
- R3 (D2/D3): the plugin is a single jar `plugins/ax-command-center.jar` instead of a folder; removal deletes only that file. No zip crate needed.
- R4 (D2): ax writes no MCP config for JetBrains; the location differs per product/AI Assistant version. Known limitation.
- R5 (new): `AX_NO_IDE_PANEL=1` skips extension/plugin/task. Needed because tests with a temp HOME would otherwise reach the real IDE CLI in `/Applications`.
- R6 (new): when the current extension version is already installed, Connect does not start the IDE CLI ("Command Center panel up to date."); policy pack import refreshes IDEs silently and must stay fast.
- R7 (N1): `ide_choice::takumi_is_not_a_target` pinned 12 targets; with D1 this becomes 13, plus an explicit `jetbrains` check.
