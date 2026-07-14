---
title: Get Started
description: Get up and running with ax v2.1.7 in seconds.
---

Install **ax v2.1.7** (or newer from [latest.txt](https://getax.wenneker.io/releases/latest.txt)) — knowledge graph, memory vault, policy engine, and Command Center in one binary.

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

## 3. Initialize each project

```bash
cd your-project
ax init
```

Creates `.ax/`, builds the knowledge graph, installs git hooks (sync, ship evaluate, memory capture), and offers the agent installer. Your agent uses ax tools automatically when `.ax/` exists.

## 4. Optional — open Command Center

```bash
ax web --open
```

![Command Center — completed quality gate pipeline with Index, TIA, Tests, Sonar, and Policy steps](/screenshots/cc-ship-full.png)

Browse the graph, edit policy, view token savings, and manage SonarQube from the local dashboard.

Next: [Your First Graph](/getting-started/your-first-graph/), [Memory vault](/guides/memory/), or full [Installation](/getting-started/installation/) options.
