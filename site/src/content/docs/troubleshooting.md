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

## Wrong or old version

```bash
ax version
ax upgrade          # installs latest from getax / GitHub (v2.0.0+)
```

If `ax upgrade` reports up to date but you expect a newer release, check [latest.txt](https://getax.wenneker.io/releases/latest.txt) and reinstall with `AX_VERSION=v2.0.0`.

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
