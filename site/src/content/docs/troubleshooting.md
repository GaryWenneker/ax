---
title: Troubleshooting
description: Fixes for the most common ax issues.
---

## "ax not initialized"

Run `ax init` in your project directory first.

## Indexing is slow

Large committed directories (mobile apps, vendored SDKs, e2e test trees) bloat the index even when gitignored paths are skipped. Add them to `exclude` in `ax.json` at the project root — see [Configuration](/getting-started/configuration/). Use `--quiet` to reduce CLI output overhead.

## MCP hits `database is locked` / agents go DEGRADED

Cursor reports **DEGRADED** when the `user-ax` MCP stdio process fails tool discovery or returns `SQLITE_BUSY` (`database is locked`, code 5). This is common when **two IDEs both spawn `ax serve --mcp` against the same project** (same `.ax/ax.db`) without the shared daemon.

ax hardens this by default:

- WAL mode + **180s** SQLite `busy_timeout` (override with `AX_DB_BUSY_TIMEOUT_SECS`)
- The shared usage database (`~/.ax/usage.db`) waits at most **10s** for a busy writer or a free connection, so a contended file fails the usage write fast instead of stalling a tool call (override with `AX_USAGE_DB_BUSY_TIMEOUT_SECS`, 1 to 600)
- Writers **wait up to 180s** for `.ax/ax.lock` instead of failing immediately
- Stale `ax.lock` / dead daemon PIDs are cleared on open and on `ax daemon restart`
- Maintenance uses `wal_checkpoint(PASSIVE)` so optimize does not starve readers

Healthy path: each IDE attaches as a thin **stdio proxy** to one **shared per-project daemon**. If the daemon fails to start within ~10s when the IDE first connects, ax falls back to an **embedded** MCP engine inside each client — multiple writers → locks → DEGRADED. Once attached, a proxy reconnects by itself when the daemon restarts.

Mitigations:

1. **Reload the shared daemon** (preferred):

```bash
ax daemon restart
```

2. Proxies reconnect on their own within 15 seconds. If the IDE still shows DEGRADED after that, run **MCP: Restart Servers** (or reload the window).
3. **Stale lock after a crash** — `ax unlock` clears the lock. When a live process still holds it, only that process is stopped, and only if it is an ax process (SIGTERM first, SIGKILL after 5 seconds). Other ax processes keep running.
4. Avoid starting a second long `ax index` / `ax init` while one is already running (the second will wait, then proceed).
5. **Network filesystem** — WAL may not work reliably on SMB/NFS or WSL2 `/mnt`. Keep the project (with `.ax/`) on a local disk.
6. Prefer **one primary IDE MCP client** for a workspace when the daemon keeps failing; search today's `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` for `attached to ax daemon`.

## MCP server not connecting

Your agent starts the server itself (proxy → shared daemon). Verify the project is indexed (`ax status`), check `ax daemon status`, run `ax daemon restart`, and re-run `ax install` to rewrite MCP config if needed.

## MCP Logging empty / Quality score stuck

1. Logging is always enabled. Confirm the MCP project path and reconnect MCP.
2. Confirm today's `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` grows after an agent turn.
3. Read the log file directly, and check that it is the file of the project the agent is connected to: the log is per project. For a remote Ax, the **Logging** page of [ax-admin](/guides/remote-admin/) lists the MCP calls that environment handled (it needs the admin grant).
4. Install `ax savings hook install` so sessions tag `session=` for tighter `ax mcp audit` correlation.

