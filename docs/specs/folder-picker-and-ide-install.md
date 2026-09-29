# SPEC — Folder picker and IDE install in Settings

Tier 3: both features start processes or write config files on the user's machine from a web page.

## Part A — Native folder picker (Add folder dialog)

- P1: `POST /api/vault/folders/pick` opens the operating system's folder dialog and returns `{ "path": "/abs/dir" }`. Cancelling returns `{ "path": null }`.
- P2: the dialog is chosen per OS:
  - macOS: `osascript` with `choose folder`;
  - Windows: PowerShell with `System.Windows.Forms.FolderBrowserDialog`;
  - Linux: `zenity --file-selection --directory`, or else `kdialog --getexistingdirectory`.
- P3: with no dialog available (for example Linux without zenity or kdialog, or a headless server), the route returns 501 with a message. The UI then says "No folder dialog on this system — type the path" and the path field keeps working.
- P4: dialog output is trimmed: the trailing newline is removed, and the trailing `/` or `\` is removed except for a root such as `/` or `C:\`.
- P5: local-only (loopback host, and a local or absent Origin) and not in read-only mode. Otherwise 403. Only one dialog is open at a time; a second request while one is open returns 409.
- P6: the dialog has a **Choose…** button next to the path field. Picking a folder fills the path, and fills the name with the folder's base name when the name is still empty.
- Test hook: when `AX_FOLDER_PICKER_CMD` is set, the route runs that command instead of the OS dialog. It is only used by tests and is documented as such.

## Part B — IDEs in Settings

- I1: Settings gets an **IDEs & agents** section that lists every installer target (Claude Code, Cursor, Codex CLI, opencode, Hermes, Gemini CLI, Antigravity, Kiro, VS Code, Takumi, Windsurf, Zed, Continue). Each row shows one of: *Connected* (the ax MCP config is present), *Found* (installed, not connected), or *Not found*.
- I2: each row has **Connect** (install the ax MCP config and hooks) or **Disconnect** (uninstall). It shows the result, meaning the files written or removed and any note such as "Reload the VS Code window". There is also **Connect all found**.
- I3: this uses the existing `/api/agent/status`, `/api/agent/install` and `/api/agent/uninstall`, and the existing `ax-installer` code, which already handles macOS, Linux and Windows paths. It adds no new installer logic.
- I4 (security fix): `/api/agent/install`, `/install/stream`, `/cli/install/stream` and `/uninstall` become local-only and refused in read-only mode (403). Today any web page can call them through the permissive CORS layer.
- I5: after connecting, the row changes to *Connected* without a page reload.

## Must not
- N1: change the installer's file formats or what it writes per IDE.
- N2: break the existing agent terminal, which uses the same routes from the same origin.
- N3: add dependencies.

## Out of scope (say if you want them)
- New targets such as VSCodium, VS Code Insiders or JetBrains.
- A marketplace extension, and running this from `install.sh` / `install.ps1`.

## Setup plan
- Isolation: none, in the current tree (it builds on uncommitted work). No commits.
- New files: `crates/ax-web/src/folder_picker.rs`, `crates/ax-web/tests/folder_picker.rs`, `crates/ax-web/tests/agent_install_guard.rs`, `crates/ax-web/web-ui/src/components/IdeInstallCard.tsx`, `crates/ax-web/web-ui/src/ideInstall.ts` and its test, `crates/ax-web/web-ui/e2e/ide-install.spec.ts`, `scripts/picker-ide-mutants.sh`, and the EVIDENCE file.
- Real install and uninstall are tested only in Rust with a temporary `HOME`. The e2e test runs against your live `ax web`, so it only checks the list and intercepts the Connect request in the browser. It never touches your real IDE configs.
- The gauntlet is the same as for vault folders: tests, clippy, tsc, node tests, mutants, rebuild with a bundle check, e2e, docs and EVIDENCE.

## Revision 1 (during implementation)
- P1 path: the route is `POST /api/vault/folder-picker`, not `/api/vault/folders/pick`. A static `pick` segment next to `/{name}` would make a folder named "pick" impossible to delete.
