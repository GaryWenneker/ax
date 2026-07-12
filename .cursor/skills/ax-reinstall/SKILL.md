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
1. cargo build --release -p ax-cli   # source of truth: target-dev/release/ax.exe
2. reinstall (script below)            # MUST — not optional
3. verify ALL ax.exe copies match      # MUST — hash check (see below)
4. ax --version                        # confirm feature set on PATH binary
5. smoke: ax help / changed subcommand # optional quick check
```

## Verify all install locations (ALWAYS)

`cargo install` can leave stale copies. On Windows, check **three** paths — especially `%LOCALAPPDATA%\ax\current\ax.exe` (root), which reinstall scripts used to skip:

```powershell
$src = "target-dev\release\ax.exe"
$targets = @(
  "$env:USERPROFILE\.cargo\bin\ax.exe",
  "$env:LOCALAPPDATA\ax\current\bin\ax.exe",
  "$env:LOCALAPPDATA\ax\current\ax.exe"
)
$hash = (Get-FileHash $src).Hash
foreach ($t in $targets) {
  if (-not (Test-Path $t)) { Write-Warning "missing: $t"; continue }
  if ((Get-FileHash $t).Hash -ne $hash) {
    Get-Process ax -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue
    Copy-Item -Force $src $t
  }
}
# Re-check — throw if still mismatched
foreach ($t in $targets) {
  if ((Test-Path $t) -and (Get-FileHash $t).Hash -ne $hash) { throw "STALE: $t" }
}
Write-Host "OK: $($targets.Count) paths match"
```

Report checked paths + hash prefix in handoff. Do **not** skip this step.

## Reinstall command

**Windows (preferred — kills all ax.exe processes first):**

```powershell
.\scripts\reinstall-cli.ps1
```

**macOS / Linux:**

```bash
bash scripts/reinstall-cli.sh
```

**Manual (any OS):**

```bash
# Scripts call kill-all first; manual equivalent:
pkill -x ax 2>/dev/null || true    # Linux/macOS
# Windows: Get-Process ax | Stop-Process -Force
cargo install --path crates/ax-cli --force
ax --version
which ax   # or: Get-Command ax
```

## Access denied on Windows

If `cargo install` still fails with **Access is denied (os error 5)** after the script runs:

1. Retry `.\scripts\reinstall-cli.ps1` (it kills all `ax.exe` PIDs twice)
2. Close Cursor / MCP panel so no new ax MCP child respawns
3. Retry again

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
