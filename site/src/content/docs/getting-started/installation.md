---
title: Installation
description: Install ax v5.1.0 and configure your AI coding agents.
---

## Current version

**Latest release: v5.1.0** — install scripts and `ax upgrade` resolve the tag from [getax.wenneker.io/releases/latest.txt](https://getax.wenneker.io/releases/latest.txt). Check your install:

```bash
ax version
# ax 5.1.0
```

Pin a specific release with `AX_VERSION=v5.1.0` when running `install.sh` / `install.ps1`.

### Local dev upgrade (maintainers)

After building from source, package and install without publishing:

```powershell
# Windows — from repo root
.\scripts\ship-local.ps1          # build + dist/ax-win32-x64.zip
ax upgrade --local                # install from dist/
```

Set `AX_UPGRADE_ARCHIVE` to point at a specific zip if needed.

**Upgrading an existing install?** See [Troubleshooting — Existing installations](/troubleshooting/#existing-installations).

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

### From the Command Center

Run `ax web` and open **Settings → IDEs & agents**. It lists every IDE and agent ax supports (Claude Code, Cursor, Codex CLI, opencode, Hermes, Gemini CLI, Antigravity, Kiro, VS Code, Windsurf, Zed, Continue) as **Connected**, **Found** (installed, ax not connected yet), or **Not found**. "Installed" means the CLI is on PATH or the app (or, for Continue, the extension) is present; a leftover config folder does not count.

- **Connect** writes the same config as `ax install --target=<id>`. **Disconnect** removes it, like `ax uninstall`.
- **Connect all found** connects every IDE that is installed but not connected yet.
- After each action it shows the files it changed and any follow-up, such as reloading the VS Code window.
- It works the same on macOS, Linux, and Windows. Only the browser on your own machine can use it; it is off in read-only mode and in share sessions.

### Non-interactive (scripting / CI)

```bash
ax install --yes                              # detected agents, no prompts
ax install --yes --all                        # every supported agent
ax install --yes --target cursor --target claude   # specific agents
ax install --yes --path ~/code/my-app         # workspace MCP files for that project
```

| Flag | What it does | Default |
|---|---|---|
| `--yes` | Skip prompts, install detected agents | prompt every step |
| `--all` | Install every supported agent, not only detected ones | detected only |
| `--target <id>` | Wire one agent (repeatable), e.g. `cursor`, `vscode` | — |
| `--path <dir>` | Project root for workspace MCP files | current directory |

## 3. Restart your agent

Restart your agent so the MCP server config loads.

## 4. Initialize projects

```bash
cd your-project
ax init
```

Creates `.ax/` (SQLite index + lock file) and runs a full index in one step. It also scaffolds `.ax/policy/rules/` and `.ax/policy/skills/` for the [policy engine](/guides/policy-engine/).

`ax init` asks which stacks and which IDEs to set up. Both choices are saved in `ax.json` and pre-checked on the next run. Unchecking an IDE you chose before removes ax from it. See [`ax init`](/reference/cli/#ax-init-path) for details.

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
