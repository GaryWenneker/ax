---
title: Get Started
description: Get up and running with ax v7.1.0 in seconds.
---

Install **ax v7.1.0** (or newer from [latest.txt](https://getax.wenneker.io/releases/latest.txt)) — knowledge graph, memory vault, policy engine, MCP Logging / Quality, and Command Center in one binary.

## 1. Install the CLI

No Node.js required — pick one:

```bash
# macOS / Linux
curl -fsSL https://getax.wenneker.io/install.sh | sh

# Windows (PowerShell)
irm https://getax.wenneker.io/install.ps1 | iex
```

Have Node? `npx @garywenneker/ax` downloads the native binary for your platform. Open a **new terminal** after install so `PATH` updates.

**WSL2:** run the Linux command above inside WSL (not PowerShell). See [Installation](/getting-started/installation/#supported-platforms).

## 2. Wire up your agent(s)

```bash
ax install
ax install --yes          # non-interactive
```

Configures Claude Code, Cursor, Codex CLI, opencode, Hermes Agent, Gemini CLI, Antigravity IDE, and Kiro with the ax MCP server. This step does **not** index code.

## 3. Projects initialize themselves

Installing or upgrading ax runs `ax init --all`. That finds projects under your home directory (four levels; git repos, project manifests, and existing ax projects) and initializes each one without questions. Run the same command again to pick up a new checkout.

```bash
ax init --all
cd your-project && ax init   # one directory, with the stack and IDE questions
```

Creates `.ax/`, builds the knowledge graph, and installs git hooks (sync, ship evaluate, memory capture). Your agent uses ax tools automatically when `.ax/` exists.

## 4. Optional — open Command Center

```bash
ax web --open
```

![Command Center — Rules page](/screenshots/cc-policy-rules.png)

Browse the graph, edit policy, view token savings, and read MCP logs from the local dashboard.

Next: [Your First Graph](/getting-started/your-first-graph/), [Memory vault](/guides/memory/), or full [Installation](/getting-started/installation/) options.
