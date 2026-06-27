---
name: ax-reinstall
description: >-
  Reinstall the ax CLI binary to ~/.cargo/bin after every ax-cli build or code
  change. Use when modifying ax-cli, finishing ax CLI work, after cargo build
  -p ax-cli, when the user says reinstall, new binary, or when ax on PATH is
  stale. Mandatory before telling the user to test ax commands.
---

# ax reinstall — always install the new binary

> **ABSOLUTE**: After any change under `crates/ax-cli/` (or a workspace build of `ax-cli`), **reinstall** before handoff or verification. A debug build in `target/debug/` is not enough — `ax` on PATH must be updated.

## When (always)

- Edited any file in `crates/ax-cli/`
- Ran `cargo build -p ax-cli` or `cargo build --release -p ax-cli`
- Fixed CLI colors, commands, help, or installer behavior
- User will test `ax …` in a terminal or MCP uses the installed binary

## Workflow (end of every ax-cli task)

```text
1. cargo build -p ax-cli          # confirm compile
2. reinstall (script below)         # MUST — not optional
3. ax --version                     # confirm PATH binary updated
4. smoke: ax help                   # optional quick check
```

## Reinstall command

**Windows (preferred — stops daemon first):**

```powershell
.\scripts\reinstall-cli.ps1
```

**macOS / Linux:**

```bash
bash scripts/reinstall-cli.sh
```

**Manual (any OS):**

```bash
ax daemon stop 2>/dev/null || true
cargo install --path crates/ax-cli --force
ax --version
which ax   # or: Get-Command ax
```

## Access denied on Windows

If `cargo install` fails with **Access is denied (os error 5)**:

1. `ax daemon stop`
2. Close terminals/processes holding `ax.exe` (MCP, Cursor agent, watch mode)
3. Retry `.\scripts\reinstall-cli.ps1`

If still locked, report the blocker and give the user the debug path as fallback:

```powershell
c:\gary\ax\target\debug\ax.exe help
```

## Verify PATH

Installed binary must be `~/.cargo/bin/ax` (Windows: `%USERPROFILE%\.cargo\bin\ax.exe`).

```powershell
(Get-Command ax).Source
(Get-Item (Get-Command ax).Source).LastWriteTime
```

Timestamp should match the build you just made.

## Agent response

When reinstall succeeds, tell the user:

- Installed path
- `ax --version` output
- Remind: Cursor sets `NO_COLOR=1` — use `$env:AX_FORCE_COLOR = "1"` to see CLI colors

Do **not** mark ax-cli work complete without a successful reinstall or an explicit blocked reason.
