---
title: Get Started
description: Get up and running with ax v2.0.0 in seconds.
---

Install **ax v2.0.0** (or newer from [latest.txt](https://getax.wenneker.io/releases/latest.txt)) — code graph + policy engine in one binary.

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
```

Configures Claude Code, Cursor, Codex CLI, opencode, Hermes Agent, Gemini CLI, Antigravity IDE, and Kiro with the ax MCP server. This step does **not** index code.

## 3. Initialize each project

```bash
cd your-project
ax init
```

Creates `.ax/` and builds the knowledge graph. Your agent uses ax tools automatically when `.ax/` exists.

Next: [Your First Graph](/getting-started/your-first-graph/), or full [Installation](/getting-started/installation/) options.
