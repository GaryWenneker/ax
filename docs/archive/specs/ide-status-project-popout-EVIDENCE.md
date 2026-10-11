# Evidence: IDE status bar pop-out

Spec: `/Users/gary/io/ax/docs/specs/ide-status-project-popout.md`

Spec approval: the user replied `approved.` to that spec.

Tier 2. Isolation: this checkout, as the spec said, because the status bar loads the extension installed from here.

Source: git `daf5772` plus uncommitted edits. Not a commit. Dev tools: Node `v26.8.2`, TypeScript `5.9.3`.

## Behavior map

| Spec | Test |
|---|---|
| P1 pop-out rows, panel not opened by the planner | `pop-out lists Open Command Center first…` and `preparePopout` returns rows only |
| P2 click switches off the current hub project before the pop-out | `click switches to the workspace project before the pop-out is ready` |
| P3 Open does not switch again | `accepting Open Command Center does not switch again` |
| P4 another row switches then opens | `accepting another project switches to that path and opens the panel` and the pop-out list test |
| P5 cold start args | `cold start launches ax web with the workspace project` |
| P6 folder, active file, parent project, no folder | the four `workspace path` / `active file` / `nested` / `no workspace folder` tests |
| P7 failures do not return rows | `a failed switch…`, `a server that does not start…`, `click with no folder…`, `click reports a hub that is down…` |
| P8 skip switch when already on the workspace | `click skips switch when the hub is already on this workspace` |
| I1 status bar text and tooltip | `status bar label stays ax and the tooltip names this workspace` |
| I2 existing switch API | `switch project posts the path to the hub` |
| I4 embed URL unchanged | `webview html has csp frame-src for port only` |
| I5 JetBrains and Zed unchanged | no files under `ide/jetbrains` or the Zed task were edited |
| I7 command id stays `ax.openCommandCenter` | `ide/vscode/package.json` command id was not edited |

Escape keeps the switch and does not open the panel. That is `if (!picked) return` in `ide/vscode/src/extension.ts`. Node tests do not run the VS Code QuickPick, so that line was not executed here.

## Gauntlet

Final run after the last edit to `ide/vscode/src/core.ts` (hub fetch timeout).

| Layer | Command | Result |
|---|---|---|
| Tests | `node --experimental-strip-types --test ./test/core.test.ts ./test/package.test.ts` in `ide/vscode` | 24 pass, 0 fail |
| Types | `npx tsc -p . --noEmit` in `ide/vscode` | exit 0 |
| Coverage | same test command with `--experimental-test-coverage --test-coverage-include=src/core.ts --test-coverage-lines=100 --test-coverage-functions=100 --test-coverage-branches=90` | exit 0. `core.ts` lines 100.00%, functions 100.00%, branches 90.29%. Uncovered branches are the Windows case fold, non-Error throws, and the filesystem-root stop |
| Mutation | `node ./test/mutate.mjs` | 5/5 killed: `skip-switch-always`, `keep-uninitialized-rows`, `no-parent-walk`, `open-row-switches`, `dead-hub-looks-successful` |
| Mutation control | `node ./test/mutate.mjs --control` | exit 1. Unmodified source is not counted as a kill |
| Control non-vacuous check | same `--control` with its final `process.exit(1)` changed to `process.exit(0)`, then restored | exit 0. The failure depends on that exit |
| Installer version | `cargo test -p ax-installer extension_version_matches_package_json` | 1 passed |
| Lint | skipped | `ide/vscode` has no ESLint script |
| Property tests | skipped | path choice is covered by the examples above, not a generated property |
| Supply chain | skipped | no dependency added |
| Frontend production build | `npm run build` (`tsc -p .`) inside `npm run package` | exit 0. This package's production build is `tsc`, not a web bundle |
| Order randomization | skipped | this package has no order randomizer. Tests do not share mutable state |

`extension.ts` is not in the coverage include. Node cannot load `vscode`. The planner it calls is `preparePopout`, which the tests and the live hub call both ran.

## Real hub

Command Center was already on `ax`. The check switched it to `/Users/gary/io`, then called `preparePopout` with folder `/Users/gary/io/ax` and the real `fetch`.

Result: `switched: true`, first row `Open Command Center` for `/Users/gary/io/ax`, next row `io`. A following `GET /api/workspace/current` reported `workspace.label` `ax` and path `/Users/gary/io/ax`. The hub was left on `ax`.

## Extension install

`cursor --install-extension ide/vscode/ax-command-center.vsix --force` installed `wenneker.ax-command-center-0.1.1` next to the existing `0.1.0`. The installed `package.json` version is `0.1.1`. The status bar keeps running the old extension until the window reloads.

The bundled installer bytes are updated at `crates/ax-installer/assets/ax-command-center.vsix`. The `ax` binary already on PATH still contains the previous bytes until that crate is rebuilt. Connect sees the `0.1.0` folder and treats the panel as up to date, so it does not overwrite `0.1.1`.

## Review rounds

| Round | Skills | Findings | Fixed |
|---|---|---|---|
| 1 | `old-coder` usable, `typescript-review` usable, `javascript-review` usable | 1 (hub `fetch` had no timeout, javascript-review §5) | R1-1 `AbortSignal.timeout` of 5s |
| 2 | same | 0 | — |

A schema library was not added. The approved spec forbids a new dependency. Hub JSON is checked field by field in `core.ts`.
