---
title: Your First Graph
description: Build an index and run your first queries against it.
---

Once ax is installed, building and exploring a graph takes a few commands.

## Index a project

```bash
cd your-project
ax init
```

`ax init` creates the `.ax/` directory and builds the full graph in the same step — one command, done. From there a native file watcher keeps the index in sync on every change, so you rarely need to rebuild by hand. When you do want to:

```bash
ax index          # full re-index
ax sync           # incremental update of changed files
```

## Check it worked

```bash
ax status
```

This reports the node/edge/file counts, the active SQLite backend, and the journal mode — a quick health check that the index is ready.

## Run a query

Reach for `ax explore` first — a natural-language question or a bag of symbol names returns the relevant source plus the call paths between those symbols in a single shot (the same output the `ax_explore` tool gives your agent):

```bash
ax explore "how does login work"
```

For narrower, scriptable lookups there are focused commands:

```bash
ax query UserService          # find symbols by name
ax callers handleRequest      # what calls a function
ax callees handleRequest      # what a function calls
ax impact AuthMiddleware      # what a change would affect
```

These four each accept `--json` for machine-readable output. See the full [CLI reference](/ax/reference/cli/).

## Hand it to your agent

With a `.ax/` directory present and an agent configured (see [Installation](/ax/getting-started/installation/)), your agent uses the [MCP tools](/ax/reference/mcp-server/) automatically — no extra step.
