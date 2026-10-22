---
title: Get Started
description: Get up and running with ax using a published download or the current main source.
---

Install the published binary with the shell below, then run `ax install` and `ax init` in your project. Knowledge graph, memory vault, policy engine, MCP Logging / Quality, and Command Center run in one binary. Building from source is a separate path at the bottom of [Installation](/getting-started/installation/#build-from-main).

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

Configures Claude Code, Cursor, Codex CLI, opencode, Hermes Agent, Gemini CLI, Antigravity IDE, and Kiro with the ax MCP server. This step does **not** index code. After `ax install`, run `ax init` in each directory where you want to use ax.

## 3. Initialize your project

```bash
cd your-project && ax init
```

After `ax install`, run `ax init` in each directory where you want to use ax. Repeat it for every other directory. You do not need `ax init --all`.

`ax init` creates `.ax/`, builds the knowledge graph, and installs git hooks (sync, ship evaluate, memory capture). It asks which stacks and which IDEs to set up. Your agent uses ax tools automatically when `.ax/` exists.

## 4. Optional — open Command Center

```bash
ax web --open
```

![Command Center — Rules page](/screenshots/cc-policy-rules-dark.png)

Browse the graph, edit policy, view token savings, and read MCP logs from the local dashboard.

Next: [Your First Graph](/getting-started/your-first-graph/), [Memory vault](/guides/memory/), or full [Installation](/getting-started/installation/) options.