Full playbook: [MCP Logging & Quality](/guides/mcp-quality/#troubleshooting).

## Logging shows only Internal (0 inbound / 0 outbound) when using the CLI

Domain rows such as **Internal · CLI** or **Internal · Policy** are expected for lifecycle detail. **Inbound** and **Outbound** appear when the CLI emits `ToolTrace` lines (`inbound tool=ax_…` / `outbound tool=ax_…`), which shipped in **ax 8.4.4**.

1. Check `ax --version` — upgrade if below 8.4.4. Headless ax: run `install-slim.sh` again. Full ax: `ax upgrade`, or re-run `install.ps1` / `install.sh`.
2. In the project that Logging is watching, run for example `ax explore "auth"` or `ax policy match "deploy"`.
3. Look at the log again — today's `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` should now hold `inbound tool=` and `outbound tool=` lines for that command.

See [Directions: Inbound, Outbound, Internal](/guides/mcp-quality/#directions-inbound-outbound-internal).

## Grep or Read was denied

The read-guard hook blocks the first whole-file Read of an indexed source file, and the first Grep whose pattern names a symbol the graph already has. A pattern that joins several names with `|` is the same kind of search: each name is checked, and a hit is denied once. The message lists the graph hits and the ax call to use (`ax_search`, `ax_node`, `ax_callers`, `ax_callees`, `ax_impact`).

A phrase, a CSS class, a regex, or a name that is not in the graph is not blocked. Repeating the same search is allowed, so an edit that needs the raw text can proceed. Set `AX_READ_GUARD=off` to disable the hook.

## Missing symbols

The MCP server auto-syncs on save (wait a couple of seconds). Run `ax sync` manually if needed. Check that the file's language is [supported](/reference/languages/) and isn't excluded via `.gitignore`, built-in skip dirs (`node_modules`, `target`, …), or `ax.json` `exclude`.

## `ax upgrade` hangs on "Do you want to continue? [Y/n]"

This applies to the full ax. The headless build has no `ax upgrade`: the command exits with a message, and you upgrade by running `install-slim.sh` again.

You are on **ax 0.1.x** installed via `cargo install`. That version uses the `self_update` crate, which waits for keyboard input and looks stuck on `Checking for updates…`.

**Fix:** install the current release non-interactively (no prompt):

```powershell
# Windows
irm https://getax.wenneker.io/install.ps1 | iex
```

```bash
# macOS / Linux / WSL2
curl -fsSL https://getax.wenneker.io/install.sh | sh
```

Open a **new terminal** so PATH picks up `%LOCALAPPDATA%\ax\current\bin` (Windows) or `~/.local/bin` (Unix). Then `ax version` should show **2.0.0+** and `ax upgrade` runs without prompts.

See [Existing installations](/troubleshooting/#existing-installations) if you still get an old version after reinstall.

## Wrong or old version

See **[Existing installations](#existing-installations)** below for the full upgrade path (multiple `ax` copies, stale installers, PATH).

Quick check:

```bash
ax version
ax upgrade --check      # full ax only
```

For headless ax, compare `ax version` with [latest.txt](https://getax.wenneker.io/releases/latest.txt) and run `install-slim.sh` again to upgrade; `--version vX.Y.Z` (or `AX_VERSION`) pins a release. See [Headless Linux build](/guides/headless-linux/#install).

For the full ax, pin and reinstall when you need a specific release:

```bash
# macOS / Linux / WSL2
AX_VERSION=v2.0.14 curl -fsSL https://getax.wenneker.io/install.sh | sh

# Windows (PowerShell)
$env:AX_VERSION = 'v2.0.14'; irm https://getax.wenneker.io/install.ps1 | iex
```

Compare with [latest.txt](https://getax.wenneker.io/releases/latest.txt).

## Existing installations

You already have ax installed but `ax version` shows an old release (e.g. **2.0.10** while [latest.txt](https://getax.wenneker.io/releases/latest.txt) says **v2.0.14**), or `irm …/install.ps1 | iex` prints `Installing ax v2.0.10 — latest available`. This section covers diagnosis and fix. It describes the full ax from `install.sh` / `install.ps1`. Headless ax is one binary in `/usr/local/bin` or `~/.local/bin`; `which -a ax` shows which copy runs, and running `install-slim.sh` again replaces it.

### Symptoms

| What you see | Likely cause |
|---|---|
| `Installing ax v2.0.10 … latest available` while newer tags exist on GitHub | Stale install script or version resolver skipped newer releases (fixed in install **schema 2** — see below) |
| `ax upgrade --check` shows `2.0.10 → 2.0.14` but install still picks 2.0.10 | Same — asset probe failed on your network; use schema 2 script or pin `AX_VERSION` |
| Multiple paths under “Synced local instances” | Normal on Windows — see [Which binary is active](#which-binary-is-active) |
| `cargo install` build overwritten after `install.ps1` | Installer syncs to `~/.cargo/bin/ax` by default |
| `cargo install` → **Access is denied** replacing `ax.exe` | Cursor MCP (or `ax web` in the full ax) respawned during the long rebuild — use `.\scripts\release-local.ps1` (copy-sync; no second cargo install) |

### Which binary is active

ax can exist in more than one place. **The first match on `PATH` wins** in the current shell.

**Windows** (typical layout after `install.ps1`):

| Path | Role |
|---|---|
| `%LOCALAPPDATA%\ax\current\bin\ax.exe` | Canonical install (user PATH entry) |
| `%LOCALAPPDATA%\ax\current\ax.exe` | Copy synced by installer |
| `%USERPROFILE%\.cargo\bin\ax.exe` | Also synced unless you opt out |

```powershell
Get-Command ax -All | Format-Table Source
ax version
```

**macOS / Linux / WSL2:**

```bash
which -a ax
ax version
```

Open a **new terminal** after install so PATH picks up the updated entry (`%LOCALAPPDATA%\ax\current\bin` on Windows, `~/.local/bin` on Unix).

### Fix: reinstall latest (recommended)

1. Close ax MCP, terminals running ax, and `ax web` if it is running.
2. Confirm you fetch the **current** install script (schema 2):

```powershell
# Windows — first line should mention "resolver schema: 2"
(irm https://getax.wenneker.io/install.ps1).Split("`n")[0..2]
```

```bash
# macOS / Linux / WSL2
curl -fsSL https://getax.wenneker.io/install.sh | head -n 3
```

3. Reinstall:

```powershell
# Windows
irm https://getax.wenneker.io/install.ps1 | iex
```

```bash
# macOS / Linux / WSL2
curl -fsSL https://getax.wenneker.io/install.sh | sh
```

4. New terminal → `ax version` should match [latest.txt](https://getax.wenneker.io/releases/latest.txt).

### Fix: pin a version

When GitHub API is rate-limited or you need a specific tag:

```powershell
# Windows
$env:AX_VERSION = 'v2.0.14'
irm https://getax.wenneker.io/install.ps1 | iex
```

```bash
# macOS / Linux / WSL2
AX_VERSION=v2.0.14 curl -fsSL https://getax.wenneker.io/install.sh | sh
```

### Fix: upgrade in place (`ax upgrade`)

Full ax only. On **ax 2.0.0+** (non-interactive, no prompts):

```bash
ax upgrade              # latest installable release
ax upgrade --check      # compare only
ax upgrade v2.0.14      # pin
```

If `ax upgrade` refuses to downgrade, you are already on a newer build than the resolver found — run `ax version` and compare with [latest.txt](https://getax.wenneker.io/releases/latest.txt).

Legacy **0.1.x** from `cargo install` may hang on prompts — use the install scripts above instead of old `ax upgrade`.

### Developers: keep `cargo install` binary

`install.ps1` copies the downloaded release to `~/.cargo/bin/ax.exe` so all local copies match. To **avoid overwriting** a dev build from `cargo install --path crates/ax-cli`:

```powershell
$env:AX_KEEP_CARGO_BIN = '1'
irm https://getax.wenneker.io/install.ps1 | iex
```

Use `.\scripts\release-local.ps1` (build + copy-sync) or `.\scripts\reinstall-cli.ps1` (copy-sync only) for ax repo development — these avoid a second `cargo install` that often hits **Access is denied** when Cursor MCP respawns `ax.exe`. Use `install.ps1` for the released binary under `%LOCALAPPDATA%\ax`.

### Still stuck?

1. Check [latest.txt](https://getax.wenneker.io/releases/latest.txt) and [GitHub Releases](https://github.com/GaryWenneker/ax/releases/latest).
2. Pin with `AX_VERSION` (see above).
3. From source: `cargo install --git https://github.com/GaryWenneker/ax ax-cli --force`
4. Report version + `Get-Command ax -All` (Windows) or `which -a ax` (Unix) when opening an issue.

## Reinstall the CLI

```bash
# Linux, headless ax
curl -fsSL https://getax.wenneker.io/install-slim.sh | sh

# macOS / Linux, full ax
curl -fsSL https://getax.wenneker.io/install.sh | sh

# Windows, full ax
irm https://getax.wenneker.io/install.ps1 | iex

# npm, full ax
npm i -g @garywenneker/ax@latest
```

`install-slim.sh` refuses to replace a full ax at the same path unless you pass `--force`. Reinstall `ax-admin` with its own installer; see [ax-admin](/guides/remote-admin/#install).

## Sharing one checkout between Windows and WSL

Don't point both at the same `.ax/` lock and database — SQLite locking across the WSL2/Windows boundary is unreliable. Use separate index dirs per OS if needed.


## My rules/skills were overwritten after reinstall

Install, init, and `ax policy sync --fix` keep a deleted rule or skill deleted. A file that is still on disk is not rewritten. `ax policy restore-item <id>` or `ax policy sync --fix --force` puts one item back. `ax policy history` shows the record.

If an older build rewrote your files, restore them from backup or git, then upgrade. Do **not** pass `--force` unless you want templates to replace local edits on purpose.
