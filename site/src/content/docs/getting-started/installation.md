---
title: Installation
description: Install headless ax on Linux, ax-admin on any system, and the full ax on macOS and Windows; then run ax install and ax init in your project.
---

Ax is two programs. `ax` is headless: the CLI and the MCP server. `ax-admin` is the visual interface, a separate application with its own installer. Each installer downloads a published binary for your operating system and CPU. You do not clone the repository or build ax for this.

ax is a **native Rust binary**. No Node.js and no Rust toolchain are required.

| You have | Install | Section |
|---|---|---|
| Linux, a container, a sandbox or CI | headless `ax` with `install-slim.sh` | [1](#1-install-ax-headless-on-linux) |
| Any system, and you want the visual interface | `ax-admin` with `install-admin.sh` / `install-admin.ps1` | [2](#2-install-ax-admin-on-windows-macos-and-linux) |
| macOS or Windows | the full ax with `install.sh` / `install.ps1` / Homebrew | [3](#3-macos-and-windows-ax-today) |

<a id="1-install-with-the-shell"></a>
<a id="linux-sandboxes-and-containers-headless"></a>

## 1. Install ax headless on Linux

The headless build is `ax` going forward: CLI and MCP server, without a built-in interface, desktop client or self-update. It is released for Linux x86_64 and arm64, from v10.1.1. Use it on a workstation, a server, a Vercel Sandbox, an Azure Container Apps job, a CI runner or another machine without a screen.

```bash
curl -fsSL https://getax.wenneker.io/install-slim.sh | sh
```

The installer puts one static binary in `/usr/local/bin` as root, otherwise in `~/.local/bin`. It does not configure agents and does not create an `~/.ax` install tree. The headless build never updates itself: run the installer again to upgrade. It refuses to replace a full ax at the same path unless you pass `--force`. Options such as `--version`, `--bin-dir` and `--uninstall` are in [Headless Linux build](/guides/headless-linux/#install).

### Containers and the cloud

To run it in the cloud without installing anything locally, deploy it as a hosted remote MCP server for one repository:

<div class="ax-deploy-row">
<a class="ax-deploy" data-provider="azure" href="https://portal.azure.com/#create/Microsoft.Template/uri/https%3A%2F%2Fgetax.wenneker.io%2Fdeploy%2Fazure-slim-server.json" target="_blank" rel="noopener noreferrer">Deploy to Azure</a>
<a class="ax-deploy" data-provider="render" href="https://render.com/deploy?repo=https://github.com/GaryWenneker/ax" target="_blank" rel="noopener noreferrer">Deploy to Render</a>
<a class="ax-deploy" data-provider="koyeb" href="https://app.koyeb.com/deploy?type=docker&amp;name=ax-mcp&amp;image=ghcr.io%2Fgarywenneker%2Fax-slim%3Alatest-server&amp;ports=8080%3Bhttp%3B%2F&amp;env%5BPORT%5D=8080&amp;env%5BAX_REPO_URL%5D=https%3A%2F%2Fgithub.com%2FOWNER%2FREPO&amp;env%5BAX_API_KEY%5D=CHANGE_ME&amp;env%5BAX_PROJECT_ID%5D=project" target="_blank" rel="noopener noreferrer">Deploy to Koyeb</a>
</div>

See [Deploy to the cloud](/guides/headless-linux/#deploy-to-the-cloud) for what each button creates and the API key it needs, and [Headless Linux build](/guides/headless-linux/) for options, container images, an Azure job that indexes once, and Vercel Sandbox.

<a id="ax-admin-separate-application"></a>

## 2. Install ax-admin on Windows, macOS and Linux

`ax-admin` is the visual interface. It manages a remote Ax, including the headless build, from your own computer. It has its own installer and does not need `ax` on that computer. It is released for Windows, macOS and Linux, on x64 and arm64, from v10.1.2.

macOS and Linux:

```bash
curl -fsSL https://getax.wenneker.io/install-admin.sh | sh
```

Windows (PowerShell):

```powershell
irm https://getax.wenneker.io/install-admin.ps1 | iex
```

`ax-admin` connects to an Ax through its remote MCP address, so that Ax must run `ax mcp remote serve`. See [ax-admin](/guides/remote-admin/) for installer options, the connect screen and the pages, and [Remote MCP](/guides/remote-mcp/) for the server side.

## 3. macOS and Windows: ax today

There is no released headless build for macOS or Windows yet. On those systems you install the full ax with `install.sh`, `install.ps1` or Homebrew. Its CLI and MCP server are the same commands as in the headless build, so everything in this documentation about the CLI and MCP applies.

:::caution[The full ax is being phased out]
The installers in this section install the full ax, with the built-in Command Center. It is being phased out. The headless build becomes ax on every platform, and `ax-admin` becomes the only visual interface. No date is set. See [Headless ax and ax-admin](/guides/phase-out/).
:::

The homepage copies the same shell command. Open **Other platforms** there to choose macOS, Linux, Windows, or WSL2.

![Ax homepage with the native download button and platform selector](/screenshots/home-install-desktop.png)

[View the mobile installer](/screenshots/home-install-mobile.png).

### macOS

#### Homebrew (optional)

```bash
brew tap GaryWenneker/tap
brew trust GaryWenneker/tap   # Homebrew 7+
brew install ax
```

That installs the same GitHub release archives as the shell installer (Apple Silicon or Intel). Project data stays in each project's `.ax/` directory and user data in `~/.ax/`. Upgrade with `brew upgrade ax`. Starting with the release that includes the Homebrew Cellar guard in the binary, Homebrew-managed installs refuse `ax upgrade` so the Cellar binary is not overwritten. Uninstall with `brew uninstall ax` — that removes only the Homebrew binary. Tap source: [`GaryWenneker/homebrew-tap`](https://github.com/GaryWenneker/homebrew-tap).

#### Shell installer

Run in Terminal on Apple Silicon or Intel Macs:

```bash
curl -fsSL https://getax.wenneker.io/install.sh | sh
```

Open a new Terminal window afterwards. The installer adds `~/.local/bin` to your zsh startup file (`${ZDOTDIR:-$HOME}/.zshrc`). Existing settings stay, and reinstalling does not duplicate that block. An installer running through `curl | sh` cannot change the terminal that launched it. If that window still says `command not found: ax`, run the `export PATH=...` line the installer printed, or open a new window.

The shell installer first resolves the release, downloads it, checks it against the release's `SHA256SUMS` and unpacks it next to your current install. The checksum list comes from GitHub, also when the archive came from the getax mirror; the mirror's list is used only when GitHub is unreachable. An archive with a missing or wrong checksum is never installed. Only when that worked does it stop running `ax` processes, clear stuck upgrade leftovers under `~/.ax` and swap the new program in, so a failed download (offline, proxy, rate limit) leaves your working `ax` and its running MCP servers alone. It **self-heals** along the way: it drops an invalid `AX_VERSION` pin and retries GitHub without the token when GitHub rejects or rate-limits it. It never changes a stored `GITHUB_TOKEN` / `GH_TOKEN`; when GitHub rejects the token it prints the command to remove it yourself. Re-run the same install command after a failed upgrade — no separate env cleanup.

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

### Windows

Run in PowerShell:

```powershell
irm https://getax.wenneker.io/install.ps1 | iex
```

Open a new PowerShell window afterwards so `ax` is on PATH.

#### Chocolatey (optional)

Once Chocolatey Community moderation approves the package:

```powershell
choco install ax-agent -y
choco upgrade ax-agent -y
```

That installs the same GitHub release zips into `%LOCALAPPDATA%\ax\current` as `install.ps1`. Open a new shell, then run `ax init` in your project.

Maintainers package from macOS without installing Chocolatey locally: GitHub Actions workflow `chocolatey.yml` (`gh workflow run chocolatey.yml -f tag=vX.Y.Z`) runs `choco pack` / install-test on `windows-latest`, and `choco push` when `-f push=true` with the `CHOCOLATEY_API_KEY` repository secret.

The Windows installer first resolves the release, downloads it, checks it against the release's `SHA256SUMS` (from GitHub, or the getax mirror only when GitHub is unreachable) and unpacks it next to `%LOCALAPPDATA%\ax\current`. Only when that worked does it stop running `ax` processes, clear stuck `upgrade-status.txt` / `upgrade-staging-*` leftovers and swap the new program in; if the swap fails, the previous install is put back. A failed download leaves your working `ax` on PATH and its running MCP servers alone. It **self-heals** along the way: it drops an invalid `AX_VERSION` pin and retries GitHub without `GITHUB_TOKEN` / `GH_TOKEN` when GitHub rejects or rate-limits the token. It never changes your User or Machine environment variables; when GitHub rejects the token it prints the command to remove it yourself. You do not need a separate environment cleanup script on each machine. Re-run the same `irm … | iex` command after a failed upgrade or “Access is denied” park message.

If the website still serves an older installer, use the repository copy (still installs a published release):

```powershell
irm https://raw.githubusercontent.com/GaryWenneker/ax/main/install.ps1 | iex
```

<a id="linux"></a>

### Full ax on Linux

The headless build in [section 1](#1-install-ax-headless-on-linux) is the ax to install on Linux. Releases still ship the full ax for Linux as well, with the installers below.

#### Homebrew on Linux (optional)

```bash
brew tap GaryWenneker/tap
brew trust GaryWenneker/tap   # Homebrew 7+
brew install ax
```

Supported archives are `ax-linux-x64.tar.gz` and `ax-linux-arm64.tar.gz`. Upgrade with `brew upgrade ax`.

#### Shell installer

Run in your Linux shell:

```bash
curl -fsSL https://getax.wenneker.io/install.sh | sh
```

For Bash, the installer updates `.bashrc` and the first applicable login file (`.bash_profile`, `.bash_login`, or `.profile`). For sh/dash it updates `.profile`. Other shells receive a manual setup notice. A startup-file write failure leaves the verified binary installed and prints recovery guidance instead of claiming that future terminals are configured. `AX_BIN_DIR` values containing a colon or newline are rejected before installation because these characters are separators in PATH or the install-target list.

### WSL2

Open your WSL2 Linux terminal and install the Linux binary inside that distribution. This is the full ax for Linux:

```bash
curl -fsSL https://getax.wenneker.io/install.sh | sh
```

Windows browsers identify Windows, rather than your WSL distribution. Select **WSL2** in the homepage installer when this is your development environment.

<a id="2-run-ax-install"></a>

## 4. Run ax install

Connect ax to the editor or agent you already use:

```bash
ax install
```

This is the step that wires ax in. It is the same command in the headless build and in the full ax. `install-slim.sh` only installs the binary, so on Linux you run `ax install` yourself. The command:

- Detects **Claude Code**, **Cursor**, **Codex CLI**, **opencode**, **Hermes Agent**, **Gemini CLI**, **Antigravity IDE**, and **Kiro**.
- Writes each agent's MCP config (`ax serve --mcp`).
- Adds a marker-fenced ax section to agent instruction files where applicable (`CLAUDE.md` / `AGENTS.md` / `GEMINI.md`). Removed cleanly by `ax uninstall`.
- Creates `~/.ax/config.json` with an empty scaffold for [global index defaults](/getting-started/configuration/#global-config-axconfigjson) if the file doesn't exist yet.

`ax install` connects your agent. It does not set up a project. After `ax install`, run `ax init` in each directory where you want to use ax.

Restart your agent after this so it loads the MCP config.

### CLI-only (no MCP)

If your agent must not load MCP — or you want instruction files without server configs:

```bash
ax install --mode cli-only
```

That seeds Cursor, VS Code/Copilot, Windsurf, Continue, Claude, Gemini, Cline/Antigravity, and shared `AGENTS.md` rules/skills. It does **not** write MCP JSON, plugins, daemons, or IDE hooks. See [CLI-only mode](/guides/cli-only/).

<a id="from-the-command-center"></a>

### One agent at a time

To connect one IDE or agent, name it with `--target`:

```bash
ax install --target cursor
ax install --target vscode
```

Ids are case-insensitive and the flag can be repeated. `ax install --all` configures every supported agent, not only the detected ones. `ax uninstall` removes the ax entries from the agent MCP configuration files again. After each run the command reports what it wrote; reload the IDE window or restart the agent so it picks up the change. See [`ax install`](/reference/cli/#ax--ax-install) for every flag and [Integrations](/reference/integrations/) for the file each agent uses.

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

<a id="3-run-ax-init"></a>

## 5. Run ax init

After `ax install`, run `ax init` in each directory where you want to use ax:

```bash
cd your-project
ax init
```

Do this again in every other directory where you want ax. You do not run `ax init --all`.

`ax init` creates `.ax/` (SQLite index + lock file) and runs a full index in one step. It also scaffolds `.ax/policy/rules/` and `.ax/policy/skills/` for the [policy engine](/guides/policy-engine/). It asks which stacks and which IDEs to set up. Both choices are saved in `ax.json` and pre-checked on the next run. Unchecking an IDE you chose before removes ax from it. See [`ax init`](/reference/cli/#ax-init-path) for details.

## Supported platforms

From v10.1.1, releases ship two headless Linux binaries (x64 and arm64) and container images. From v10.1.2, releases ship `ax-admin` for six platforms. Every release also still ships **six** prebuilt binaries of the full ax; its shell installer picks the match for your OS and CPU.

| Program and platform | Architectures | Install |
|---|---|---|
| ax, headless: Linux (workstations, servers, sandboxes, containers) | x64, arm64 | `install-slim.sh` or `ghcr.io/garywenneker/ax-slim` images ([guide](/guides/headless-linux/)) |
| ax-admin (separate application) | Windows, macOS, Linux; x64 and arm64 | `install-admin.sh` / `install-admin.ps1` or GitHub Releases ([guide](/guides/remote-admin/#install)) |
| Full ax: Windows | x64, arm64 | PowerShell installer (`install.ps1`) or [GitHub Releases](https://github.com/GaryWenneker/ax/releases) |
| Full ax: macOS | x64 (Intel), arm64 (Apple Silicon) | shell installer (`install.sh`), Homebrew or GitHub Releases |
| Full ax: Linux | x64, arm64 | shell installer (`install.sh`) or GitHub Releases |
| Full ax: WSL2 | x64, arm64 | **Use the Linux installer** inside WSL — `curl -fsSL https://getax.wenneker.io/install.sh \| sh` |

WSL2 notes:

- Run `install.sh` from a WSL shell (Ubuntu, etc.), not from PowerShell on the Windows host.
- Keep the project and `.ax/` index on the **Linux filesystem** (`~/…`), not under `/mnt/c/…`, for reliable SQLite locking.
- Windows and WSL should use **separate** `.ax/` directories if you work on the same checkout from both sides.

## Uninstall

```bash
ax uninstall          # remove MCP config from agents
ax uninit [path]      # remove .ax/ from a project (asks first; --yes in scripts)
```

To remove the headless binary itself, run its installer with `--uninstall`:

```bash
curl -fsSL https://getax.wenneker.io/install-slim.sh | sh -s -- --uninstall
```

`ax-admin` is removed the same way with its own installer; see [ax-admin](/guides/remote-admin/#install).

The full ax installer (`install.sh`) takes the same `--uninstall` flag. With `--purge` it also deletes your ax data in `~/.ax` after asking for confirmation (`--yes` skips the question when the script cannot ask, for example in CI). `--purge` only deletes a folder with an ax-specific name, such as `.ax`.

## Build ax yourself

Building from source is for when you need the current `main` tree instead of the published binary. A beginning install does not need this. The steps below build the full ax. To build the headless binary, see [Build a static Linux binary](/guides/headless-linux/#build-a-static-linux-binary).

## Source and download versions

The website documents the CLI version in the current `main` source. The download installers install published GitHub releases. So does `ax upgrade`, which only the full ax has; the headless build is upgraded by running `install-slim.sh` again. A source build does not need a release tag; a download does need published binaries.

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

Both versions must match the source manifest. If your terminal resolves a different installation, refresh its command cache with `hash -r` and check again. Configure your IDE's MCP command to use this checkout's `target-dev/release/ax`, then restart its MCP server to load the new build.

### Published downloads

**Latest published release:** [getax.wenneker.io/releases/latest.txt](https://getax.wenneker.io/releases/latest.txt). Install scripts, and `ax upgrade` in the full ax, resolve the published tag independently of your local source checkout. Check your install:

```bash
ax version
# ax 8.4.4
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

The npm package is a thin launcher: it fetches the prebuilt `ax` binary from [GitHub Releases](https://github.com/GaryWenneker/ax/releases), through getax.wenneker.io download links that redirect there. See [npm README](https://github.com/GaryWenneker/ax/blob/main/docs/npm/README.md) for publish details.

### Local dev upgrade (maintainers)

After building the full ax from source, package and install without publishing. `ax upgrade` is not part of the headless build:

```powershell
# Windows — from repo root
.\scripts\ship-local.ps1          # build + dist/ax-win32-x64.zip
ax upgrade --local                # install from dist/
```

Set `AX_UPGRADE_ARCHIVE` to point at a specific zip if needed.

**Upgrading an existing install?** See [Troubleshooting — Existing installations](/troubleshooting/#existing-installations).