---
title: Installation
description: Install ax v2.0.0 and configure your AI coding agents.
---

## Current version

**Latest release: v2.0.0** — install scripts and `ax upgrade` resolve the tag from [getax.wenneker.io/releases/latest.txt](https://getax.wenneker.io/releases/latest.txt). Check your install:

```bash
ax version
# ax 2.0.0
```

Pin a specific release with `AX_VERSION=v2.0.0` when running `install.sh` / `install.ps1`.

## 1. Install the CLI

ax is a **native Rust binary** — no Node.js required for normal use.

```bash
# macOS / Linux
curl -fsSL https://getax.wenneker.io/install.sh | sh

# Windows (PowerShell)
irm https://getax.wenneker.io/install.ps1 | iex
```

**From source** (requires Rust 1.75+):

```bash
cargo install --git https://github.com/GaryWenneker/ax ax-cli --force
```

**Via npm** (optional — downloads the matching release binary for your OS):

```bash
npx @garywenneker/ax
# or globally:
npm install -g @garywenneker/ax
```

The npm package is a thin launcher: it fetches the prebuilt `ax` binary from [getax.wenneker.io](https://getax.wenneker.io/releases/) (public CDN) with GitHub Releases as fallback. See [npm README](https://github.com/GaryWenneker/ax/blob/main/docs/npm/README.md) for publish details.

## 2. Wire up your agent(s)

```bash
ax install
```

The installer:

- Detects **Claude Code**, **Cursor**, **Codex CLI**, **opencode**, **Hermes Agent**, **Gemini CLI**, **Antigravity IDE**, and **Kiro**.
- Writes each agent's MCP config (`ax serve --mcp`).
- Adds a marker-fenced ax section to agent instruction files where applicable (`CLAUDE.md` / `AGENTS.md` / `GEMINI.md`). Removed cleanly by `ax uninstall`.
- Creates `~/.ax/config.json` with an empty scaffold for [global index defaults](/getting-started/configuration/#global-config-axconfigjson) if the file doesn't exist yet.

The installer **connects agents only — it does not index your code.** Run `ax init` per project (step 4).

### Non-interactive (scripting / CI)

```bash
ax install --yes                              # auto-detect agents
ax install --target=cursor,claude --yes       # explicit target list
ax install --target=auto --location=local     # detected agents, project-local
ax install --print-config codex               # print snippet, no file writes
```

| Flag | Values | Default |
|---|---|---|
| `--target` | `auto`, `all`, `none`, or csv (`claude,cursor,…`) | prompt |
| `--location` | `global`, `local` | prompt |
| `--yes` | (boolean) | prompt every step |
| `--no-permissions` | (boolean) skip Claude auto-allow list | permissions on |
| `--print-config <id>` | dump snippet for one agent and exit | — |

## 3. Restart your agent

Restart your agent so the MCP server config loads.

## 4. Initialize projects

```bash
cd your-project
ax init
```

Creates `.ax/` (SQLite index + lock file) and runs a full index in one step. **v2.0.0+** also scaffolds `.ax/policy/rules/` and `.ax/policy/skills/` for the [policy engine](/guides/policy-engine/).

## Supported platforms

Every release ships **six** prebuilt binaries. Install scripts and npm pick the match for your OS and CPU.

| Platform | Architectures | Install |
|---|---|---|
| Windows | x64, arm64 | PowerShell installer (`install.ps1`), npm, or [getax CDN](https://getax.wenneker.io/releases/) |
| macOS | x64 (Intel), arm64 (Apple Silicon) | shell installer (`install.sh`), npm, or getax CDN |
| Linux | x64, arm64 | shell installer, npm, or getax CDN |
| WSL2 | x64, arm64 | **Use the Linux installer** inside WSL — `curl -fsSL https://getax.wenneker.io/install.sh \| sh` |

WSL2 notes:

- Run `install.sh` from a WSL shell (Ubuntu, etc.), not from PowerShell on the Windows host.
- Keep the project and `.ax/` index on the **Linux filesystem** (`~/…`), not under `/mnt/c/…`, for reliable SQLite locking.
- Windows and WSL should use **separate** `.ax/` directories if you work on the same checkout from both sides.

## Uninstall

```bash
ax uninstall          # remove MCP config from agents
ax uninit [path]      # remove .ax/ from a project
```
