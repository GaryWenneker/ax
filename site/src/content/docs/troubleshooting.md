---
title: Troubleshooting
description: Fixes for the most common ax issues.
---

## "ax not initialized"

Run `ax init` in your project directory first.

## Indexing is slow

Large committed directories (mobile apps, vendored SDKs, e2e test trees) bloat the index even when gitignored paths are skipped. Add them to `exclude` in `ax.json` at the project root — see [Configuration](/getting-started/configuration/). Use `--quiet` to reduce CLI output overhead.

## MCP hits `database is locked`

Current builds use SQLite WAL mode via sqlx; concurrent reads should not block writers. If you still see lock errors:

- **Stale lock after a crash** — run `ax unlock`, then retry.
- **Another ax process is indexing** — wait for `ax init` / `ax index` to finish, or stop the other process.
- **Network filesystem** — WAL may not work reliably on SMB/NFS or WSL2 `/mnt`. Move the project (with `.ax/`) to a local disk.

## MCP server not connecting

Your agent starts the server itself. Verify the project is indexed (`ax status`) and re-run `ax install` to rewrite MCP config if needed.

## MCP Logging empty / Quality score stuck

1. Enable **Logging → Verbose MCP logging** (or `AX_MCP_VERBOSE=1`) and reconnect MCP.
2. Confirm today's `<project>/.ax/mcp-verbose-YYYY-MM-DD.log` grows after an agent turn.
3. Open Command Center **Logging**; use the project switcher if you are on the wrong workspace.
4. Install `ax savings hook install` so sessions tag `session=` for tighter `ax mcp audit` correlation.

Full playbook: [MCP Logging & Quality](/guides/mcp-quality/#troubleshooting).

## Missing symbols

The MCP server auto-syncs on save (wait a couple of seconds). Run `ax sync` manually if needed. Check that the file's language is [supported](/reference/languages/) and isn't excluded via `.gitignore`, built-in skip dirs (`node_modules`, `target`, …), or `ax.json` `exclude`.

## `ax upgrade` hangs on "Do you want to continue? [Y/n]"

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
ax upgrade --check
```

Pin and reinstall when you need a specific release:

```bash
# macOS / Linux / WSL2
AX_VERSION=v2.0.14 curl -fsSL https://getax.wenneker.io/install.sh | sh

# Windows (PowerShell)
$env:AX_VERSION = 'v2.0.14'; irm https://getax.wenneker.io/install.ps1 | iex
```

Compare with [latest.txt](https://getax.wenneker.io/releases/latest.txt).

## Existing installations

You already have ax installed but `ax version` shows an old release (e.g. **2.0.10** while [latest.txt](https://getax.wenneker.io/releases/latest.txt) says **v2.0.14**), or `irm …/install.ps1 | iex` prints `Installing ax v2.0.10 — latest available`. This section covers diagnosis and fix.

### Symptoms

| What you see | Likely cause |
|---|---|
| `Installing ax v2.0.10 … latest available` while newer tags exist on GitHub | Stale install script or version resolver skipped newer releases (fixed in install **schema 2** — see below) |
| `ax upgrade --check` shows `2.0.10 → 2.0.14` but install still picks 2.0.10 | Same — asset probe failed on your network; use schema 2 script or pin `AX_VERSION` |
| Multiple paths under “Synced local instances” | Normal on Windows — see [Which binary is active](#which-binary-is-active) |
| `cargo install` build overwritten after `install.ps1` | Installer syncs to `~/.cargo/bin/ax` by default |
| `cargo install` → **Access is denied** replacing `ax.exe` | Cursor MCP / `ax web` respawned during the long rebuild — use `.\scripts\release-local.ps1` (copy-sync; no second cargo install) |

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

1. Close ax MCP, `ax web`, and terminals running ax.
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

On **ax 2.0.0+** (non-interactive, no prompts):

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
# macOS / Linux
curl -fsSL https://getax.wenneker.io/install.sh | sh

# Windows
irm https://getax.wenneker.io/install.ps1 | iex

# npm
npm i -g @garywenneker/ax@latest
```

## Sharing one checkout between Windows and WSL

Don't point both at the same `.ax/` lock and database — SQLite locking across the WSL2/Windows boundary is unreliable. Use separate index dirs per OS if needed.
