---
title: Installation
description: Install ax from the shell, then run ax install and ax init in your project.
---

Install ax from your terminal. The shell installer downloads the published binary for your operating system and CPU. That is the regular install. You do not clone the repository or build ax for this.

## 1. Install with the shell

ax is a **native Rust binary**. No Node.js and no Rust toolchain are required.

The homepage copies the same shell command. Open **Other platforms** there to choose macOS, Linux, Windows, or WSL2.

![Ax homepage with the native download button and platform selector](/screenshots/home-install-desktop.png)

[View the mobile installer](/screenshots/home-install-mobile.png).

### macOS

Run in Terminal on Apple Silicon or Intel Macs:

```bash
curl -fsSL https://getax.wenneker.io/install.sh | sh
```

Open a new Terminal window afterwards. The installer adds `~/.local/bin` to your zsh startup file (`${ZDOTDIR:-$HOME}/.zshrc`). Existing settings stay, and reinstalling does not duplicate that block. An installer running through `curl | sh` cannot change the terminal that launched it. If that window still says `command not found: ax`, run the `export PATH=...` line the installer printed, or open a new window.

If the website serves an older installer, use the current repository copy. It still installs a published release. It does not build source:

```bash
curl -fsSL https://raw.githubusercontent.com/GaryWenneker/ax/main/install.sh | sh
```

For an existing standard install that reports `command not found: ax`, verify the binary and repair your zsh PATH:

```zsh
"$HOME/.local/bin/ax" version
ax_shell_rc="${ZDOTDIR:-$HOME}/.zshrc"
ax_path_line='export PATH="$HOME/.local/bin:$PATH"'
grep -Fqx "$ax_path_line" "$ax_shell_rc" 2>/dev/null ||
  printf '\n%s\n' "$ax_path_line" >> "$ax_shell_rc"
export PATH="$HOME/.local/bin:$PATH"
rehash
command -v ax
ax version
```

If you downloaded an archive manually, `./ax version` runs the executable from its extracted directory. `ax` alone searches PATH. Executable files can appear red in a terminal listing; their color is not a diagnostic error. If running the full path returns an error, diagnose that error before changing PATH. The PATH steps do not address an incompatible architecture or a blocked binary.

### Linux

Run in your Linux shell:

```bash
curl -fsSL https://getax.wenneker.io/install.sh | sh
```

For Bash, the installer updates `.bashrc` and the first applicable login file (`.bash_profile`, `.bash_login`, or `.profile`). For sh/dash it updates `.profile`. Other shells receive a manual setup notice. A startup-file write failure leaves the verified binary installed and prints recovery guidance instead of claiming that future terminals are configured. `AX_BIN_DIR` values containing a colon or newline are rejected before installation because these characters are separators in PATH or the install-target list.

### WSL2

Open your WSL2 Linux terminal and install the Linux binary inside that distribution:

```bash
curl -fsSL https://getax.wenneker.io/install.sh | sh
```

Windows browsers identify Windows, rather than your WSL distribution. Select **WSL2** in the homepage installer when this is your development environment.

### Windows

Run in PowerShell:

```powershell
irm https://getax.wenneker.io/install.ps1 | iex
```

Open a new PowerShell window afterwards so `ax` is on PATH.

## 2. Run ax install

Connect ax to the editor or agent you already use:

```bash
ax install
```

This is the step that wires ax in. The command:

- Detects **Claude Code**, **Cursor**, **Codex CLI**, **opencode**, **Hermes Agent**, **Gemini CLI**, **Antigravity IDE**, and **Kiro**.
- Writes each agent's MCP config (`ax serve --mcp`).
- Adds a marker-fenced ax section to agent instruction files where applicable (`CLAUDE.md` / `AGENTS.md` / `GEMINI.md`). Removed cleanly by `ax uninstall`.
- Creates `~/.ax/config.json` with an empty scaffold for [global index defaults](/getting-started/configuration/#global-config-axconfigjson) if the file doesn't exist yet.

`ax install` connects your agent. It does not set up a project. After `ax install`, run `ax init` in each directory where you want to use ax.

Restart your agent after this so it loads the MCP config.

### From the Command Center

Run `ax web` and open **Settings → IDEs & agents**.

![Settings — IDEs and coding agents found on this machine, with Connect and Disconnect](/screenshots/cc-settings-dark.png)

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

## 3. Run ax init

After `ax install`, run `ax init` in each directory where you want to use ax:

```bash
cd your-project
ax init
```

Do this again in every other directory where you want ax. You do not run `ax init --all`.

`ax init` creates `.ax/` (SQLite index + lock file) and runs a full index in one step. It also scaffolds `.ax/policy/rules/` and `.ax/policy/skills/` for the [policy engine](/guides/policy-engine/). It asks which stacks and which IDEs to set up. Both choices are saved in `ax.json` and pre-checked on the next run. Unchecking an IDE you chose before removes ax from it. See [`ax init`](/reference/cli/#ax-init-path) for details.

## Supported platforms

Every release ships **six** prebuilt binaries. The shell installer picks the match for your OS and CPU.

| Platform | Architectures | Install |
|---|---|---|
| Windows | x64, arm64 | PowerShell installer (`install.ps1`) or [getax CDN](https://getax.wenneker.io/releases/) |
| macOS | x64 (Intel), arm64 (Apple Silicon) | shell installer (`install.sh`) or getax CDN |
| Linux | x64, arm64 | shell installer or getax CDN |
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

## Build ax yourself

Building from source is for when you need the current `main` tree instead of the published binary. A beginning install does not need this.

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

### Published downloads

**Latest published release:** [getax.wenneker.io/releases/latest.txt](https://getax.wenneker.io/releases/latest.txt). Install scripts and `ax upgrade` resolve the published tag independently of your local source checkout. Check your install:

```bash
ax version
# ax 7.1.0
```

Pin a specific published release with `AX_VERSION` when running `install.sh` / `install.ps1`. Confirm that the tag and your platform binary exist on [GitHub Releases](https://github.com/GaryWenneker/ax/releases) before pinning it.

### From source with Cargo

Requires Rust 1.75+:

```bash
cargo install --git https://github.com/GaryWenneker/ax ax-cli --force
```

### Via npm

Optional. This downloads the matching release binary. It does not build ax:

```bash
npx @garywenneker/ax
# or globally:
npm install -g @garywenneker/ax
```

The npm package is a thin launcher: it fetches the prebuilt `ax` binary from [getax.wenneker.io](https://getax.wenneker.io/releases/) (public CDN) with GitHub Releases as fallback. See [npm README](https://github.com/GaryWenneker/ax/blob/main/docs/npm/README.md) for publish details.

### Local dev upgrade (maintainers)

After building from source, package and install without publishing:

```powershell
# Windows — from repo root
.\scripts\ship-local.ps1          # build + dist/ax-win32-x64.zip
ax upgrade --local                # install from dist/
```

Set `AX_UPGRADE_ARCHIVE` to point at a specific zip if needed.

**Upgrading an existing install?** See [Troubleshooting — Existing installations](/troubleshooting/#existing-installations).
