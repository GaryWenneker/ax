# WebDAV mount named "ax" from Settings

Status: draft, awaiting approval. Tier 2 (local-only, same-user OS commands).

## Goal

From Command Center → Settings, one click connects the existing WebDAV vault as a
drive/volume named `ax` on macOS, Windows, and Linux, with optional reconnect at login.

## Decisions (from user)

- D1 The vault is also served at `/ax/`; `/dav/` stays as an alias (existing vaults keep working).
- D2 Windows uses the first free drive letter (searching Z: down to D:), never a fixed letter.

## Behaviors

- B1 `PROPFIND /ax/ Depth:1` returns the same listing as `/dav/`, with hrefs under `/ax/`.
- B2 Every existing `/dav/` test still passes unchanged.
- B3 `GET /api/dav/mount` returns `{ os, mounted, mountPoint, url, name, autostart }`.
- B4 `POST /api/dav/mount {name?, autostart?}` connects the vault; `name` defaults to `ax`.
- B5 `DELETE /api/dav/mount` disconnects it and removes the autostart entry.
- B6 `name` must match `^[A-Za-z0-9 _-]{1,32}$`; anything else returns 400 and runs no command.
- B7 Commands are built as argv arrays (no shell). Pure builder functions per OS:
  - macOS: `mkdir -p ~/<name>`, then `mount_webdav -S http://127.0.0.1:<port>/ax/ ~/<name>`;
    autostart = LaunchAgent `~/Library/LaunchAgents/io.getax.dav.plist`.
  - Windows: `net use <L>: \\127.0.0.1@<port>\ax /persistent:yes|no`, where `<L>` is the
    first free letter Z..D; label `<name>` via `HKCU\...\Explorer\MountPoints2\##127.0.0.1@<port>#ax\_LabelFromReg`.
  - Linux: `gio mount dav://127.0.0.1:<port>/ax/`; GTK bookmark line `dav://127.0.0.1:<port>/ax/ <name>`;
    autostart = `~/.config/autostart/ax-dav.desktop`.
- B8 Free-letter picker: given used letters {C,D,Z}, returns Y; given all D..Z used, returns
  an error "no free drive letter" (409) and runs no command.
- B9 Windows: if the `WebClient` service is not running, POST returns 409 with the fix command
  `sc config WebClient start= auto && sc start WebClient`.
- B10 A failing command returns 502 with its stderr and the exact command text to copy.
- B11 Settings card "Vault connection": status, Connect / Disconnect / Open, a name field
  (default `ax`), and an "Connect at login" toggle; on error it shows the copyable command.

## Must NOT

- N1 The mount endpoints are rejected (403) in read-only/share sessions, when the `Host`
  header is not `127.0.0.1`, `localhost`, or `[::1]` (any port), and when an `Origin`
  header is present that is not one of those hosts.
  Revision (during build): the server has no peer address (`axum::serve` without
  `ConnectInfo`), so "non-loopback client" is enforced through Host/Origin, which also
  blocks DNS rebinding and LAN clients of `ax share`.
- N2 No shell string interpolation; the user name never reaches a shell.
- N3 No `sudo`: Linux `davfs2` is only offered as a copyable command.
- N4 The `/dav/` URL and its behaviors do not change.

- Test hook: `AX_DAV_MOUNT_DRY_RUN=1` makes POST/DELETE return the commands without
  running them (files under `$HOME` are still written, so tests isolate `HOME`).

## Setup plan

- Isolation: branch `feat/webdav-mount` (the working tree has many unrelated changes).
- No new dependencies (std::process::Command, existing axum/serde).
- New files: `crates/ax-web/src/dav/mount.rs`, `crates/ax-web/tests/dav_mount.rs`,
  `crates/ax-web/web-ui/src/components/VaultMountCard.tsx` (+ `.test.ts` for helpers).
  Revision (during build): unit tests live in `crates/ax-web/src/dav/mount_tests.rs`; UI
  helpers and API calls in `web-ui/src/vaultMount.ts` + `vaultMount.test.ts` (node:test,
  like the other web-ui unit tests). The "Open" button of B11 is `POST /api/dav/mount/open`.
- Docs: a section in the site guide for the Obsidian vault (docs-with-features).

## Gauntlet

- `cargo test -p ax-web`, clippy, `tsc`, web-ui vitest, manual mutants on the builders + picker.
- Real execution: mount/unmount on macOS via Settings. Windows/Linux: builder tests only;
  a real mount on those OSes is reported as skipped unless a machine is available.
