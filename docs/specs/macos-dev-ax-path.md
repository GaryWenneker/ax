# SPEC: macOS PATH always runs the latest local ax build

**Tier:** 2 (dev workflow / PATH; machine-local SIGKILL workaround)
**Spec approval:** requested with this document (user asked for a plan and to update the private MCP rule)

## Problem

On this Mac, `ax` on PATH is `~/.local/bin/ax` (often a symlink or Mach-O copy). Executing a Mach-O from `~/.local/bin` or `~/.cargo/bin` is **SIGKILL (137)**. The same bytes run from `~/io/ax/target-dev/release/ax` and from `/tmp`. `zsh: killed ax web --open` is that kill, not an `ax web` crash.

`cargo install` / copying into home bin recreates the kill. Developers need `ax` from **any cwd** and must always hit the **latest `cargo build --release`** output.

## Source of truth

| Role | Path |
|------|------|
| Latest build | `/Users/gary/io/ax/target-dev/release/ax` (Cargo `target-dir` from `.cargo/config.toml`) |
| PATH entry | `~/.local/bin/ax` — **POSIX shell shim**, never a Mach-O, never a symlink to a home-bin Mach-O |
| Cursor MCP `command` | the same `target-dev/release/ax` Mach-O (stdio spawn; no home-bin Mach-O) |

## Behaviors

### B1 — `ax` from any directory

Given `~/.local/bin` is first on PATH, `cd /tmp && ax --version` exits 0 and prints the same string as `$REPO/target-dev/release/ax --version`.

### B2 — Shim is not a Mach-O

`file ~/.local/bin/ax` matches a shell script / text executable, not `Mach-O`.

### B3 — Shim execs the checkout binary

The shim contains `exec` of `$REPO/target-dev/release/ax` (absolute path). Replacing the checkout binary (rebuild) changes `ax --version` / embedded web UI without copying into `~/.local/bin`.

### B4 — Never copy Mach-O into home bins on this Mac

`scripts/reinstall-cli.sh` on Darwin must **not** `cargo install` and must **not** `cp` a Mach-O onto `~/.local/bin/ax` or `~/.cargo/bin/ax`. It builds into `target-dev` and (re)writes the POSIX shim.

### B5 — Cursor MCP

`~/.cursor/mcp.json` `mcpServers.ax.command` is `$REPO/target-dev/release/ax` with `serve --mcp --path ${workspaceFolder}`. After changing it: **MCP: Restart Servers**.

### B6 — Private rule

`macos-cursor-ax-mcp-binary` documents this layout (not “always launch cargo bin”). Still `scope: private_user`; not git-shared; not seeded into `.agents/`.

## Invariants (must not)

- Do not git-share the private rule or hardcode other users’ home paths into team `.agents/` rules.
- Do not change Windows `reinstall-cli.ps1` three-copy sync.
- Do not require a Nerd Font / p10k fix as part of this spec.

## Setup

- Isolation: none (PATH + MCP + private rule are this machine’s dev loop).
- No new crates/npm dependencies.
- Files: `docs/specs/macos-dev-ax-path.md`, `docs/specs/macos-dev-ax-path-EVIDENCE.md`, `scripts/reinstall-cli.sh`, `tools/gauntlet-macos-dev-ax-path.sh`, `.cursor/skills/ax-reinstall/SKILL.md`; home: `~/.local/bin/ax`, `~/.cursor/mcp.json`, `~/.ax/private_policy/rules/macos-cursor-ax-mcp-binary.mdc`.

### B7 — Never overwrite a Mach-O in place (2026-09-01)

`cp` or `cargo build` onto an existing `target-dev/release/ax` is SIGKILL’d. Same bytes work from a **new inode** (`rm` then `mv`). After every Darwin build, `relink_macos_macho` copies to `ax.new`, deletes `ax`, then moves. Do not `cp` onto a live `ax` path.
