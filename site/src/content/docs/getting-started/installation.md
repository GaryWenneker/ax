---
title: Installation
description: Build current ax from main or install a published release, then configure your AI coding agents.
---

## Source and download versions

The website documents the CLI version in the current `main` source. The download installer and `ax upgrade` install published GitHub releases. A source build does not need a release tag; a download does need published binaries.

## Build from main

Use a new checkout to preserve any changes in your existing project:

```bash
git clone --branch main --single-branch https://github.com/GaryWenneker/ax.git ax-main
cd ax-main
git fetch origin main
test "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" || exit 1
bash scripts/reinstall-cli.sh
```

On macOS, the script builds `target-dev/release/ax` and installs a PATH shim at `~/.local/bin/ax`. Verify both binaries against the version in `crates/ax-cli/Cargo.toml`:

```bash
./target-dev/release/ax --version
~/.local/bin/ax --version
type -a ax
```

Both versions must match the source manifest. If your terminal resolves a different installation, refresh its command cache with `hash -r` and check again. Configure your IDE's MCP command to use this checkout's `target-dev/release/ax`, then restart its MCP server and any running `ax web` process to load the new build.

## Published downloads

**Latest published release:** [getax.wenneker.io/releases/latest.txt](https://getax.wenneker.io/releases/latest.txt). Install scripts and `ax upgrade` resolve the published tag independently of your local source checkout. Check your install:

```bash
ax version
# ax 7.1.0
```

Pin a specific published release with `AX_VERSION` when running `install.sh` / `install.ps1`. Confirm that the tag and your platform binary exist on [GitHub Releases](https://github.com/GaryWenneker/ax/releases) before pinning it.

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

The installer connects agents, then discovers projects under your home directory and runs `ax init` on each one. You do not have to initialize them yourself. Set `AX_SKIP_PROJECT_INIT=1` to install the binary only.

### From the Command Center

Run `ax web` and open **Settings → IDEs & agents**.

![Settings — IDEs and coding agents found on this machine, with Connect and Disconnect](/screenshots/cc-settings.png)

It lists every IDE and agent ax supports (Claude Code, Cursor, Codex CLI, opencode, Hermes, Gemini CLI, Antigravity, Kiro, VS Code, Windsurf, Zed, Continue) as **Connected**, **Found** (installed, ax not connected yet), or **Not found**. "Installed" means the CLI is on PATH or the app (or, for Continue, the extension) is present; a leftover config folder does not count.

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

## 4. Projects are initialized for you

Install and `ax upgrade` already run:

```bash
ax init --all
```

That walks four levels under your home directory and initializes every git repository, project manifest, or existing ax project it finds. It does not ask questions. A project that already has `.ax/ax.db` gets an incremental sync. Run the same command again any time.

To initialize one directory yourself, and to choose stacks and IDEs in a menu:

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
