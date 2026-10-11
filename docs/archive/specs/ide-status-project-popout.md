# IDE status bar pop-out switches to the workspace project

Tier 2. The Cursor / VS Code status bar item `ax` becomes a pop-out that opens Command Center and switches projects. Pressing it selects the project of that window's workspace, even when localhost already has a different project selected.

## Why

Today `ax` in the status bar runs `ax.openCommandCenter`. That starts `ax web` only when port 7070 is down, and only then passes the workspace folder. When the server is already up, the panel iframes `http://127.0.0.1:7070/?embed=1` and shows whatever project the hub last switched to. A browser tab on project `io` stays on `io` after you open the `ax` workspace and click the button.

`POST /api/workspace/switch` already switches the hub. The extension never calls it.

## Behaviors

### P1 — Click opens a pop-out

Given the status bar item `ax` is visible,
when the user clicks it,
then a QuickPick opens (the pop-out). The editor panel does not open until a row is accepted.

### P2 — Click switches to this workspace

Given Command Center on port 7070 is already serving project `/Users/gary/io/io`,
and this window's workspace project is `/Users/gary/io/ax`,
when the user clicks `ax`,
then the extension `POST`s `/api/workspace/switch` with `{ "path": "/Users/gary/io/ax" }` before the pop-out is shown,
and a following `GET /api/workspace/current` reports `workspace.path` for `/Users/gary/io/ax`.

The switch is the click itself. Dismissing the pop-out with Escape does not revert it.

### P3 — Open Command Center uses the workspace project

Given the pop-out is open after P2,
when the user accepts the first row, `Open Command Center`,
then the embedded panel loads `http://127.0.0.1:{port}/?embed=1` for the project switched in P2.
If that panel was already open, it reloads after the switch.

### P4 — Another row switches, then opens

Given `GET /api/workspace/current` lists another initialized project `Pf_Portal`,
when the user accepts that row,
then the extension `POST`s `/api/workspace/switch` with that project's path,
and then opens (or reloads) the panel.

The workspace row is not repeated in the list. Uninitialized recent projects are omitted. The workspace project stays first only as the `Open Command Center` row.

### P5 — Cold start still selects the workspace

Given nothing is listening on the configured port,
when the user clicks `ax`,
then the extension starts `ax web --port {port} {workspace project path}`,
waits until the port answers,
and then follows P2 (the POST is a no-op when the server already opened that project) and shows the pop-out.

### P6 — Workspace folder resolves to the ax project

Given the window has one workspace folder,
when that folder contains `.ax/ax.db`,
then the switch path is that folder.

Given the active editor file sits in one of several workspace folders,
then the switch path is that folder, not the first folder in the list.

Given the workspace folder is a subdirectory of an initialized project (a parent contains `.ax/ax.db`),
then the switch path is the nearest parent that contains `.ax/ax.db`.

Given no workspace folder is open,
then the pop-out does not switch, and the error is `Open a folder to choose its ax project.`

### P7 — Switch failure leaves the hub alone

Given the switch response is `ok: false` (not initialized, path not allowed, read-only, or the port never answers),
when the user clicked `ax`,
then the hub's current project is unchanged,
the panel does not open,
and the error text from the response (or the start failure) is shown.
The pop-out does not open on this failure.

### P8 — Same project does not reload the hub twice

Given the hub is already on the workspace project,
when the user clicks `ax`,
then the extension still shows the pop-out,
and it does not `POST /switch` for that same path.
Accepting `Open Command Center` opens the panel without a second switch.

## Invariants

- I1. The status bar item stays `$(graph) ax`, right-aligned. Tooltip becomes `Open ax Command Center for this workspace`.
- I2. No new HTTP route. The extension uses `GET /api/workspace/current` and `POST /api/workspace/switch` with body `{ "path": "<absolute path>" }`.
- I3. No new dependency. QuickPick is the VS Code API. Tests stay on `node --test`.
- I4. `commandCenterUrl` stays `http://127.0.0.1:{port}/?embed=1`. The project is chosen by the switch, not a query string.
- I5. JetBrains and Zed are unchanged in this change. The button the user pressed is the VS Code-family status bar item (`extensionKind: ui`, one host per window, so each window uses its own workspace folders).
- I6. A switch updates the shared hub, so a browser tab and any other open panel follow that project. That is the point of P2.
- I7. Existing `ax.openCommandCenter` command id stays. The command opens the pop-out instead of the panel directly.

## Setup

- Isolation: this branch, in the current checkout. A separate worktree would not update the Cursor extension the status bar loads. No checkpoint commits unless you ask for them.
- New dependencies: none.
- Files the implementation will add or edit:
  - `ide/vscode/src/core.ts` — path resolution, switch skip, pop-out rows
  - `ide/vscode/src/extension.ts` — click handler
  - `ide/vscode/test/core.test.ts` — the scenarios above that do not need the VS Code runtime
  - `site/src/content/docs/guides/command-center.md` — status bar pop-out and workspace switch
- Docs: the command center guide only. No CLI or MCP surface change, so README and `reference/cli.md` stay as they are.
- Gauntlet entry point for this package: `npm test` in `ide/vscode` (existing `node --test test/*.test.ts`) and `npx tsc -p . --noEmit`. Frontend production build does not apply: this package's `build` is `tsc`, and the web UI bundle is not part of the change.
- Manual check after the extension is rebuilt and installed: with Command Center on `io`, focus the `ax` window, click the status bar item, confirm the hub label is `ax`, then accept `Open Command Center`.

## Out of scope

- A project browser (creating folders, `ax init` from the pop-out). `needs_init` is an error, not a wizard.
- Changing the Command Center page project modal.
- Installing or bumping the VSIX as part of the spec approval. That happens after the tests are green, in the same implementation, so the status bar you click is the new one.
