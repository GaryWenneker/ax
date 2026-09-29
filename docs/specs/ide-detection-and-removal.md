# SPEC — "Found" means installed, and unchecking always removes

Tier 2 bug fix. Two user-visible bugs:

1. After `ax init` with only Cursor checked, the Command Center still showed every other IDE as Connected.
2. IDEs that are not installed showed as Found.

## Root causes
- R1: the old `ax init` connected all 13 targets without asking. The first run with the IDE menu has no saved list, and spec A6 only removed IDEs that were in the saved list, so nothing was removed.
- R2: "found" (`is_detected`) is "the CLI is on PATH, or a config folder exists" (`~/.codex`, `~/.kiro`, `~/.gemini`, `~/.claude.json`, …). ax creates those folders when it writes config, so every IDE ax ever connected looks installed, including after Disconnect.

## Behaviors
- F1 (replaces A6): after an answer (the menu, typed input, or `AX_INIT_IDES`), init removes ax from **every target that is not chosen and still has ax configured**, whether or not it was saved before. It lists the removed files. Targets without ax config are not touched.
- F2: "found" means the program is installed, never that a config folder exists. A target is found when any of these is true:
  - its CLI resolves (the existing `resolve_cli_spawn`: PATH, npm global, `~/.local/bin`, the Cursor agent);
  - an app is installed in a known place for this OS:

| Target | macOS | Windows | Linux |
|---|---|---|---|
| Cursor | `Cursor.app` | `%LOCALAPPDATA%\Programs\cursor\Cursor.exe` | `cursor` on PATH, `/opt/Cursor`, `/usr/share/cursor` |
| VS Code | `Visual Studio Code.app` | `%LOCALAPPDATA%\Programs\Microsoft VS Code\Code.exe`, `%ProgramFiles%\Microsoft VS Code\Code.exe` | `code` on PATH, `/usr/share/code` |
| Windsurf | `Windsurf.app` | `%LOCALAPPDATA%\Programs\Windsurf\Windsurf.exe` | `windsurf` on PATH, `/usr/share/windsurf` |
| Zed | `Zed.app` | `%LOCALAPPDATA%\Programs\Zed\Zed.exe` | `zed` or `zeditor` on PATH |
| Kiro | `Kiro.app` | `%LOCALAPPDATA%\Programs\Kiro\Kiro.exe` | `kiro` on PATH, `/usr/share/kiro` |
| Antigravity | `Antigravity.app` | `%LOCALAPPDATA%\Programs\Antigravity\Antigravity.exe` | `antigravity` on PATH |
| Continue | the `continue.continue-*` extension folder in `~/.vscode/extensions` or `~/.cursor/extensions` | same | same |

  On macOS, `.app` is looked up in `/Applications` and `~/Applications`. The CLI agents (Claude Code, Codex, opencode, Hermes, Gemini, Takumi) count only when their CLI resolves.
- F3: the Command Center list and `ax init` both use the new check, so after Disconnect a target that isn't installed shows **Not found**, not Found.
- F4: "Connected" is unchanged: ax config is present in that target's files.

## Must not
- N1: change what install or uninstall writes.
- N2: change how agent CLIs are resolved for running them (the terminal still uses `resolve_cli_spawn`).

## Tests
- `ide_choice`: `ides_to_remove(configured, chosen)` returns every configured target that isn't chosen. It replaces the saved-list version.
- `targets`: a pure `app_found(target, os, home, env, exists)` checked against the table above with a fake filesystem, for macOS, Windows and Linux, plus "a config folder alone is not found".
- `init_ides`:
  - with Cursor and Codex configured and nothing saved, `AX_INIT_IDES=cursor` removes Codex;
  - with nothing configured, nothing is removed;
  - A6's "only saved" test is replaced, because this spec reverses it.
- Mutation script, full tests, clippy, reinstall, a real check of the status list on this Mac (you have only Cursor installed, per your screenshot), docs, and EVIDENCE.
