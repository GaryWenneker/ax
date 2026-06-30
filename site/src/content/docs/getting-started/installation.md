---
title: Installation
description: Install ax and configure your AI coding agents.
---

## 1. Install the CLI

ax is a **native Rust binary** — no Node.js required for normal use.

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/GaryWenneker/ax/main/install.sh | sh

# Windows (PowerShell)
irm https://raw.githubusercontent.com/GaryWenneker/ax/main/install.ps1 | iex
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

The npm package is a thin launcher: it fetches the prebuilt `ax` binary from [GitHub Releases](https://github.com/GaryWenneker/ax/releases). See [npm README](https://github.com/GaryWenneker/ax/blob/main/docs/npm/README.md) for publish details.

## 2. Wire up your agent(s)

```bash
ax install
```

The installer:

- Detects **Claude Code**, **Cursor**, **Codex CLI**, **opencode**, **Hermes Agent**, **Gemini CLI**, **Antigravity IDE**, and **Kiro**.
- Writes each agent's MCP config (`ax serve --mcp`).
- Adds a marker-fenced ax section to agent instruction files where applicable (`CLAUDE.md` / `AGENTS.md` / `GEMINI.md`). Removed cleanly by `ax uninstall`.

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

`ax init` creates `.ax/` (SQLite index + lock file) and runs a full index in one step.

## Supported platforms

Prebuilt binaries ship for:

| Platform | Architectures | Install |
|---|---|---|
| Windows | x64, arm64 | PowerShell installer, npm, or GitHub release |
| macOS | x64, arm64 | shell installer, npm, or GitHub release |
| Linux | x64, arm64 | shell installer, npm, or GitHub release |

## Uninstall

```bash
ax uninstall          # remove MCP config from agents
ax uninit [path]      # remove .ax/ from a project
```
