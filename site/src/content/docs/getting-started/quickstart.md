---
title: Get Started
description: Install ax, index a project, connect an agent over MCP and optionally connect ax-admin.
---

Install `ax`, connect your agent over MCP, and run `ax init` in your project. `ax` is headless: the knowledge graph, memory vault, policy engine and Logging / Quality (MCP and CLI) run in one binary, and you reach them through the CLI and the MCP server. The visual interface is a separate application, [`ax-admin`](/guides/remote-admin/). Building from source is a separate path at the bottom of [Installation](/getting-started/installation/#build-from-main).

## 1. Install the CLI

No Node.js required. Pick the line for your system.

**Linux** — the headless build (x86_64 and arm64):

```bash
curl -fsSL https://getax.wenneker.io/install-slim.sh | sh
```

**macOS and Windows** — there is no released headless build for these systems yet, so you install the full ax. Its CLI and MCP server are the same commands. The full ax, with its built-in Command Center, is being phased out; see [Headless ax and ax-admin](/guides/phase-out/).

```bash
# macOS
curl -fsSL https://getax.wenneker.io/install.sh | sh

# Windows (PowerShell)
irm https://getax.wenneker.io/install.ps1 | iex
```

Have Node? `npx @garywenneker/ax` downloads the native binary of the full ax for your platform. Open a **new terminal** after install so `PATH` updates.

**WSL2:** run the Linux installer inside WSL (not PowerShell). See [Installation](/getting-started/installation/#supported-platforms).

## 2. Connect your agent over MCP

```bash
ax install
ax install --yes          # non-interactive
```

Configures Claude Code, Cursor, Codex CLI, opencode, Hermes Agent, Gemini CLI, Antigravity IDE, and Kiro with the ax MCP server (`ax serve --mcp`). This step does **not** index code. Restart your agent afterwards so it loads the MCP config.

The headless installer (`install-slim.sh`) does not configure agents by itself; `ax install` is the step that does. To write the MCP entry by hand, see [Integrations — Manual setup](/reference/integrations/#manual-setup). An agent on another machine connects through [Remote MCP](/guides/remote-mcp/).

## 3. Initialize your project

```bash
cd your-project && ax init
```

After `ax install`, run `ax init` in each directory where you want to use ax. Repeat it for every other directory. You do not need `ax init --all`.

`ax init` creates `.ax/`, builds the knowledge graph, and installs git hooks (sync, ship evaluate, memory capture). It asks which stacks and which IDEs to set up. Your agent uses ax tools automatically when `.ax/` exists.

## 4. Optional — connect ax-admin

`ax-admin` is the visual interface. It is a separate application that runs on your own computer and connects to an Ax through its remote MCP address.

```bash
# macOS / Linux
curl -fsSL https://getax.wenneker.io/install-admin.sh | sh

# Windows (PowerShell)
irm https://getax.wenneker.io/install-admin.ps1 | iex
```

`ax-admin` needs two things from the Ax it connects to:

- `ax mcp remote serve` running for the project. [Remote MCP](/guides/remote-mcp/) covers its configuration and how to make it reachable.
- The project MCP address and an API key (or a browser sign-in). [ax-admin](/guides/remote-admin/) covers the connect screen and the admin grant that the Operations, Files, Usage and Logging pages need.

Then start it with `ax-admin` and enter the address and key. You can search the graph, browse rules and skills, search memory, view usage and read MCP logs from there.

Next: [Your First Graph](/getting-started/your-first-graph/), [Memory vault](/guides/memory/), or full [Installation](/getting-started/installation/) options.
